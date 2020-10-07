#![feature(proc_macro_hygiene, decl_macro)]

// TODO: Remove this in the next release of diesel
#[macro_use]
extern crate diesel;

use serde::{Deserialize, Serialize};
use serde_json::json;

use actix_files::Files;
use actix_identity::Identity;
use actix_web::middleware::errhandlers::{ErrorHandlerResponse, ErrorHandlers};
use actix_web::{dev, get, http, middleware, post, web, App, HttpServer};
use diesel::SqliteConnection;
use std::env;
use std::io;
use uuid::Uuid;

use actix_identity::{CookieIdentityPolicy, IdentityService};
use actix_web::HttpResponse;
use chrono::prelude::*;
use diesel::r2d2::ConnectionManager;
use handlebars::Handlebars;
use models::submission;
use models::user;

mod import_contest;
mod isolate;
mod language;
mod models;
mod queue;
mod schema;
mod setup;

use std::thread;
type DbPool = r2d2::Pool<ConnectionManager<SqliteConnection>>;

#[actix_web::main]
async fn main() -> io::Result<()> {
    setup::setup_dotenv();

    std::env::set_var("RUST_LOG", "actix_web=info");
    env_logger::init();

    let private_key = env::var("IDENTITY_SECRET_KEY").unwrap();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL");
    let manager = ConnectionManager::<SqliteConnection>::new(database_url);
    let pool = r2d2::Pool::builder()
        .max_size(1)
        .build(manager)
        .expect("Failed to create pool.");

    setup::setup_admin(&pool.get().expect("Coudln't get connection from the pool"));

    let mut handlebars = Handlebars::new();
    handlebars
        .register_templates_directory(".html.hbs", "./templates")
        .unwrap();
    let handlebars_ref = web::Data::new(handlebars);

    let isolate_executable_path = setup::get_isolate_executable_path();
    let languages = language::get_supported_languages();
    let (channel, submission_completion_channel) =
        queue::setup_workers(isolate_executable_path, languages);

    let submission_pool = pool.clone();
    thread::spawn(move || loop {
        let submission_completion = submission_completion_channel
            .recv()
            .expect("Failed to recv from submission completion channel");
        let connection = submission_pool
            .get()
            .expect("Couldn't get connection from the pool");
        submission::complete_submission(&connection, submission_completion)
            .expect("Couldn't complete submission");
    });

    HttpServer::new(move || {
        let languages = language::get_supported_languages();
        App::new()
            .data(pool.clone())
            .data(SubmissionState {
                channel: channel.clone(),
                languages,
            })
            .wrap(ErrorHandlers::new().handler(http::StatusCode::UNAUTHORIZED, render_401))
            .wrap(actix_flash::Flash::default())
            .wrap(IdentityService::new(
                CookieIdentityPolicy::new(&private_key.as_bytes())
                    .name("user")
                    .secure(false),
            ))
            .wrap(middleware::Logger::default())
            .app_data(handlebars_ref.clone())
            .service(get_login)
            .service(post_login)
            .service(index)
            .service(get_submissions)
            .service(create_submission)
            .service(Files::new("/static/", "./static/"))
    })
    .bind("localhost:8000")?
    .run()
    .await
}

#[get("/login")]
async fn get_login(
    flash: Option<actix_flash::Message<String>>,
    hb: web::Data<Handlebars<'_>>,
) -> HttpResponse {
    HttpResponse::Ok().body(
        hb.render(
            "login",
            &json!({
                "flash_message": flash.map_or("".into(), |f| f.into_inner())
            }),
        )
        .unwrap(),
    )
}

use actix_web::Responder;
use futures::FutureExt;

fn render_401(
    res: dev::ServiceResponse<dev::Body>,
) -> actix_web::Result<ErrorHandlerResponse<dev::Body>> {
    Ok(ErrorHandlerResponse::Future(
        async move {
            let response = actix_flash::Response::with_redirect(
                String::from("Você precisa estar logado para acessar esta página"),
                "/login",
            )
            .respond_to(res.request())
            .await?;
            Ok(res.into_response(response))
        }
        .boxed_local(),
    ))
}

#[derive(Serialize, Deserialize)]
struct LoginForm {
    name: String,
    password: String,
}

#[get("/")]
async fn index(id: Identity, hb: web::Data<Handlebars<'_>>) -> HttpResponse {
    if let None = id.identity() {
        return HttpResponse::Unauthorized().finish();
    }
    let languages = language::get_supported_languages();
    let mut languages = languages.keys().cloned().collect::<Vec<String>>();
    languages.sort();

    #[derive(Serialize)]
    struct IndexContext {
        languages: Vec<String>,
    };

    HttpResponse::Ok().body(hb.render("index", &IndexContext { languages }).unwrap())
}

#[derive(Serialize, Deserialize)]
struct LoggedUser {
    name: String,
    is_admin: bool,
}

#[post("/login")]
async fn post_login(
    id: Identity,
    pool: web::Data<DbPool>,
    form: web::Form<LoginForm>,
) -> actix_flash::Response<HttpResponse, String> {
    let connection = pool.get().expect("couldn't get db connection from pool");

    use user::PasswordMatched;
    match web::block(move || user::check_matching_password(&connection, &form.name, &form.password))
        .await
    {
        Ok(PasswordMatched::UserDoesntExist) => {
            actix_flash::Response::with_redirect("Usuário não existe".into(), "/login")
        }
        Ok(PasswordMatched::PasswordDoesntMatch) => {
            actix_flash::Response::with_redirect("Senha incorreta".into(), "/login")
        }
        Ok(PasswordMatched::PasswordMatches(user)) => {
            if let Ok(user) = user {
                id.remember(
                    serde_json::to_string(&LoggedUser {
                        name: (&user.name).into(),
                        is_admin: user.is_admin,
                    })
                    .expect("Couldn't convert user to JSON"),
                );
                actix_flash::Response::with_redirect("".into(), "/")
            } else {
                actix_flash::Response::with_redirect("Erro interno do servidor".into(), "/login")
            }
        }
        Err(_) => actix_flash::Response::with_redirect("Erro interno do servidor".into(), "/login"),
    }
}

#[get("/submissions")]
async fn get_submissions(
    id: Identity,
    pool: web::Data<DbPool>,
    hb: web::Data<Handlebars<'_>>,
) -> HttpResponse {
    if let None = id.identity() {
        return HttpResponse::Unauthorized().finish();
    }

    let connection = pool.get().expect("couldn't get db connection from pool");

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

    HttpResponse::Ok().body(
        hb.render(
            "submissions",
            &SubmissionsContext {
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
                        compilation_stderr: submission
                            .compilation_stderr
                            .as_ref()
                            .map(|s| s.into()),
                    })
                    .collect(),
            },
        )
        .unwrap(),
    )
}

#[derive(Serialize, Deserialize)]
struct SubmissionForm {
    language: String,
    source_text: String,
}

use crossbeam::channel::Sender;
use language::LanguageParams;
use queue::Submission;
use std::collections::HashMap;

struct SubmissionState {
    channel: Sender<Submission>,
    languages: HashMap<String, LanguageParams>,
}

use actix_web::Either;

#[post("/submissions")]
async fn create_submission(
    id: Identity,
    form: web::Form<SubmissionForm>,
    submission_state: web::Data<SubmissionState>,
    pool: web::Data<DbPool>,
) -> Either<actix_flash::Response<HttpResponse, String>, HttpResponse> {
    if let None = id.identity() {
        return Either::B(HttpResponse::Unauthorized().finish());
    }

    let connection = pool.get().expect("couldn't get db connection from pool");

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
                return Either::A(actix_flash::Response::with_redirect(
                    "Falha ao submeter".into(),
                    "/",
                ));
            }

            if let Err(_) = submission_state.channel.send(Submission {
                uuid,
                language: (&form.language).into(),
                source_text: (&form.source_text).into(),
            }) {
                return Either::A(actix_flash::Response::with_redirect(
                    "Falha ao submeter".into(),
                    "/",
                ));
            }
            Either::A(actix_flash::Response::with_redirect(
                format!("Submetido {} com sucesso!", uuid),
                "/",
            ))
        }
        None => Either::A(actix_flash::Response::with_redirect(
            "Linguagem inexistente".into(),
            "/",
        )),
    }
}
