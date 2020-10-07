#![feature(proc_macro_hygiene, decl_macro)]

// TODO: Remove this in the next release of diesel
#[macro_use]
extern crate diesel;

use rocket::request::{Form, FromForm};
use rocket::response::status::NotFound;
use rocket::{catch, catchers, get, post, routes};
use rocket_contrib::json::Json;
use rocket_contrib::serve::StaticFiles;
use rocket_contrib::templates::Template;
use rocket_contrib::{database, databases};
use serde::{Deserialize, Serialize};

use models::user;
use models::user::User;

mod import_contest;
mod isolate;
mod language;
mod models;
mod queue;
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

#[derive(FromForm)]
struct LoginForm {
    name: String,
    password: String,
}

use rocket::http::{Cookie, Cookies};
use rocket::request::FlashMessage;
use rocket::request::FromRequest;
use rocket::response::{Flash, Redirect};

#[derive(Serialize, Deserialize)]
struct LoggedUser {
    name: String,
    is_admin: bool,
}

use rocket::outcome::IntoOutcome;
use rocket::request::Outcome;
use rocket::Request;
use serde_json;

impl<'a, 'r> FromRequest<'a, 'r> for LoggedUser {
    type Error = ();

    fn from_request(request: &'a Request<'r>) -> Outcome<LoggedUser, ()> {
        request
            .cookies()
            .get_private("user")
            .and_then(|cookie| serde_json::from_str(cookie.value()).ok())
            .into_outcome((rocket::http::Status::Unauthorized, ()))
    }
}

#[catch(401)]
fn unauthorized() -> Flash<Redirect> {
    Flash::error(
        Redirect::to("/login"),
        "Por favor entre para acessar esta página",
    )
}

#[post("/login", data = "<form>")]
fn post_login(
    connection: DbConnection,
    mut cookies: Cookies,
    form: Form<LoginForm>,
) -> Flash<Redirect> {
    use user::PasswordMatched;
    match user::check_matching_password(&connection, &form.name, &form.password) {
        Ok(PasswordMatched::UserDoesntExist) => {
            Flash::error(Redirect::to("/login"), "Usuário não existe")
        }
        Ok(PasswordMatched::PasswordDoesntMatch) => {
            Flash::error(Redirect::to("/login"), "Senha incorreta")
        }
        Ok(PasswordMatched::PasswordMatches) => {
            if let Ok(user) = user::get_user_by_name(&connection, &form.name) {
                cookies.add_private(Cookie::new(
                    "user",
                    serde_json::to_string(&LoggedUser {
                        name: String::from(&user.name),
                        is_admin: user.is_admin,
                    })
                    .unwrap(),
                ));
                Flash::success(Redirect::to("/"), "")
            } else {
                Flash::error(Redirect::to("/login"), "Erro interno do servidor")
            }
        }
        Err(_) => Flash::error(Redirect::to("/login"), "Erro interno do servidor"),
    }
}

#[get("/")]
fn index(_user: LoggedUser) -> Template {
    let languages = language::get_supported_languages();
    let mut languages = languages.keys().cloned().collect::<Vec<String>>();
    languages.sort();

    #[derive(Serialize)]
    struct IndexContext {
        languages: Vec<String>,
    }

    Template::render("index", IndexContext { languages })
}

#[get("/login")]
fn get_login(flash_message: Option<FlashMessage>) -> Template {
    #[derive(Serialize)]
    struct LoginContext {
        flash_message: Option<String>,
    }

    Template::render(
        "login",
        LoginContext {
            flash_message: flash_message.map(|f| String::from(f.msg())),
        },
    )
}

#[derive(FromForm)]
struct SubmissionForm {
    language: String,
    source_text: String,
}

use language::LanguageParams;
use queue::{enqueue_submission, Submission, SubmissionQueue};
use rocket::State;
use std::collections::HashMap;
use std::sync::Arc;

struct SubmissionState {
    queue: Arc<SubmissionQueue>,
    languages: HashMap<String, LanguageParams>,
}

#[post("/submissions", data = "<form>")]
fn create_submission(
    form: Form<SubmissionForm>,
    submission_state: State<SubmissionState>,
) -> Flash<Redirect> {
    match submission_state.languages.get(&form.language) {
        Some(_) => {
            enqueue_submission(
                &submission_state.queue,
                Submission {
                    language: String::from(&form.language),
                    source_text: String::from(&form.source_text),
                },
            );
            Flash::success(Redirect::to("/"), "Submetido com sucesso!")
        }
        None => Flash::error(Redirect::to("/"), "Linguagem inexistente"),
    }
}

fn main() {
    setup::setup_dotenv();
    let connection = setup::establish_connection();
    setup::setup_admin(&connection);

    let isolate_executable_path = setup::get_isolate_executable_path();
    let languages = language::get_supported_languages();
    let queue = queue::setup_workers(isolate_executable_path, languages);
    let languages = language::get_supported_languages();

    rocket::ignite()
        .mount(
            "/",
            routes![index, post_login, get_login, create_submission],
        )
        .mount("/static", StaticFiles::from("./static"))
        .attach(DbConnection::fairing())
        .attach(Template::fairing())
        .register(catchers![unauthorized])
        .manage(SubmissionState { queue, languages })
        .launch();
}
