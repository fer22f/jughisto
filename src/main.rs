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
                        name: (&user.name).into(),
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
            flash_message: flash_message.map(|f| f.msg().into()),
        },
    )
}

#[derive(FromForm)]
struct SubmissionForm {
    language: String,
    source_text: String,
}

use language::LanguageParams;
use queue::Submission;
use rocket::State;
use std::collections::HashMap;
use crossbeam_channel::Sender;

struct SubmissionState {
    channel: Sender<Submission>,
    languages: HashMap<String, LanguageParams>,
}

#[post("/submissions", data = "<form>")]
fn create_submission(
    _user: LoggedUser,
    form: Form<SubmissionForm>,
    submission_state: State<SubmissionState>,
    connection: DbConnection,
) -> Flash<Redirect> {
    match submission_state.languages.get(&form.language) {
        Some(_) => {
            let uuid = Uuid::new_v4();
            if let Err(_) = submission::insert_submission(
                &connection,
                submission::NewSubmission {
                    uuid: uuid.to_string(),
                    source_text: (&form.source_text).into(),
                    language: (&form.language).into(),
                    submission_instant: Local::now().naive_local(),
                },
            ) {
                return Flash::error(Redirect::to("/"), "Falha ao submeter");
            }
            if let Err(_) = submission_state.channel.send(
                Submission {
                    uuid,
                    language: (&form.language).into(),
                    source_text: (&form.source_text).into(),
                },
            ) {
                return Flash::error(Redirect::to("/"), "Falha ao submeter");
            }
            Flash::success(
                Redirect::to("/"),
                format!("Submetido {} com sucesso!", uuid),
            )
        }
        None => Flash::error(Redirect::to("/"), "Linguagem inexistente"),
    }
}

use chrono::prelude::*;
use models::submission;

#[get("/submissions")]
fn get_submissions(_user: LoggedUser, connection: DbConnection) -> Template {
    #[derive(Serialize)]
    struct SubmissionResult {
        uuid: String,
        verdict: String,
        problem: String,
        formatted_date_time: String,
        compilation_stderr: Option<String>,
    }

    #[derive(Serialize)]
    struct SubmissionsContext {
        submissions: Vec<SubmissionResult>,
    }

    let submissions = submission::get_submissions(&connection).unwrap();

    Template::render(
        "submissions",
        SubmissionsContext {
            submissions: submissions
                .iter()
                .map(|submission| SubmissionResult {
                    uuid: (&submission.uuid).into(),
                    verdict: submission
                        .verdict
                        .as_ref()
                        .map(|s| String::from(s))
                        .unwrap_or("WJ".into())
                        .to_string(),
                    problem: "A".into(),
                    formatted_date_time: submission
                        .submission_instant
                        .format("%d/%m/%Y %H:%M:%S")
                        .to_string(),
                    compilation_stderr: submission.compilation_stderr.as_ref().map(|s| s.into()),
                })
                .collect(),
        },
    )
}

use rocket_contrib::templates::handlebars::{
    Context, Handlebars, Helper, HelperResult, Output, RenderContext, RenderError,
};
use uuid::Uuid;

fn main() {
    setup::setup_dotenv();
    {
        let connection = setup::establish_connection();
        setup::setup_admin(&connection);
    }

    let isolate_executable_path = setup::get_isolate_executable_path();
    let languages = language::get_supported_languages();
    let channel = queue::setup_workers(isolate_executable_path, languages);
    let languages = language::get_supported_languages();

    rocket::ignite()
        .mount(
            "/",
            routes![
                index,
                post_login,
                get_login,
                create_submission,
                get_submissions
            ],
        )
        .mount("/static", StaticFiles::from("./static"))
        .attach(DbConnection::fairing())
        .attach(Template::custom(|engines| {
            // TODO: When Rocket updates finally, this will be removed
            // This is needed because rocket_contrib depends on Handlebars 1.0
            // which doesn't have eq implemented for strings
            engines.handlebars.register_helper(
                "eq",
                Box::new(
                    |h: &Helper,
                     _: &Handlebars,
                     _: &Context,
                     _: &mut RenderContext,
                     out: &mut dyn Output|
                     -> HelperResult {
                        let f = h
                            .param(0)
                            .ok_or(RenderError::new("first param not found"))?;
                        let s = h
                            .param(1)
                            .ok_or(RenderError::new("second param not found"))?;

                        if f.value() == s.value() {
                            out.write("ok")?;
                        }
                        Ok(())
                    },
                ),
            );
        }))
        .register(catchers![unauthorized])
        .manage(SubmissionState { channel, languages })
        .launch();
}
