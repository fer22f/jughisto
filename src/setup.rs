use chrono::prelude::*;
use log::info;

use crate::models::user;
use crate::models::user::NewUser;
use diesel::PgConnection;

pub fn setup_admin(connection: &PgConnection) {
    use user::PasswordMatched;

    let admin_user_name = "admin";
    let admin_user_password = "admin";

    match user::check_matching_password(connection, admin_user_name, admin_user_password)
        .expect("Couldn't check admin user")
    {
        PasswordMatched::UserDoesntExist => {
            info!("Inserting admin...");
            user::insert_new_user(
                connection,
                NewUser {
                    name: admin_user_name,
                    password: admin_user_password,
                    is_admin: true,
                    creation_instant: Local::now().naive_local(),
                    creation_user_id: None,
                },
            )
            .expect("Error saving new user");
        }
        PasswordMatched::PasswordMatches(_) => {
            info!("Admin already created and is using default password.",)
        }
        PasswordMatched::PasswordDoesntMatch => {
            info!("Admin already created and is not using the default password.",)
        }
    }
}
