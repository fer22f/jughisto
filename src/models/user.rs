use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use serde::Serialize;
use std::env;
use thiserror::Error;

use crate::schema::user as user_column;
use crate::schema::user;
use crate::schema::user::dsl::user as user_table;

#[derive(Queryable)]
struct UserWithHashedPassword {
    pub id: i32,
    pub name: String,
    pub hashed_password: String,
    pub is_admin: bool,
}

#[derive(Queryable, Serialize)]
pub struct User {
    pub id: i32,
    pub name: String,
    pub is_admin: bool,
}
const USER_COLUMNS: (user_column::id, user_column::name, user_column::is_admin) =
    (user_column::id, user_column::name, user_column::is_admin);

#[derive(Insertable)]
#[table_name = "user"]
struct DatabaseNewUser<'a> {
    pub name: &'a str,
    pub hashed_password: &'a str,
    pub is_admin: bool,
}

pub struct NewUser<'a> {
    pub name: &'a str,
    pub password: &'a str,
    pub is_admin: bool,
}

pub fn get_user_by_name(connection: &SqliteConnection, name: &str) -> QueryResult<User> {
    Ok(user_table
        .select(USER_COLUMNS)
        .filter(user_column::name.eq(name))
        .first::<User>(connection)?)
}

#[derive(Error, Debug)]
pub enum UserHashingError {
    #[error(transparent)]
    Database(#[from] diesel::result::Error),
    #[error(transparent)]
    Hash(#[from] argon2::Error),
}

pub fn check_matching_password(
    connection: &SqliteConnection,
    name: &str,
    password: &str,
) -> Result<bool, UserHashingError> {
    let user = user_table
        .filter(user_column::name.eq(name))
        .first::<UserWithHashedPassword>(connection)?;
    Ok(argon2::verify_encoded(
        &user.hashed_password,
        password.as_bytes(),
    )?)
}

pub fn insert_new_user(
    connection: &SqliteConnection,
    new_user: NewUser,
) -> Result<User, UserHashingError> {
    let NewUser {
        name,
        password,
        is_admin,
    } = new_user;

    let config = argon2::Config::default();
    let hashed_password = argon2::hash_encoded(
        password.as_bytes(),
        env::var("SECRET_HASH_KEY")
            .expect("SECRET_HASH_KEY must be set")
            .as_bytes(),
        &config,
    )?;

    diesel::insert_into(user_table)
        .values(DatabaseNewUser {
            name,
            hashed_password: &hashed_password,
            is_admin,
        })
        .execute(connection)?;

    Ok(get_user_by_name(connection, name)?)
}

pub fn get_users(connection: &SqliteConnection) -> QueryResult<Vec<User>> {
    user_table.select(USER_COLUMNS).load::<User>(connection)
}
