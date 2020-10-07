use diesel::sqlite::SqliteConnection;
use diesel::Connection;
use dotenv::dotenv;
use std::env;
use which::which;

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
    use user::PasswordMatched;

    let admin_user_name = "admin";
    let admin_user_password = "admin";

    match user::check_matching_password(connection, admin_user_name, admin_user_password)
        .expect("Couldn't check admin user")
    {
        PasswordMatched::UserDoesntExist => {
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
        PasswordMatched::PasswordMatches => {
            println!("Admin already created and is using default password.",)
        }
        PasswordMatched::PasswordDoesntMatch => {
            println!("Admin already created and is not using the default password.",)
        }
    }
}

use std::path::PathBuf;

pub fn get_isolate_executable_path() -> PathBuf {
    which("isolate").expect("isolate binary not installed")
}
