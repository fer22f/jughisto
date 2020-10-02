#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use] extern crate rocket;
#[macro_use] extern crate rocket_contrib;
#[macro_use] extern crate diesel;
extern crate dotenv;
extern crate argon2;

use rocket_contrib::serve::StaticFiles;
use rocket_contrib::databases;
use rocket_contrib::json::Json;
use rocket::response::status::NotFound;
use serde::Serialize;
use diesel::Connection;
use crate::diesel::query_dsl::filter_dsl::FilterDsl;
use crate::diesel::ExpressionMethods;

mod models;
mod schema;

use diesel::RunQueryDsl;
use models::User;
use schema::user::dsl;
use std::env;
use dotenv::dotenv;

#[database("default")]
struct DbConnection(databases::diesel::SqliteConnection);

#[derive(Serialize)]
struct UserResponse {
    users: Vec<User>
}

#[get("/users")]
fn get_users<'a>(connection: DbConnection) -> Result<Json<UserResponse>, NotFound<String>> {
    match dsl::user.load::<User>(&*connection) {
        Ok(users) => Ok(Json(UserResponse { users })),
        Err(e) => Err(NotFound(e.to_string()))
    }
}

fn establish_connection() -> diesel::sqlite::SqliteConnection {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    diesel::sqlite::SqliteConnection::establish(&database_url).expect(&format!("Error connecting to {}", database_url))
}

fn setup_admin() {
    let connection = establish_connection();
    if let Ok(a) = dsl::user.filter(dsl::name.eq("admin")).first::<User>(&connection) {
        println!("Admin already created. Is using default password? {}", argon2::verify_encoded(&a.hashed_password, "admin".as_bytes()).unwrap());
    } else {
        println!("Inserting admin...");
        let config = argon2::Config::default();
        let hashed_password = &argon2::hash_encoded(
            "admin".as_bytes(),
            env::var("SECRET_HASH_KEY").expect("SECRET_HASH_KEY must be set").as_bytes(),
            &config
        ).unwrap();
        diesel::insert_into(dsl::user).values(models::NewUser {
            name: "admin",
            hashed_password,
            is_admin: true,
        }).execute(&connection).expect("Error saving new post");
    }
}

fn main() {
    setup_admin();

    rocket::ignite()
        .mount("/api", routes![get_users])
        .mount("/", StaticFiles::from("./static"))
        .attach(DbConnection::fairing())
        .launch();
}
