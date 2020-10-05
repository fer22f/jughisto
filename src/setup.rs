use diesel::sqlite::SqliteConnection;
use diesel::Connection;
use dotenv::dotenv;
use std::env;

use crate::models::user;
use crate::models::user::NewUser;

pub fn setup_dotenv() {
    dotenv().expect(".env should work");
}

pub fn establish_connection() -> SqliteConnection {
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    SqliteConnection::establish(&database_url)
        .expect(&format!("Error connecting to {}", database_url))
}

pub fn setup_admin(connection: &SqliteConnection) {
    let admin_user_name = "admin";
    let admin_user_password = "admin";

    match user::get_user_by_name(connection, admin_user_name) {
        Ok(_) => {
            println!(
                "Admin already created. Is using default password? {}",
                user::check_matching_password(connection, admin_user_name, admin_user_password)
                    .expect("Couldn't check match password")
            );
        }
        Err(_) => {
            println!("Inserting admin...");
            user::insert_new_user(
                connection,
                NewUser {
                    name: admin_user_name,
                    password: admin_user_password,
                    is_admin: true,
                },
            )
            .expect("Error saving new user");
        }
    }
}
