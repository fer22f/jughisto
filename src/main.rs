// TODO: Remove this in the next release of diesel
#[macro_use]
extern crate diesel;

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use serde_json::json;

use actix_files::Files;
use actix_identity::Identity;
use actix_web::middleware::errhandlers::{ErrorHandlerResponse, ErrorHandlers};
use actix_web::{dev, get, http, middleware, post, web, App, HttpServer};
use diesel::SqliteConnection;
use std::env;
use std::fs;
use std::fs::File;
use std::io;
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
mod isolate;
mod language;
mod models;
mod queue;
mod schema;
mod setup;

use broadcaster::Broadcaster;
use listenfd::ListenFd;
use std::sync::Mutex;
use std::thread;
type DbPool = r2d2::Pool<ConnectionManager<SqliteConnection>>;
use chrono_tz::Tz;
use isolate::RunStats;
use std::time::Duration;

#[actix_web::main]
async fn main() -> io::Result<()> {
    setup::setup_dotenv();

    std::env::set_var("RUST_LOG", "info");
    env_logger::init();

    let private_key = env::var("IDENTITY_SECRET_KEY")
        .expect("IDENTITY_SECRET_KEY environment variable is not set");

    let database_url =
        env::var("DATABASE_URL").expect("DATABASE_URL environment variable is not set");
    let manager = ConnectionManager::<SqliteConnection>::new(database_url);
    let pool = r2d2::Pool::builder()
        .max_size(1)
        .connection_timeout(Duration::from_secs(1))
        .build(manager)
        .expect("Failed to create pool.");

    setup::setup_admin(&pool.get().expect("Couldn't get connection from the pool"));

    let mut handlebars = Handlebars::new();
    handlebars
        .register_templates_directory(".html.hbs", "./templates")
        .expect("Couldn't find templates directory");
    let handlebars_ref = web::Data::new(handlebars);

    let isolate_executable_path = setup::get_isolate_executable_path();
    let languages = language::get_supported_languages();
    let (channel, submission_completion_channel) =
        queue::setup_workers(isolate_executable_path, languages.clone());

    let broadcaster = Broadcaster::create();

    let submission_pool = pool.clone();
    let submission_broadcaster = broadcaster.clone();
    thread::spawn(move || loop {
        let submission_completion = submission_completion_channel
            .recv()
            .expect("Failed to recv from submission completion channel");
        let connection = submission_pool
            .get()
            .expect("Couldn't get connection from the pool");
        let uuid = String::from(&submission_completion.uuid);
        submission::complete_submission(&connection, submission_completion)
            .expect("Couldn't complete submission");
        submission_broadcaster
            .lock()
            .expect("Submission broadcaster is not active")
            .send("update_submission", &uuid);
    });

    let tz: Tz = env::var("TZ")
        .expect("TZ environment variable is not set")
        .parse()
        .expect("Invalid timezone in environment variable TZ");

    let mut listenfd = ListenFd::from_env();
    let mut server = HttpServer::new(move || {
        App::new()
            .data(pool.clone())
            .data(SubmissionState {
                channel: channel.clone(),
                languages: languages.clone(),
            })
            .data(languages.clone())
            .data(tz.clone())
            .wrap(ErrorHandlers::new().handler(http::StatusCode::UNAUTHORIZED, render_401))
            .wrap(ErrorHandlers::new().handler(http::StatusCode::BAD_REQUEST, render_400))
            .wrap(actix_flash::Flash::default())
            .wrap(IdentityService::new(
                CookieIdentityPolicy::new(&private_key.as_bytes())
                    .name("user")
                    .secure(false),
            ))
            .wrap(CookieSession::signed(&private_key.as_bytes()).secure(false))
            .wrap(middleware::Logger::default())
            .app_data(broadcaster.clone())
            .app_data(handlebars_ref.clone())
            .service(get_login)
            .service(post_login)
            .service(manage_contests)
            .service(get_contest_by_id)
            .service(get_submissions)
            .service(create_submission)
            .service(create_contest)
            .service(submission_updates)
            .service(Files::new("/static/", "./static/"))
    });

    server = if let Some(l) = listenfd
        .take_tcp_listener(0)
        .expect("Can't take TCP listener from listenfd")
    {
        server.listen(l)?
    } else {
        server.bind("0.0.0.0:8000")?
    };

    server.run().await
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
    #[error(transparent)]
    Web(#[from] actix_web::Error),
    #[error(transparent)]
    Queue(#[from] crossbeam::channel::SendError<queue::Submission>),
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
            | PostError::Zip(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

type PostResult = Result<actix_flash::Response<HttpResponse, String>, PostError>;

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
}

impl actix_web::error::ResponseError for GetError {
    fn error_response(&self) -> HttpResponse {
        error_response_and_log(self)
    }

    fn status_code(&self) -> StatusCode {
        match *self {
            GetError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            GetError::Render(_)
            | GetError::Actix(_)
            | GetError::Diesel(_)
            | GetError::R2d2Pool(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

type GetResult = Result<HttpResponse, GetError>;

#[get("/login")]
async fn get_login(
    flash: Option<actix_flash::Message<String>>,
    hb: web::Data<Handlebars<'_>>,
) -> GetResult {
    Ok(HttpResponse::Ok().body(hb.render(
        "login",
        &json!({
            "flash_message": flash.map_or("".into(), |f| f.into_inner())
        }),
    )?))
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

fn render_400(
    mut res: dev::ServiceResponse<dev::Body>,
) -> actix_web::Result<ErrorHandlerResponse<dev::Body>> {
    Ok(ErrorHandlerResponse::Future(
        async move {
            let response = redirect_to_referer(
                match res.take_body() {
                    actix_web::dev::ResponseBody::Body(actix_web::dev::Body::Bytes(bytes)) => {
                        String::from_utf8((&bytes).to_vec()).unwrap()
                    }
                    _ => "Entrada inválida".into(),
                },
                res.request(),
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

use models::problem;
use models::problem::ProblemByContest;

fn get_identity(identity: Identity) -> Result<LoggedUser, UnauthorizedError> {
    let identity = identity.identity().ok_or(UnauthorizedError {})?;
    serde_json::from_str(&identity).map_err(|_| UnauthorizedError {})
}

#[get("/contests/{id}")]
async fn get_contest_by_id(
    identity: Identity,
    pool: web::Data<DbPool>,
    hb: web::Data<Handlebars<'_>>,
    languages: web::Data<Arc<HashMap<String, LanguageParams>>>,
    session: Session,
    path: web::Path<(i32,)>,
    tz: web::Data<Tz>,
) -> GetResult {
    get_identity(identity)?;

    #[derive(Serialize)]
    struct Language {
        order: i32,
        name: String,
        value: String,
    }

    #[derive(Serialize)]
    struct ContestContext {
        languages: Vec<Language>,
        language: Option<String>,
        problems: Vec<ProblemByContest>,
        submissions: Vec<FormattedSubmission>,
    }

    let mut languages = languages
        .iter()
        .map(|(value, language_params)| Language {
            order: language_params.order,
            value: value.into(),
            name: language_params.name.clone(),
        })
        .collect::<Vec<_>>();
    languages.sort_by(|a, b| a.order.cmp(&b.order));

    let connection = pool.get()?;
    let problems = problem::get_problems_by_contest_id(&connection, path.into_inner().0)?;
    let submissions = submission::get_submissions(&connection)?;

    Ok(HttpResponse::Ok().body(
        hb.render(
            "contest",
            &ContestContext {
                languages,
                problems,
                language: session.get("language")?,
                submissions: submissions
                    .iter()
                    .map(|s| format_submission(&tz, s))
                    .collect(),
            },
        )?,
    ))
}

#[derive(Serialize, Deserialize)]
struct LoggedUser {
    id: i32,
    name: String,
    is_admin: bool,
}

#[post("/login")]
async fn post_login(
    identity: Identity,
    pool: web::Data<DbPool>,
    form: web::Form<LoginForm>,
) -> PostResult {
    let connection = pool.get()?;

    use actix_web::error::BlockingError;
    use user::PasswordMatched;
    use user::UserHashingError;
    match web::block(move || user::check_matching_password(&connection, &form.name, &form.password))
        .await
        .map_err(|e| match e {
            BlockingError::Error(UserHashingError::Database(e)) => PostError::Database(e),
            BlockingError::Error(UserHashingError::Hash(_)) => {
                PostError::Validation("Senha inválida".into())
            }
            BlockingError::Canceled => PostError::Web(e.into()),
        })? {
        PasswordMatched::UserDoesntExist => {
            Err(PostError::Validation("Usuário inexistente".into()))
        }
        PasswordMatched::PasswordDoesntMatch => {
            Err(PostError::Validation("Senha incorreta".into()))
        }
        PasswordMatched::PasswordMatches(user) => {
            identity.remember(
                serde_json::to_string(&LoggedUser {
                    id: user.id,
                    name: (&user.name).into(),
                    is_admin: user.is_admin,
                })
                .map_err(|_| PostError::Custom("Usuário no banco de dados inconsistente".into()))?,
            );
            Ok(actix_flash::Response::with_redirect("".into(), "/"))
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
        .header("content-type", "text/event-stream")
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

fn format_submission(tz: &Tz, submission: &models::submission::Submission) -> FormattedSubmission {
    FormattedSubmission {
        uuid: (&submission.uuid).into(),
        verdict: submission
            .verdict
            .as_ref()
            .map(|s| String::from(s))
            .unwrap_or("WJ".into())
            .into(),
        problem_label: "A".into(),
        submission_instant: format_utc_date_time(tz, submission.submission_instant),
        error_output: submission.error_output.as_ref().map(|s| s.into()),
    }
}

#[get("/submissions/")]
async fn get_submissions(
    identity: Identity,
    pool: web::Data<DbPool>,
    hb: web::Data<Handlebars<'_>>,
    tz: web::Data<Tz>,
) -> GetResult {
    get_identity(identity)?;
    let connection = pool.get()?;

    #[derive(Serialize)]
    struct SubmissionsContext {
        submissions: Vec<FormattedSubmission>,
    }

    let submissions = submission::get_submissions(&connection)?;

    Ok(HttpResponse::Ok().body(
        hb.render(
            "submissions",
            &SubmissionsContext {
                submissions: submissions
                    .iter()
                    .map(|s| format_submission(&tz, s))
                    .collect(),
            },
        )?,
    ))
}

#[derive(Serialize, Deserialize)]
struct SubmissionForm {
    contest_problem_id: i32,
    language: String,
    source_text: String,
}

use crossbeam::channel::Sender;
use language::LanguageParams;
use queue::Submission;
use std::collections::HashMap;
use std::sync::Arc;

struct SubmissionState {
    channel: Sender<Submission>,
    languages: Arc<HashMap<String, LanguageParams>>,
}

use actix_web::HttpRequest;

fn redirect_to_referer(message: String, request: &HttpRequest) -> PostResult {
    let referer = request
        .headers()
        .get("Referer")
        .ok_or(PostError::Validation(
            "Cabeçalho Referer inexistente".into(),
        ))?;
    let referer_str = referer
        .to_str()
        .map_err(|_| PostError::Validation("Cabeçalho Referer inválido".into()))?;
    Ok(actix_flash::Response::with_redirect(message, referer_str))
}

#[post("/submissions/")]
async fn create_submission(
    identity: Identity,
    form: web::Form<SubmissionForm>,
    submission_state: web::Data<SubmissionState>,
    pool: web::Data<DbPool>,
    session: Session,
    request: HttpRequest,
) -> PostResult {
    let identity = get_identity(identity)?;
    let connection = pool.get()?;

    submission_state
        .languages
        .get(&form.language)
        .ok_or(PostError::Validation("Linguagem inexistente".into()))?;

    let uuid = Uuid::new_v4();
    submission::insert_submission(
        &connection,
        submission::NewSubmission {
            uuid: uuid.to_string(),
            source_text: (&form.source_text).into(),
            language: (&form.language).into(),
            submission_instant: Local::now().naive_local(),
            contest_problem_id: form.contest_problem_id,
            user_id: identity.id,
        },
    )?;

    let metadata =
        problem::get_problem_by_contest_id_metadata(&connection, form.contest_problem_id)?;

    submission_state.channel.send(Submission {
        uuid,
        language: (&form.language).into(),
        source_text: (&form.source_text).into(),
        time_limit_ms: metadata.time_limit_ms,
        memory_limit_kib: metadata.memory_limit_bytes / 1_024,
        test_count: metadata.test_count,
        test_pattern: format!("./data/{}/{}", metadata.id, metadata.test_pattern).into(),
        checker_binary_path: format!("./data/{}/checker", metadata.id).into(),
    })?;

    session.set("language", &form.language)?;

    redirect_to_referer(format!("Submetido {} com sucesso!", uuid), &request)
}

#[get("/contests/")]
async fn manage_contests(
    identity: Identity,
    pool: web::Data<DbPool>,
    hb: web::Data<Handlebars<'_>>,
    tz: web::Data<Tz>,
) -> GetResult {
    get_identity(identity)?;
    let connection = pool.get()?;
    let contests = contest::get_contests(&connection)?;

    #[derive(Serialize)]
    struct FormattedContest {
        pub id: i32,
        pub name: String,
        pub start_instant: Option<String>,
        pub end_instant: Option<String>,
        pub creation_instant: String,
    }

    #[derive(Serialize)]
    struct ContestsContext {
        contests: Vec<FormattedContest>,
    }

    Ok(HttpResponse::Ok().body(
        hb.render(
            "contests",
            &ContestsContext {
                contests: contests
                    .iter()
                    .map(|c| FormattedContest {
                        id: c.id,
                        name: c.name.clone(),
                        start_instant: c.start_instant.map(|i| format_utc_date_time(&tz, i)),
                        end_instant: c.end_instant.map(|i| format_utc_date_time(&tz, i)),
                        creation_instant: format_utc_date_time(&tz, c.creation_instant),
                    })
                    .collect(),
            },
        )?,
    ))
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

#[post("/contests/")]
async fn create_contest(
    identity: Identity,
    pool: web::Data<DbPool>,
    mut payload: Multipart,
    languages: web::Data<Arc<HashMap<String, LanguageParams>>>,
) -> PostResult {
    let identity = get_identity(identity)?;

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
        .map_err(|_| PostError::Validation("Não foi possível importar".into()))?;
    let connection = pool.get()?;

    let contest = contest::insert_contest(
        &connection,
        contest::NewContest {
            name: form.name.unwrap(),
            start_instant: form.start_instant.and_then(|s| s.parse().ok()),
            end_instant: form.end_instant.and_then(|s| s.parse().ok()),
            creation_instant: Local::now().naive_local(),
            creation_user_id: identity.id,
        },
    )?;

    fn polygon_url_to_id_without_revision(url: String) -> String {
        url.replace("https://polygon.codeforces.com/", "polygon:")
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
            let data_path = format!("./data/{}/{}", problem_id, relative_path);

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
                    .path
                    .clone(),
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
                creation_user_id: identity.id,
            },
        )?;

        let isolate_executable_path = setup::get_isolate_executable_path();
        let isolate_box = isolate::new_isolate_box(&isolate_executable_path, 1)
            .map_err(|_| PostError::Validation("Can't create isolate box".into()))?;

        use std::path::PathBuf;

        let uuid = "import-solution";
        let compile_stats = language::compile(
            &isolate_executable_path,
            &isolate_box,
            languages.get(&problem.checker_language).unwrap(),
            &uuid,
            &PathBuf::from("/data-import-solution/")
                .join(&problem.id)
                .join(problem.checker_path),
            &PathBuf::from("/box/program"),
        )
        .map_err(|_| PostError::Validation("Couldn't compile checker".into()))?;
        if match compile_stats {
            None => false,
            Some(RunStats {
                exit_code: Some(c), ..
            }) => c != 0,
            Some(RunStats {
                exit_code: None, ..
            }) => true,
        } {
            let stats = compile_stats.unwrap();
            let mut stderr = String::new();
            File::open(&stats.stderr_path)
                .expect("Stderr should exist")
                .read_to_string(&mut stderr)
                .unwrap_or(0);
            File::open(&stats.stdout_path)
                .expect("Stdout should exist")
                .read_to_string(&mut stderr)
                .unwrap_or(0);
            info!("Couldn't compile: {:#?}", stats);
            info!("Stderr: {}", stderr);
            return Err(PostError::Validation("Couldn't compile checker".into()));
        }
        fs::copy(
            isolate_box.path.join("program"),
            &PathBuf::from("./data").join(&problem.id).join("checker"),
        )?;

        let compile_stats = language::compile(
            &isolate_executable_path,
            &isolate_box,
            languages.get(&problem.validator_language).unwrap(),
            &uuid,
            &PathBuf::from("/data-import-solution/")
                .join(&problem.id)
                .join(problem.validator_path),
            &PathBuf::from("/box/program"),
        )
        .map_err(|_| PostError::Validation("Couldn't compile validator".into()))?;
        if match compile_stats {
            None => false,
            Some(RunStats {
                exit_code: Some(c), ..
            }) => c != 0,
            Some(RunStats {
                exit_code: None, ..
            }) => true,
        } {
            let stats = compile_stats.unwrap();
            let mut stderr = String::new();
            File::open(&stats.stderr_path)
                .expect("Stderr should exist")
                .read_to_string(&mut stderr)
                .unwrap_or(0);
            File::open(&stats.stdout_path)
                .expect("Stdout should exist")
                .read_to_string(&mut stderr)
                .unwrap_or(0);
            info!("Couldn't compile: {:#?}", stats);
            info!("Stderr: {}", stderr);
            return Err(PostError::Validation("Couldn't compile validator".into()));
        }
        fs::copy(
            isolate_box.path.join("program"),
            &PathBuf::from("./data").join(&problem.id).join("validator"),
        )?;

        let compile_stats = language::compile(
            &isolate_executable_path,
            &isolate_box,
            languages.get(&problem.main_solution_language).unwrap(),
            &uuid,
            &PathBuf::from("/data-import-solution/")
                .join(&problem.id)
                .join(problem.main_solution_path),
            &PathBuf::from("/box/program"),
        )
        .map_err(|_| PostError::Validation("Couldn't compile main solution".into()))?;
        if match compile_stats {
            None => false,
            Some(RunStats {
                exit_code: Some(c), ..
            }) => c != 0,
            Some(RunStats {
                exit_code: None, ..
            }) => true,
        } {
            let stats = compile_stats.unwrap();
            let mut stderr = String::new();
            File::open(&stats.stderr_path)
                .expect("Stderr should exist")
                .read_to_string(&mut stderr)
                .unwrap_or(0);
            File::open(&stats.stdout_path)
                .expect("Stdout should exist")
                .read_to_string(&mut stderr)
                .unwrap_or(0);
            info!("Couldn't compile: {:#?}", stats);
            info!("Stderr: {}", stderr);
            return Err(PostError::Validation(
                "Couldn't compile main solution".into(),
            ));
        }
        fs::copy(
            isolate_box.path.join("program"),
            &PathBuf::from("./data")
                .join(&problem.id)
                .join("main_solution"),
        )?;

        for (i, test) in metadata.judging.testset[0].tests.test.iter().enumerate() {
            let i = i + 1;
            let test_path = PathBuf::from(format!("./data/{}", problem_id))
                .join(import_contest::format_width(&problem.test_pattern, i));

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
                    &mut File::create(&test_path)?,
                )?;
            } else {
                let cmd: Vec<_> = test.cmd.as_ref().unwrap().split(" ").collect();
                let binary_path =
                    PathBuf::from(format!("./data/{}", problem_id)).join(cmd.get(0).unwrap());
                if !binary_path.exists() {
                    let compile_stats = language::compile(
                        &isolate_executable_path,
                        &isolate_box,
                        languages.get("cpp.17.g++").unwrap(),
                        &uuid,
                        &PathBuf::from("/data-import-solution/")
                            .join(&problem.id)
                            .join("files")
                            .join(cmd.get(0).unwrap())
                            .with_extension("cpp"),
                        &PathBuf::from("/box/program"),
                    )
                    .map_err(|e| {
                        info!("{}", e);
                        PostError::Validation("Couldn't compile an intermediate program".into())
                    })?;
                    if match compile_stats {
                        None => false,
                        Some(RunStats {
                            exit_code: Some(c), ..
                        }) => c != 0,
                        Some(RunStats {
                            exit_code: None, ..
                        }) => true,
                    } {
                        let stats = compile_stats.unwrap();
                        let mut stderr = String::new();
                        File::open(&stats.stderr_path)
                            .expect("Stderr should exist")
                            .read_to_string(&mut stderr)
                            .unwrap_or(0);
                        File::open(&stats.stdout_path)
                            .expect("Stdout should exist")
                            .read_to_string(&mut stderr)
                            .unwrap_or(0);
                        info!("Couldn't compile: {:#?}", stats);
                        info!("Stderr: {}", stderr);
                        return Err(PostError::Validation(
                            "Couldn't compile an intermediate program".into(),
                        ));
                    }
                    fs::copy(isolate_box.path.join("program"), &binary_path)?;
                }

                let run_stats = isolate::execute(
                    &isolate_executable_path,
                    &isolate_box,
                    &isolate::CommandTuple {
                        binary_path: PathBuf::from("/data-import-solution/")
                            .join(binary_path.strip_prefix("./data").unwrap()),
                        args: cmd[1..].iter().map(|s| s.clone().into()).collect(),
                    },
                    &isolate::ExecuteParams {
                        uuid: &uuid,
                        memory_limit_kib: problem.memory_limit_bytes / 1_024,
                        time_limit_ms: problem.time_limit_ms,
                        stdin_path: None,
                        process_limit: 1,
                    },
                )
                .map_err(|_| {
                    PostError::Validation("Couldn't use an intermediate program".into())
                })?;
                if match run_stats {
                    RunStats {
                        exit_code: Some(c), ..
                    } => c != 0,
                    RunStats {
                        exit_code: None, ..
                    } => true,
                } {
                    let stats = run_stats;
                    let mut stderr = String::new();
                    File::open(&stats.stderr_path)
                        .expect("Stderr should exist")
                        .read_to_string(&mut stderr)
                        .unwrap_or(0);
                    File::open(&stats.stdout_path)
                        .expect("Stdout should exist")
                        .read_to_string(&mut stderr)
                        .unwrap_or(0);
                    info!("Couldn't compile: {:#?}", stats);
                    info!("Stderr: {}", stderr);
                    return Err(PostError::Validation(
                        "Couldn't use an intermediate program".into(),
                    ));
                }
                fs::copy(isolate_box.path.join("stdout"), &test_path)?;
            }

            let run_stats = isolate::execute(
                &isolate_executable_path,
                &isolate_box,
                &isolate::CommandTuple {
                    binary_path: format!("/data-import-solution/{}/main_solution", problem_id)
                        .into(),
                    args: vec![],
                },
                &isolate::ExecuteParams {
                    uuid: &uuid,
                    memory_limit_kib: problem.memory_limit_bytes / 1_024,
                    time_limit_ms: problem.time_limit_ms,
                    stdin_path: Some(&test_path),
                    process_limit: 1,
                },
            )
            .map_err(|_| PostError::Validation("Couldn't run solution on test".into()))?;
            if match run_stats {
                RunStats {
                    exit_code: Some(c), ..
                } => c != 0,
                RunStats {
                    exit_code: None, ..
                } => true,
            } {
                let stats = run_stats;
                let mut stderr = String::new();
                File::open(&stats.stderr_path)
                    .expect("Stderr should exist")
                    .read_to_string(&mut stderr)
                    .unwrap_or(0);
                File::open(&stats.stdout_path)
                    .expect("Stdout should exist")
                    .read_to_string(&mut stderr)
                    .unwrap_or(0);
                info!("Couldn't compile: {:#?}", stats);
                info!("Stderr: {}", stderr);
                return Err(PostError::Validation(
                    "Couldn't run main solution on test".into(),
                ));
            }
            fs::copy(run_stats.stdout_path, test_path.with_extension("a"))?;
        }

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

    Ok(actix_flash::Response::new(
        None,
        HttpResponse::Ok().body(imported.3),
    ))
}
