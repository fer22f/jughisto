// TODO: Remove this in the next release of diesel
#[macro_use]
extern crate diesel;

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};

use actix_files::Files;
use actix_identity::Identity;
use actix_web::middleware::{ErrorHandlerResponse, ErrorHandlers};
use actix_web::{dev, get, http, middleware, post, web, App, HttpServer};
use diesel::pg::PgConnection;
use std::env;
use std::fs::File;
use uuid::Uuid;
use actix_identity::{CookieIdentityPolicy, IdentityService};
use actix_session::{CookieSession, Session};
use actix_web::HttpResponse;
use chrono::prelude::*;
use diesel::r2d2::ConnectionManager;
use handlebars::Handlebars;
use log::{error, info};
use models::submission;
use models::user;

mod broadcaster;
mod import_contest;
mod models;
mod schema;
mod setup;
mod queue;
mod flash;
mod language;

use broadcaster::Broadcaster;
use listenfd::ListenFd;
use std::sync::Mutex;
type DbPool = r2d2::Pool<ConnectionManager<PgConnection>>;
use chrono_tz::Tz;
use std::time::Duration;
use queue::{JobQueuer};
use queue::job_protocol::job_queue_server::JobQueueServer;
use queue::job_protocol::{Language, job, Job, JobResult, job_result};
use tonic::transport::Server;
use std::error::Error;
use async_channel::Sender;
use futures::TryFutureExt;
use actix_web::web::Data;
use tokio::sync::broadcast;
use std::fs;
use submission::SubmissionCompletion;

async fn update_database(mut job_result_receiver: broadcast::Receiver<JobResult>, pool: DbPool) -> Result<(), PostError> {
    loop {
        let job_result = job_result_receiver.recv().await.unwrap();
        if let JobResult {
            which: Some(job_result::Which::Judgement(judgement)),
            ..
        } = job_result {
            let connection = pool.get().unwrap();
            submission::complete_submission(&connection, SubmissionCompletion {
                uuid: job_result.uuid,
                verdict: match job_result::judgement::Verdict::from_i32(judgement.verdict) {
                    Some(job_result::judgement::Verdict::Accepted) => "AC".into(),
                    Some(job_result::judgement::Verdict::WrongAnswer) => "WA".into(),
                    Some(job_result::judgement::Verdict::CompilationError) => "CE".into(),
                    Some(job_result::judgement::Verdict::TimeLimitExceeded) => "TL".into(),
                    Some(job_result::judgement::Verdict::MemoryLimitExceeded) => "ML".into(),
                    Some(job_result::judgement::Verdict::RuntimeError) => "RE".into(),
                    None => "XX".into(),
                },
                judge_start_instant: chrono::NaiveDateTime::parse_from_str(&judgement.judge_start_instant, "%Y-%m-%dT%H:%M:%S%.f").unwrap(),
                judge_end_instant: chrono::NaiveDateTime::parse_from_str(&judgement.judge_end_instant, "%Y-%m-%dT%H:%M:%S%.f").unwrap(),
                memory_kib: Some(judgement.memory_kib),
                time_ms: Some(judgement.time_ms),
                time_wall_ms: Some(judgement.time_wall_ms),
                error_output: Some(judgement.error_output)
            }).unwrap();
        }
    }
}

#[actix_web::main]
async fn main() -> Result<(), Box<dyn Error>> {
    std::env::set_var("RUST_LOG", "info");
    env_logger::init();

    let private_key = env::var("IDENTITY_SECRET_KEY")
        .expect("IDENTITY_SECRET_KEY environment variable is not set");

    let database_url =
        env::var("DATABASE_URL").expect("DATABASE_URL environment variable is not set");
    let manager = ConnectionManager::<PgConnection>::new(database_url);
    let pool = r2d2::Pool::builder()
        .max_size(50)
        .connection_timeout(Duration::from_secs(1))
        .build(manager)
        .expect("Failed to create pool.");

    setup::setup_admin(&pool.get().expect("Couldn't get connection from the pool"));

    let mut handlebars = Handlebars::new();
    handlebars
        .register_templates_directory(".html.hbs", "./templates")
        .expect("Couldn't find templates directory");
    let handlebars_ref = web::Data::new(handlebars);

    let languages = Arc::new(DashMap::<String, Language>::new());

    let tz: Tz = env::var("TZ")
        .expect("TZ environment variable is not set")
        .parse()
        .expect("Invalid timezone in environment variable TZ");

    let (job_sender, job_receiver) = async_channel::unbounded();
    let (job_result_sender, job_result_receiver) = broadcast::channel(40);

    let broadcaster = Broadcaster::create(job_result_receiver);

    let mut listenfd = ListenFd::from_env();
    let job_sender_data = job_sender.clone();
    let job_result_sender_data = job_result_sender.clone();
    let languages_data = languages.clone();
    let pool_data = pool.clone();
    let mut server = HttpServer::new(move || {
        App::new()
            .app_data(Data::new(pool_data.clone()))
            .app_data(Data::new(job_sender_data.clone()))
            .app_data(Data::new(job_result_sender_data.clone()))
            .app_data(Data::new(languages_data.clone()))
            .app_data(Data::new(tz.clone()))
            .wrap(ErrorHandlers::new().handler(http::StatusCode::UNAUTHORIZED, render_401))
            .wrap(ErrorHandlers::new().handler(http::StatusCode::BAD_REQUEST, render_400))
            .wrap(flash::Flash::default())
            .wrap(IdentityService::new(
                CookieIdentityPolicy::new(&private_key.as_bytes())
                    .name("user")
                    .secure(false),
            ))
            .wrap(CookieSession::signed(&private_key.as_bytes()).secure(false))
            .wrap(middleware::Logger::default())
            .app_data(broadcaster.clone())
            .app_data(handlebars_ref.clone())
            .service(
            web::scope("/jughisto")
            .service(get_login)
            .service(get_me)
            .service(change_password)
            .service(post_login)
            .service(post_logout)
            .service(get_main)
            .service(get_contests)
            .service(get_contest_by_id)
            .service(get_contest_problem_by_id_label)
            .service(get_submissions)
            .service(get_submissions_by_contest_id)
            .service(get_submissions_by_contest_id_problem_label)
            .service(create_submission)
            .service(create_contest)
            .service(create_user)
            .service(submission_updates)
            .service(Files::new("/static/", "./static/"))
            .service(get_problem_by_id_assets))
    });

    server = if let Some(l) = listenfd
        .take_tcp_listener(0)
        .expect("Can't take TCP listener from listenfd")
    {
        server.listen(l)?
    } else {
        server.bind("0.0.0.0:8000")?
    };

    let addr = "0.0.0.0:50051".parse().unwrap();

    let update_dabase_sender = job_result_sender.clone();
    log::info!("Starting at {}", addr);
    tokio::try_join!(
        server.run().map_err(|e| Into::<Box<dyn Error>>::into(e)),
        Server::builder()
            .add_service(JobQueueServer::new(JobQueuer {
                job_receiver,
                job_result_sender,
                languages,
            }))
            .serve(addr)
            .map_err(|e| Into::<Box<dyn Error>>::into(e)),
        update_database(update_dabase_sender.subscribe(), pool.clone())
            .map_err(|e| Into::<Box<dyn Error>>::into(e))
    )?;

    Ok(())
}

use actix_web::http::StatusCode;
use thiserror::Error;

#[derive(Error, Debug)]
#[error("unauthorized")]
struct UnauthorizedError {}

#[derive(Error, Debug)]
enum PostError {
    #[error(transparent)]
    Unauthorized(#[from] UnauthorizedError),
    #[error("{0}")]
    Custom(String),
    #[error("{0}")]
    Validation(String),
    #[error("couldn't get connection from pool")]
    ConnectionPool(#[from] r2d2::Error),
    #[error("couldn't hash")]
    UserHashing(#[from] user::UserHashingError),
    #[error(transparent)]
    Web(#[from] actix_web::Error),
    #[error(transparent)]
    Queue(#[from] async_channel::SendError<Job>),
    #[error("couldn't fetch result from database")]
    Database(#[from] diesel::result::Error),
    #[error("couldn't work with the filesystem")]
    Io(#[from] std::io::Error),
    #[error("couldn't work with the zip")]
    Zip(#[from] zip::result::ZipError),
}

fn error_response_and_log(me: &impl actix_web::error::ResponseError) -> HttpResponse {
    use std::fmt::Write;
    error!("{}", me);
    let mut resp = HttpResponse::new(me.status_code());
    let mut buf = actix_web::web::BytesMut::new();
    let _ = write!(&mut buf, "{}", me);
    resp.headers_mut().insert(
        actix_web::http::header::CONTENT_TYPE,
        actix_web::http::HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    resp.set_body(actix_web::body::Body::from(buf))
}

impl actix_web::error::ResponseError for PostError {
    fn error_response(&self) -> HttpResponse {
        error_response_and_log(self)
    }

    fn status_code(&self) -> StatusCode {
        match *self {
            PostError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            PostError::Validation(_) => StatusCode::BAD_REQUEST,
            PostError::Custom(_)
            | PostError::ConnectionPool(_)
            | PostError::Web(_)
            | PostError::Queue(_)
            | PostError::Database(_)
            | PostError::Io(_)
            | PostError::UserHashing(_)
            | PostError::Zip(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

type PostResult = Result<flash::Response<HttpResponse, String>, PostError>;

#[derive(Error, Debug)]
enum GetError {
    #[error("unauthorized")]
    Unauthorized(#[from] UnauthorizedError),
    #[error("couldn't render")]
    Render(#[from] handlebars::RenderError),
    #[error(transparent)]
    Actix(#[from] actix_web::Error),
    #[error("couldn't fetch result from database")]
    Diesel(#[from] diesel::result::Error),
    #[error("couldn't get connection from pool")]
    R2d2Pool(#[from] r2d2::Error),
    #[error("couldn't find file")]
    Io(#[from] std::io::Error),
}

impl actix_web::error::ResponseError for GetError {
    fn error_response(&self) -> HttpResponse {
        error_response_and_log(self)
    }

    fn status_code(&self) -> StatusCode {
        match *self {
            GetError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            GetError::Io(_) => StatusCode::NOT_FOUND,
            GetError::Render(_)
            | GetError::Actix(_)
            | GetError::Diesel(_)
            | GetError::R2d2Pool(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

type GetResult = Result<flash::Response<HttpResponse, String>, GetError>;

#[get("/login")]
async fn get_login(
    flash: Option<flash::Message<String>>,
    identity: Identity,
    hb: web::Data<Handlebars<'_>>,
) -> GetResult {
    let logged_user = get_identity(identity);
    #[derive(Serialize)]
    struct LoginContext {
        logged_user: Option<LoggedUser>,
        flash_message: String,
        base_url: String,
    }
    Ok(flash::Response::new(
        None,
        HttpResponse::Ok().body(hb.render(
        "login",
        &LoginContext {
            logged_user,
            flash_message: flash.map_or("".into(), |f| f.into_inner()),
            base_url: env::var("BASE_URL")
                .expect("BASE_URL environment variable is not set")
        },
    )?)))
}

#[get("/me")]
async fn get_me(
    flash: Option<flash::Message<String>>,
    identity: Identity,
    hb: web::Data<Handlebars<'_>>,
) -> GetResult {
    let logged_user = require_identity(identity)?;
    #[derive(Serialize)]
    struct MeContext {
        logged_user: LoggedUser,
        flash_message: String,
        base_url: String,
    }
    Ok(flash::Response::new(
        None,
        HttpResponse::Ok().body(hb.render(
        "me",
        &MeContext {
            logged_user,
            flash_message: flash.map_or("".into(), |f| f.into_inner()),
            base_url: env::var("BASE_URL")
                .expect("BASE_URL environment variable is not set")
        },
    )?)))
}

use actix_files::NamedFile;

#[get("/problems/{id}/assets/{filename}")]
async fn get_problem_by_id_assets(
    identity: Identity,
    pool: web::Data<DbPool>,
    path: web::Path<(i32, String,)>,
) -> Result<NamedFile, GetError> {
    require_identity(identity)?;
    let path = path.into_inner();
    let connection = pool.get()?;
    let problem = problem::get_problem_by_contest_id_metadata(&connection, path.0)?;
    let file_path = PathBuf::from("/data/").join(problem.id).join("statements/.html/portuguese/").join(path.1);
    Ok(NamedFile::open(file_path)?)
}

use actix_web::Responder;
use futures::FutureExt;

fn render_401(
    res: dev::ServiceResponse<dev::Body>,
) -> actix_web::Result<ErrorHandlerResponse<dev::Body>> {
    Ok(ErrorHandlerResponse::Future(
        async move {
            let response = flash::Response::with_redirect(
                String::from("Você precisa estar logado para acessar esta página"),
                &format!("{}login",
                    &env::var("BASE_URL")
                        .expect("BASE_URL environment variable is not set")),
            )
            .respond_to(res.request());
            Ok(res.into_response(response))
        }
        .boxed_local(),
    ))
}

fn render_400(
    res: dev::ServiceResponse<dev::Body>,
) -> actix_web::Result<ErrorHandlerResponse<dev::Body>> {
    Ok(ErrorHandlerResponse::Future(
        async move {
            let response = redirect_to_referer(
                match res.response().body() {
                    actix_web::body::AnyBody::Bytes(bytes) => {
                        String::from_utf8((&bytes).to_vec()).unwrap()
                    }
                    _ => "Entrada inválida".into(),
                },
                res.request(),
            )
            .respond_to(res.request());
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

use models::problem;
use models::problem::ProblemByContest;

fn get_identity(identity: Identity) -> Option<LoggedUser> {
    let identity = identity.identity();
    identity.and_then(|identity| serde_json::from_str(&identity).ok())
}

fn require_identity(identity: Identity) -> Result<LoggedUser, UnauthorizedError> {
    get_identity(identity).ok_or(UnauthorizedError {})
}

use contest::Contest;

#[get("/contests/{id}")]
async fn get_contest_by_id(
    flash: Option<flash::Message<String>>,
    identity: Identity,
    pool: web::Data<DbPool>,
    hb: web::Data<Handlebars<'_>>,
    path: web::Path<(i32,)>,
    tz: web::Data<Tz>,
    request: HttpRequest,
) -> GetResult {
    let logged_user = require_identity(identity)?;

    #[derive(Serialize)]
    struct FormattedProblemByContestWithScore {
        pub first_ac_submission_time: String,
        pub failed_submissions: i32,
        pub id: i32,
        pub name: String,
        pub label: String,
        pub memory_limit_mib: i32,
        pub time_limit: String,
    }

    #[derive(Serialize)]
    struct ContestContext {
        contest: FormattedContest,
        problems: Vec<FormattedProblemByContestWithScore>,
        submissions: Vec<FormattedSubmission>,
        logged_user: LoggedUser,
        flash_message: String,
        base_url: String,
    }

    let path = path.into_inner();

    let connection = pool.get()?;
    let contest = contest::get_contest_by_id(&connection, path.0)?;

    if contest.start_instant.map(|s| s > Local::now().naive_utc()).unwrap_or(false) && !logged_user.is_admin {
        return Ok(redirect_to_referer("Essa competição ainda não começou".into(), &request));
    }

    let problems = problem::get_problems_user_by_contest_id_with_score(&connection, logged_user.id, path.0)?;
    let submissions = submission::get_submissions_user_by_contest(&connection, logged_user.id, path.0)?;

    Ok(flash::Response::new(
        None,
        HttpResponse::Ok().body(
        hb.render(
            "contest",
            &ContestContext {
                contest: get_formatted_contest(&tz, &contest),
                problems: problems.iter().map(|p| FormattedProblemByContestWithScore {
                    first_ac_submission_time: p.first_ac_submission_instant.map(|t| format_utc_date_time(&tz, t)).unwrap_or("".into()),
                    failed_submissions: p.failed_submissions,
                    id: p.id,
                    name: p.name.clone(),
                    label: p.label.clone(),
                    memory_limit_mib: p.memory_limit_bytes / 1_024 / 1_024,
                    time_limit: format!("{}", f64::from(p.time_limit_ms) / 1000.0).replacen(".", ",", 1),
                }).collect(),
                logged_user,
                flash_message: flash.map_or("".into(), |f| f.into_inner()),
                base_url: env::var("BASE_URL")
                    .expect("BASE_URL environment variable is not set"),
                submissions: submissions
                    .iter()
                    .map(|(s, c)| format_submission(&tz, s, c))
                    .collect(),
            },
        )?,
    )))
}

#[get("/contests/{id}/{label}")]
async fn get_contest_problem_by_id_label(
    flash: Option<flash::Message<String>>,
    identity: Identity,
    pool: web::Data<DbPool>,
    hb: web::Data<Handlebars<'_>>,
    languages: web::Data<Arc<DashMap<String, Language>>>,
    session: Session,
    path: web::Path<(i32,String)>,
    tz: web::Data<Tz>,
) -> GetResult {
    let logged_user = require_identity(identity)?;

    #[derive(Serialize, Debug)]
    struct LanguageContext {
        order: i32,
        name: String,
        value: String,
    }

    #[derive(Serialize)]
    struct ContestProblemContext {
        languages: Vec<LanguageContext>,
        language: Option<String>,
        contest: FormattedContest,
        problems: Vec<ProblemByContest>,
        problem: ProblemByContest,
        submissions: Vec<FormattedSubmission>,
        flash_message: String,
        base_url: String,
        logged_user: LoggedUser,
    }

    let mut languages = languages
        .iter()
        .map(|kv| LanguageContext {
            order: kv.value().order,
            value: kv.key().into(),
            name: kv.value().name.clone(),
        })
        .collect::<Vec<_>>();
    languages.sort_by(|a, b| a.order.cmp(&b.order));

    let connection = pool.get()?;
    let path = path.into_inner();
    let contest = get_formatted_contest(&tz, &contest::get_contest_by_id(&connection, path.0)?);
    let problems = problem::get_problems_by_contest_id(&connection, path.0)?;
    let problem = problem::get_problem_by_contest_id_label(&connection, path.0, &path.1)?;
    let submissions = submission::get_submissions_user_by_contest_problem(&connection, logged_user.id, path.0, &path.1)?;

    Ok(flash::Response::new(
        None,
        HttpResponse::Ok().body(
        hb.render(
            "contest_problem",
            &ContestProblemContext {
                contest,
                languages,
                problems,
                problem,
                flash_message: flash.map_or("".into(), |f| f.into_inner()),
                base_url: env::var("BASE_URL")
                    .expect("BASE_URL environment variable is not set"),
                language: session.get("language")?,
                logged_user,
                submissions: submissions
                    .iter()
                    .map(|(s, c)| format_submission(&tz, s, c))
                    .collect(),
            },
        )?,
    )))
}

#[derive(Serialize, Deserialize)]
struct LoggedUser {
    id: i32,
    name: String,
    is_admin: bool,
}

#[post("/logout")]
async fn post_logout(
    identity: Identity,
) -> PostResult {
    identity.forget();
    Ok(flash::Response::with_redirect("".into(),
            &env::var("BASE_URL")
                .expect("BASE_URL environment variable is not set")))
}

#[post("/login")]
async fn post_login(
    identity: Identity,
    pool: web::Data<DbPool>,
    form: web::Form<LoginForm>,
) -> PostResult {
    let connection = pool.get()?;

    use user::PasswordMatched;
    use user::UserHashingError;
    match web::block(move || user::check_matching_password(&connection, &form.name, &form.password))
        .await
        .map_err(|e| PostError::Web(e.into()))?.map_err(|e| match e {
            UserHashingError::Database(e) => PostError::Database(e),
            UserHashingError::Hash(_) => {
                PostError::Validation("Senha inválida".into())
            },
        })? {
        PasswordMatched::UserDoesntExist => {
            Err(PostError::Validation("Usuário inexistente".into()))
        }
        PasswordMatched::PasswordDoesntMatch => {
            Err(PostError::Validation("Senha incorreta".into()))
        }
        PasswordMatched::PasswordMatches(logged_user) => {
            identity.remember(
                serde_json::to_string(&LoggedUser {
                    id: logged_user.id,
                    name: (&logged_user.name).into(),
                    is_admin: logged_user.is_admin,
                })
                .map_err(|_| PostError::Custom("Usuário no banco de dados inconsistente".into()))?,
            );
            Ok(flash::Response::with_redirect("".into(), &env::var("BASE_URL")
                .expect("BASE_URL environment variable is not set")))
        }
    }
}

#[get("/submission_updates/")]
async fn submission_updates(broadcaster: web::Data<Mutex<Broadcaster>>) -> HttpResponse {
    let rx = broadcaster
        .lock()
        .expect("Submission broadcaster is not active")
        .new_client();

    HttpResponse::Ok()
        .append_header(("content-type", "text/event-stream"))
        .streaming(rx)
}

#[derive(Serialize)]
struct FormattedSubmission {
    uuid: String,
    verdict: String,
    problem_label: String,
    submission_instant: String,
    error_output: Option<String>,
}

fn format_utc_date_time(tz: &Tz, input: NaiveDateTime) -> String {
    tz.from_utc_datetime(&input)
        .format("%d/%m/%Y %H:%M:%S")
        .to_string()
}

fn format_submission(
    tz: &Tz,
    submission: &models::submission::Submission,
    contest_problem: &models::submission::ContestProblem
) -> FormattedSubmission {
    FormattedSubmission {
        uuid: (&submission.uuid).into(),
        verdict: submission
            .verdict
            .as_ref()
            .map(|s| String::from(s))
            .unwrap_or("WJ".into())
            .into(),
        problem_label: contest_problem.label.clone(),
        submission_instant: format_utc_date_time(tz, submission.submission_instant),
        error_output: submission.error_output.as_ref().map(|s| s.into()),
    }
}

#[get("/submissions/me/")]
async fn get_submissions(
    identity: Identity,
    pool: web::Data<DbPool>,
    hb: web::Data<Handlebars<'_>>,
    tz: web::Data<Tz>,
) -> GetResult {
    let logged_user = require_identity(identity)?;
    let connection = pool.get()?;

    #[derive(Serialize)]
    struct SubmissionsContext {
        submissions: Vec<FormattedSubmission>,
    }

    let submissions = submission::get_submissions_user(&connection, logged_user.id)?;

    Ok(flash::Response::new(
        None,
        HttpResponse::Ok().body(
        hb.render(
            "submissions",
            &SubmissionsContext {
                submissions: submissions
                    .iter()
                    .map(|(s, c)| format_submission(&tz, s, c))
                    .collect(),
            },
        )?,
    )))
}

#[get("/submissions/me/contests/{id}")]
async fn get_submissions_by_contest_id(
    identity: Identity,
    pool: web::Data<DbPool>,
    hb: web::Data<Handlebars<'_>>,
    path: web::Path<(i32,)>,
    tz: web::Data<Tz>,
) -> GetResult {
    let logged_user = require_identity(identity)?;
    let connection = pool.get()?;

    #[derive(Serialize)]
    struct SubmissionsContext {
        submissions: Vec<FormattedSubmission>,
    }

    let path = path.into_inner();
    let submissions = submission::get_submissions_user_by_contest(&connection, logged_user.id, path.0)?;

    Ok(flash::Response::new(
        None,
        HttpResponse::Ok().body(
        hb.render(
            "submissions",
            &SubmissionsContext {
                submissions: submissions
                    .iter()
                    .map(|(s, c)| format_submission(&tz, s, c))
                    .collect(),
            },
        )?,
    )))
}

#[get("/submissions/me/contests/{id}/{label}")]
async fn get_submissions_by_contest_id_problem_label(
    identity: Identity,
    pool: web::Data<DbPool>,
    hb: web::Data<Handlebars<'_>>,
    path: web::Path<(i32, String)>,
    tz: web::Data<Tz>,
) -> GetResult {
    let logged_user = require_identity(identity)?;
    let connection = pool.get()?;

    #[derive(Serialize)]
    struct SubmissionsContext {
        submissions: Vec<FormattedSubmission>,
    }

    let path = path.into_inner();
    let submissions = submission::get_submissions_user_by_contest_problem(&connection, logged_user.id, path.0, &path.1)?;

    Ok(flash::Response::new(
        None,
        HttpResponse::Ok().body(
        hb.render(
            "submissions",
            &SubmissionsContext {
                submissions: submissions
                    .iter()
                    .map(|(s, c)| format_submission(&tz, s, c))
                    .collect(),
            },
        )?,
    )))
}

#[derive(Serialize, Deserialize)]
struct SubmissionForm {
    contest_problem_id: i32,
    language: String,
    source_text: String,
}

use std::sync::Arc;
use dashmap::DashMap;
use std::collections::HashMap;

use actix_web::HttpRequest;

fn redirect_to_referer(message: String, request: &HttpRequest) -> flash::Response<HttpResponse, String> {
    let referer = request
        .headers()
        .get("Referer")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.into())
        .unwrap_or(env::var("BASE_URL").expect("BASE_URL environment variable is not set"));
    flash::Response::with_redirect(message, &referer)
}

#[derive(Serialize, Deserialize)]
struct ChangePasswordForm {
    old_password: String,
    new_password: String,
    new_password_repeat: String,
}

#[post("/me/password")]
async fn change_password(
    identity: Identity,
    form: web::Form<ChangePasswordForm>,
    pool: web::Data<DbPool>,
    request: HttpRequest,
) -> PostResult {
    let identity = require_identity(identity)?;
    if form.new_password != form.new_password_repeat {
      return Err(PostError::Validation("Senhas são diferentes".into()));
    }

    let connection = pool.get()?;

    use user::PasswordMatched;
    match user::change_password(&connection, identity.id, &form.old_password, &form.new_password)? {
        PasswordMatched::PasswordMatches(_) => Ok(redirect_to_referer("Senha alterada com sucesso".into(), &request)),
        _ => Ok(redirect_to_referer("Senha antiga incorreta".into(), &request))
    }
}

#[derive(Serialize, Deserialize)]
struct CreateUserForm {
    name: String,
    password: String,
    is_admin: bool,
}

#[post("/users/")]
async fn create_user(
    identity: Identity,
    pool: web::Data<DbPool>,
    form: web::Form<CreateUserForm>,
    request: HttpRequest,
) -> PostResult {
    let identity = require_identity(identity)?;
    if !identity.is_admin {
        return Err(PostError::Unauthorized(UnauthorizedError {}));
    }

    let connection = pool.get()?;

    user::insert_new_user(&connection, user::NewUser {
        name: &form.name,
        password: &form.password,
        is_admin: form.is_admin,
        creation_instant: Local::now().naive_utc(),
        creation_user_id: Some(identity.id)
    })?;

    Ok(redirect_to_referer("Usuário criado com sucesso".into(), &request))
}

#[post("/submissions/")]
async fn create_submission(
    identity: Identity,
    form: web::Form<SubmissionForm>,
    pool: web::Data<DbPool>,
    job_sender: web::Data<Sender<Job>>,
    languages: web::Data<Arc<DashMap<String, Language>>>,
    session: Session,
    request: HttpRequest,
) -> PostResult {
    let identity = require_identity(identity)?;
    let connection = pool.get()?;

    languages
        .get(&form.language)
        .ok_or(PostError::Validation("Linguagem inexistente".into()))?;

    let uuid = Uuid::new_v4();
    submission::insert_submission(
        &connection,
        submission::NewSubmission {
            uuid: uuid.to_string(),
            source_text: (&form.source_text).into(),
            language: (&form.language).into(),
            submission_instant: Local::now().naive_utc(),
            contest_problem_id: form.contest_problem_id,
            user_id: identity.id,
        },
    )?;

    let metadata =
        problem::get_problem_by_contest_id_metadata(&connection, form.contest_problem_id)?;

    job_sender.send(Job {
        uuid: uuid.to_string(),
        language: (&form.language).into(),
        time_limit_ms: metadata.time_limit_ms,
        memory_limit_kib: metadata.memory_limit_bytes / 1_024,

        which: Some(job::Which::Judgement(job::Judgement {
            source_text: (&form.source_text).into(),
            test_count: metadata.test_count,
            test_pattern: format!("./{}/{}", metadata.id, metadata.test_pattern).into(),
            checker_language: metadata.checker_language,
            checker_source_path: format!("./{}/{}", metadata.id, metadata.checker_path).into(),
        }))
    }).await?;

    session.insert("language", &form.language)?;

    Ok(redirect_to_referer(format!("Submetido {} com sucesso!", uuid), &request))
}

#[derive(Serialize)]
struct FormattedContest {
    pub id: i32,
    pub name: String,
    pub start_instant: Option<String>,
    pub end_instant: Option<String>,
    pub creation_instant: String,
}

fn get_formatted_contest(tz: &Tz, contest: &Contest) -> FormattedContest {
    FormattedContest {
        id: contest.id,
        name: contest.name.clone(),
        start_instant: contest.start_instant.map(|i| format_utc_date_time(&tz, i)),
        end_instant: contest.end_instant.map(|i| format_utc_date_time(&tz, i)),
        creation_instant: format_utc_date_time(&tz, contest.creation_instant),
    }
}

fn get_formatted_contests(pool: &DbPool, tz: &Tz) -> Result<Vec<FormattedContest>, GetError> {
    let connection = pool.get()?;
    let contests = contest::get_contests(&connection)?;

    Ok(contests
        .iter()
        .map(|c| get_formatted_contest(tz, c))
    .collect())
}

#[get("/")]
async fn get_main(
    flash: Option<flash::Message<String>>,
    identity: Identity,
    pool: web::Data<DbPool>,
    hb: web::Data<Handlebars<'_>>,
    tz: web::Data<Tz>,
) -> GetResult {
    let logged_user = get_identity(identity);

    #[derive(Serialize)]
    struct MainContext {
        logged_user: Option<LoggedUser>,
        flash_message: String,
        base_url: String,
        contests: Vec<FormattedContest>
    }

    Ok(flash::Response::new(
        None,
        HttpResponse::Ok().body(
        hb.render(
            "main",
            &MainContext {
                flash_message: flash.map_or("".into(), |f| f.into_inner()),
                base_url: env::var("BASE_URL")
                    .expect("BASE_URL environment variable is not set"),
                logged_user,
                contests: get_formatted_contests(&pool, &tz)?,
            },
        )?,
    )))
}

#[get("/contests/")]
async fn get_contests(
    flash: Option<flash::Message<String>>,
    identity: Identity,
    pool: web::Data<DbPool>,
    hb: web::Data<Handlebars<'_>>,
    tz: web::Data<Tz>,
) -> GetResult {
    let logged_user = require_identity(identity)?;

    #[derive(Serialize)]
    struct ContestsContext {
        logged_user: LoggedUser,
        flash_message: String,
        base_url: String,
        contests: Vec<FormattedContest>,
    }

    Ok(flash::Response::new(
        None,
        HttpResponse::Ok().body(
        hb.render(
            "contests",
            &ContestsContext {
                logged_user,
                flash_message: flash.map_or("".into(), |f| f.into_inner()),
                base_url: env::var("BASE_URL")
                    .expect("BASE_URL environment variable is not set"),
                contests: get_formatted_contests(&pool, &tz)?,
            },
        )?,
    )))
}

use crate::models::contest;
use actix_multipart::Multipart;
use futures::StreamExt;
use futures::TryStreamExt;
use regex::Regex;
use std::fs::create_dir_all;
use std::io::Cursor;
use std::io::Read;
use std::io::Write;
use std::iter::FromIterator;
use std::str;
use std::path::PathBuf;

#[post("/contests/")]
async fn create_contest(
    identity: Identity,
    pool: web::Data<DbPool>,
    mut payload: Multipart,
    job_sender: web::Data<Sender<Job>>,
    job_result_sender: web::Data<broadcast::Sender<JobResult>>,
    tz: web::Data<Tz>,
) -> PostResult {
    let logged_user = require_identity(identity)?;
    if !logged_user.is_admin {
      return Err(PostError::Unauthorized(UnauthorizedError {}));
    }

    #[derive(Debug)]
    struct Form {
        name: Option<String>,
        start_instant: Option<String>,
        end_instant: Option<String>,
        polygon_zip: Option<Cursor<Vec<u8>>>,
    }

    let mut form = Form {
        name: None,
        start_instant: None,
        end_instant: None,
        polygon_zip: None,
    };

    while let Ok(Some(mut field)) = payload.try_next().await {
        let mut cursor = Cursor::new(vec![]);
        while let Some(chunk) = field.next().await {
            let data = chunk.unwrap();
            cursor
                .write(&data)
                .map_err(|_| PostError::Validation("Corpo inválido".into()))?;
        }

        cursor.set_position(0);

        fn parse_field(field: &str, cursor: &mut Cursor<Vec<u8>>) -> Result<String, PostError> {
            let mut value = String::new();
            cursor
                .read_to_string(&mut value)
                .map_err(|_| PostError::Validation(format!("Campo {} inválido", field)))?;
            Ok(value)
        }

        match field.content_disposition().unwrap().get_name() {
            Some("name") => form.name = Some(parse_field("name", &mut cursor)?),
            Some("start_instant") => {
                form.start_instant = Some(parse_field("start_instant", &mut cursor)?)
            }
            Some("end_instant") => {
                form.end_instant = Some(parse_field("end_instant", &mut cursor)?)
            }
            Some("polygon_zip") => form.polygon_zip = Some(cursor),
            _ => {}
        }
    }

    let polygon_zip = form
        .polygon_zip
        .ok_or(PostError::Validation("Arquivo não informado".into()))?;
    let imported = import_contest::import_file(polygon_zip)
        .map_err(|e| {
            println!("{}", e);
            PostError::Validation("Não foi possível importar".into())
        })?;
    let connection = pool.get()?;

    let contest = contest::insert_contest(
        &connection,
        contest::NewContest {
            name: form.name.unwrap(),
            start_instant: form.start_instant.and_then(|s| tz.datetime_from_str(&s, "%Y-%m-%d %H:%M:%S").ok()).map(|d| d.naive_utc()),
            end_instant: form.end_instant.and_then(|s| tz.datetime_from_str(&s, "%Y-%m-%d %H:%M:%S").ok()).map(|d| d.naive_utc()),
            creation_instant: Local::now().naive_local(),
            creation_user_id: logged_user.id,
        },
    )?;

    fn polygon_url_to_id_without_revision(url: String) -> String {
        url.replace("https://polygon.codeforces.com/", "polygon.")
            .replace("/", ".")
    }

    let problem_label: HashMap<String, String> =
        HashMap::from_iter(imported.0.problems.problem.iter().map(|problem| {
            (
                polygon_url_to_id_without_revision(problem.url.clone()),
                problem.index.clone(),
            )
        }));

    let mut zip = imported.2;

    lazy_static! {
        static ref CODEFORCES_LANGUAGE_TO_JUGHISTO: HashMap<String, String> = {
            let mut m = HashMap::new();
            m.insert("cpp.g++17".into(), "cpp.17.g++".into());
            m.insert("java.8".into(), "java.8".into());
            m.insert("testlib".into(), "cpp.17.g++".into());
            m
        };
    }

    for (name, metadata) in imported.1 {
        let problem_id_without_revision = polygon_url_to_id_without_revision(metadata.url);
        let problem_id = format!("{}.r{}", problem_id_without_revision, &metadata.revision);

        let files_regex: Regex = Regex::new(&format!(
            concat!(
                "^{}/(",
                r"files/$|",
                r"files/.*\.cpp$|",
                r"files/.*\.h$|",
                r"files/tests/$|",
                r"files/tests/validator-tests/$|",
                r"files/tests/validator-tests/.*$|",
                r"files/tests/validator-tests/.*$|",
                r"solutions/$|",
                r"solutions/.*.cc$|",
                r"solutions/.*.cpp$|",
                r"statements/$|",
                r"statements/.html/.*$|",
                r"tests/$",
                ")"
            ),
            name
        ))
        .unwrap();
        let mut filenames = zip
            .file_names()
            .filter(|name| files_regex.is_match(name))
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        filenames.sort();
        for name in filenames {
            let relative_path = files_regex
                .captures(&name)
                .unwrap()
                .get(1)
                .unwrap()
                .as_str();
            let data_path = format!("/data/{}/{}", problem_id, relative_path);

            if name.ends_with("/") {
                info!("Creating directory {} into {}", name, data_path);
                create_dir_all(data_path)?;
                continue;
            }

            info!("Putting file {} into {}", name, data_path);
            std::io::copy(&mut zip.by_name(&name)?, &mut File::create(data_path)?)?;
        }

        fn map_codeforces_language(input: &String) -> Result<String, PostError> {
            Ok(CODEFORCES_LANGUAGE_TO_JUGHISTO
                .get(input)
                .ok_or_else(|| PostError::Validation(format!("Linguagem {} não suportada", input)))?
                .into())
        }

        let problem = problem::upsert_problem(
            &connection,
            problem::NewProblem {
                id: problem_id.clone(),
                name: metadata.names.name[0].value.clone(),
                memory_limit_bytes: metadata.judging.testset[0]
                    .memory_limit
                    .value
                    .parse()
                    .unwrap(),
                time_limit_ms: metadata.judging.testset[0]
                    .time_limit
                    .value
                    .parse()
                    .unwrap(),
                checker_path: metadata.assets.checker.source.path.clone(),
                checker_language: map_codeforces_language(&metadata.assets.checker.r#type)?,
                validator_path: metadata.assets.validators.validator[0].source.path.clone(),
                validator_language: map_codeforces_language(
                    &metadata.assets.validators.validator[0].source.r#type,
                )?,
                main_solution_path: metadata
                    .assets
                    .solutions
                    .solution
                    .iter()
                    .find(|s| s.tag == "main")
                    .ok_or(PostError::Validation("No main solution".into()))?
                    .source
                    .path.clone(),
                main_solution_language: map_codeforces_language(
                    &metadata
                        .assets
                        .solutions
                        .solution
                        .iter()
                        .find(|s| s.tag == "main")
                        .ok_or(PostError::Validation("No main solution".into()))?
                        .source
                        .r#type,
                )?,
                test_pattern: metadata.judging.testset[0].input_path_pattern.value.clone(),
                test_count: metadata.judging.testset[0]
                    .test_count
                    .value
                    .parse()
                    .unwrap(),
                status: "compiled".into(),
                creation_instant: Local::now().naive_local(),
                creation_user_id: logged_user.id,
            },
        )?;

        for (i, test) in metadata.judging.testset[0].tests.test.iter().enumerate() {
            let i = i + 1;
            let test_path = format!(
                "./{}/{}",
                problem_id,
                import_contest::format_width(&problem.test_pattern, i)
            );

            info!(
                "Iterating through test {} to {:#?}, which is {}",
                i,
                test_path,
                test.method.as_ref().unwrap()
            );
            if test.method.as_ref().unwrap() == "manual" {
                let test_name = PathBuf::from(&name)
                    .join(import_contest::format_width(&problem.test_pattern, i));
                info!("Extracting {:#?} from zip", test_name);
                std::io::copy(
                    &mut zip.by_name(&test_name.to_str().unwrap())?,
                    &mut File::create(PathBuf::from("/data/").join(&test_path))?,
                )?;
            } else {
                let cmd: Vec<_> = test.cmd.as_ref().unwrap().split(" ").collect();
                let run_stats = language::run_cached(
                    &job_sender,
                    &job_result_sender,
                    &"cpp.17.g++".into(),
                    format!("./{}/files/{}.cpp", problem.id, cmd.get(0).unwrap()),
                    cmd[1..].iter().map(|s| s.clone().into()).collect(),
                    None,
                    Some(test_path.clone()),
                    problem.memory_limit_bytes / 1_024,
                    problem.time_limit_ms,
                )
                .await
                .map_err(|_| {
                    PostError::Validation("Couldn't use an intermediate program".into())
                })?;

                if run_stats.result != i32::from(job_result::run_cached::Result::Ok) {
                    return Err(PostError::Validation("Couldn't run an intermediate program".into()));
                }
            }

            let run_stats = language::run_cached(
                &job_sender,
                &job_result_sender,
                &problem.main_solution_language,
                format!("./{}/{}", problem.id, problem.main_solution_path),
                vec![],
                Some(test_path.clone()),
                Some(format!("{}.a", test_path)),
                problem.memory_limit_bytes / 1_024,
                problem.time_limit_ms,
            )
            .await
            .map_err(|_| PostError::Validation("Couldn't run solution on test".into()))?;
            if run_stats.exit_code != 0 {
                return Err(PostError::Validation("Couldn't run solution on test".into()));
            }
        }

        language::judge(
            &job_sender,
            &job_result_sender,
            &problem.main_solution_language,
            fs::read_to_string(
                PathBuf::from(format!("/data/{}/{}", problem.id, problem.main_solution_path))
            )?,
            problem.test_count,
            format!("./{}/{}", problem.id, problem.test_pattern).into(),
            problem.checker_language,
            format!("./{}/{}", problem.id, problem.checker_path).into(),
            problem.memory_limit_bytes / 1_024,
            problem.time_limit_ms,
        )
        .await
        .map_err(|_| PostError::Validation("Couldn't judge main solution".into()))?;


        contest::relate_problem(
            &connection,
            contest::NewContestProblems {
                label: problem_label
                    .get(&problem_id_without_revision)
                    .ok_or(PostError::Validation(
                        "Arquivo não contém problemas listados".into(),
                    ))?
                    .to_string()
                    .to_uppercase(),
                contest_id: contest.id,
                problem_id,
            },
        )?;
    }

    Ok(flash::Response::new(
        None,
        HttpResponse::Ok().body(imported.3),
    ))
}
