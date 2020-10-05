#![feature(proc_macro_hygiene, decl_macro)]

// TODO: Remove this in the next release of diesel
#[macro_use]
extern crate diesel;

use rocket::response::status::NotFound;
use rocket::{get, routes};
use rocket_contrib::json::Json;
use rocket_contrib::serve::StaticFiles;
use rocket_contrib::{database, databases};
use serde::Serialize;

use models::user;
use models::user::User;

mod import_contest;
mod models;
mod schema;
mod setup;

#[database("default")]
struct DbConnection(databases::diesel::SqliteConnection);

#[derive(Serialize)]
struct UserResponse {
    users: Vec<User>,
}

#[get("/users")]
fn get_users(connection: DbConnection) -> Result<Json<UserResponse>, NotFound<String>> {
    user::get_users(&connection)
        .map(|users| Json(UserResponse { users }))
        .map_err(|e| NotFound(e.to_string()))
}

use std::fs::File;

#[get("/test")]
fn test() -> Result<String, import_contest::ImportContestError> {
    let file = File::open("./data/contest.zip").unwrap();
    import_contest::import_file(file)
}

fn main() {
    setup::setup_dotenv();
    let connection = setup::establish_connection();
    setup::setup_admin(&connection);

    rocket::ignite()
        .mount("/api", routes![get_users, test])
        .mount("/", StaticFiles::from("./static"))
        .attach(DbConnection::fairing())
        .launch();
}
