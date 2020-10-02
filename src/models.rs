use serde::Serialize;
use super::schema::user;

#[derive(Queryable, Serialize)]
pub struct User {
    pub id: i32,
    pub name: String,
    pub hashed_password: String,
    pub is_admin: bool,
}

#[derive(Insertable)]
#[table_name="user"]
pub struct NewUser<'a> {
    pub name: &'a str,
    pub hashed_password: &'a str,
    pub is_admin: bool,
}
