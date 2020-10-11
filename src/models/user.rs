use chrono::prelude::*;
use diesel::insert_into;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use serde::Serialize;
use std::env;
use thiserror::Error;

use crate::schema::user;

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
const USER_COLUMNS: (user::id, user::name, user::is_admin) = (user::id, user::name, user::is_admin);

#[derive(Insertable)]
#[table_name = "user"]
struct DatabaseNewUser<'a> {
    pub name: &'a str,
    pub hashed_password: &'a str,
    pub is_admin: bool,
    pub creation_instant: NaiveDateTime,
    pub creation_user_id: Option<i32>,
}

pub struct NewUser<'a> {
    pub name: &'a str,
    pub password: &'a str,
    pub is_admin: bool,
    pub creation_instant: NaiveDateTime,
    pub creation_user_id: Option<i32>,
}

pub fn get_user_by_name(connection: &SqliteConnection, name: &str) -> QueryResult<User> {
    user::table
        .select(USER_COLUMNS)
        .filter(user::name.eq(name))
        .first(connection)
}

#[derive(Error, Debug)]
pub enum UserHashingError {
    #[error(transparent)]
    Database(#[from] diesel::result::Error),
    #[error(transparent)]
    Hash(#[from] argon2::Error),
}

pub enum PasswordMatched {
    UserDoesntExist,
    PasswordDoesntMatch,
    PasswordMatches(User),
}

pub fn check_matching_password(
    connection: &SqliteConnection,
    name: &str,
    password: &str,
) -> Result<PasswordMatched, UserHashingError> {
    match user::table
        .filter(user::name.eq(name))
        .select((user::id, user::name, user::hashed_password, user::is_admin))
        .first::<UserWithHashedPassword>(connection)
        .optional()?
    {
        Some(user) => Ok(
            if argon2::verify_encoded(&user.hashed_password, password.as_bytes())? {
                PasswordMatched::PasswordMatches(get_user_by_name(&connection, &name)?)
            } else {
                PasswordMatched::PasswordDoesntMatch
            },
        ),
        None => Ok(PasswordMatched::UserDoesntExist),
    }
}

pub fn insert_new_user(
    connection: &SqliteConnection,
    new_user: NewUser,
) -> Result<User, UserHashingError> {
    let config = argon2::Config::default();
    let hashed_password = argon2::hash_encoded(
        new_user.password.as_bytes(),
        env::var("SECRET_HASH_KEY")
            .expect("SECRET_HASH_KEY must be set")
            .as_bytes(),
        &config,
    )?;

    insert_into(user::table)
        .values(DatabaseNewUser {
            name: new_user.name,
            hashed_password: &hashed_password,
            is_admin: new_user.is_admin,
            creation_instant: new_user.creation_instant,
            creation_user_id: new_user.creation_user_id,
        })
        .execute(connection)?;

    Ok(get_user_by_name(connection, new_user.name)?)
}
