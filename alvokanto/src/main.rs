use jughisto::job_protocol::{GetJobRequest, JobResult, job_result, job, Language};
use jughisto::job_protocol::job_queue_client::JobQueueClient;
use jughisto::import_contest::format_width;
use std::path::PathBuf;
use which::which;
use chrono::Local;

use tonic::transport::channel::Channel;
use tonic::Status;

mod isolate;
mod language;

use tokio::time::{sleep, Duration};
use isolate::{IsolateBox, new_isolate_box, RunStatus, RunStats, CommandTuple, CompileParams};
use std::fs::read_to_string;
use std::fs;
use log::info;
use language::Compile;
use std::convert::TryInto;
use std::fs::File;
use std::io::Write;

pub fn get_isolate_executable_path() -> PathBuf {
    which("isolate").expect("isolate binary not installed")
}

fn run_cached(
    isolate_executable_path: &PathBuf,
    isolate_box: &IsolateBox,
    supported_languages: &HashMap<String, language::LanguageParams>,
    uuid: String,
    language: String,
    time_limit_ms: i32,
    memory_limit_kib: i32,
    request: job::RunCached
) -> JobResult {
    info!("Starting to run");
    let root_data = PathBuf::from("../data/");

    let language = supported_languages.get(&language);
    if let None = language {
        return JobResult {
            uuid,
            code: job_result::Code::InvalidLanguage.into(),
            which: None,
        };
    }
    let language = language.unwrap();

    let path_with_suffix = PathBuf::from("../data/").join(&request.source_path);
    let path_without_suffix = path_with_suffix.with_extension("");

    if let Compile::Command(_, command, output) = &language.compile {
        let output_path =
            output.replace("{.}", path_with_suffix.to_str().unwrap())
                .replace("{}", path_without_suffix.to_str().unwrap());
        if !root_data.join(&output_path).exists() {
            fs_extra::dir::copy(
                path_with_suffix.parent().unwrap(),
                &isolate_box.path,
                &fs_extra::dir::CopyOptions {
                    overwrite: false,
                    skip_exist: false,
                    buffer_size: 64000, //64kb
                    copy_inside: true,
                    content_only: true,
                    depth: 0,
                }
            ).unwrap();

            let command = CommandTuple {
                binary_path: command.binary_path.clone(),
                args: command
                    .args
                    .iter()
                    .map(|c|
                        c.replace("{.}", path_with_suffix.file_name().unwrap().to_str().unwrap())
                            .replace("{}", path_without_suffix.file_name().unwrap().to_str().unwrap()))
                    .collect(),
            };

            info!("Compiling: {:#?}", command);

            let compile_stats = isolate::compile(
                isolate_executable_path,
                isolate_box,
                CompileParams {
                    uuid: &uuid,
                    // 1GiB
                    memory_limit_kib: 1_024 * 1_024,
                    // 25 seconds
                    time_limit_ms: 25_000,
                    command: &command,
                },
            ).unwrap();

            if match compile_stats {
                RunStats {
                    exit_code: Some(c), ..
                } => c != 0,
                RunStats {
                    exit_code: None, ..
                } => true,
            } {
                fs_extra::dir::create(&isolate_box.path, true).unwrap();
                return JobResult {
                    uuid,
                    code: job_result::Code::Ok.into(),
                    which: Some(job_result::Which::RunCached(job_result::RunCached {
                        result: job_result::run_cached::Result::CompilationError.into(),
                        exit_code: compile_stats.exit_code.unwrap_or(42),
                        exit_signal: compile_stats.exit_signal,
                        memory_kib: compile_stats.memory_kib.unwrap(),
                        time_ms: compile_stats.time_ms.unwrap(),
                        time_wall_ms: compile_stats.time_wall_ms.unwrap(),
                        error_output: read_to_string(compile_stats.stderr_path).unwrap(),
                    }))
                }
            }

            fs::copy(&isolate_box.path.join(
                output.replace("{.}", path_with_suffix.file_name().unwrap().to_str().unwrap())
                .replace("{}", path_without_suffix.file_name().unwrap().to_str().unwrap())), &output_path).unwrap();

            fs_extra::dir::create(&isolate_box.path, true).unwrap();
        }
    }

    let inside_path_with_suffix = PathBuf::from(format!("/data-{}/", uuid)).join(&request.source_path);
    let inside_path_without_suffix = inside_path_with_suffix.with_extension("");

    let command = CommandTuple {
        binary_path: language.run.binary_path.to_str().unwrap()
            .replace("{.}", inside_path_with_suffix.to_str().unwrap())
            .replace("{}", inside_path_without_suffix.to_str().unwrap()).into(),
        args: language.run
            .args
            .iter()
            .map(|c|
            c.replace("{.}", inside_path_with_suffix.to_str().unwrap())
                .replace("{}", inside_path_without_suffix.to_str().unwrap()))
            .chain(request.arguments)
            .collect(),
    };

    info!("Executing");
    let run_stats = isolate::execute(
        &isolate_executable_path,
        &isolate_box,
        &command,
        &isolate::ExecuteParams {
            uuid: &uuid,
            memory_limit_kib: memory_limit_kib,
            time_limit_ms: time_limit_ms,
            stdin_path: request.stdin_path,
            process_limit: language.process_limit,
        },
    ).unwrap();

    if let Some(stdout_path) = request.stdout_path {
        fs::copy(run_stats.stdout_path, root_data.join(stdout_path)).unwrap();
    }

    let error_output = read_to_string(run_stats.stderr_path).unwrap();

    fs_extra::dir::create(&isolate_box.path, true).unwrap();

    JobResult {
        uuid,
        code: job_result::Code::Ok.into(),
        which: Some(job_result::Which::RunCached(job_result::RunCached {
            result: match run_stats.status {
                RunStatus::Ok =>
                    job_result::run_cached::Result::Ok.into(),
                RunStatus::RuntimeError =>
                    job_result::run_cached::Result::RuntimeError.into(),
                RunStatus::TimeLimitExceeded =>
                    job_result::run_cached::Result::TimeLimitExceeded.into(),
                RunStatus::MemoryLimitExceeded =>
                    job_result::run_cached::Result::MemoryLimitExceeded.into(),
            },
            exit_code: run_stats.exit_code.unwrap(),
            exit_signal: run_stats.exit_signal,
            memory_kib: run_stats.memory_kib.unwrap(),
            time_ms: run_stats.time_ms.unwrap(),
            time_wall_ms: run_stats.time_wall_ms.unwrap(),
            error_output,
        })),
    }
}

fn judge(
    isolate_executable_path: &PathBuf,
    isolate_box: &IsolateBox,
    supported_languages: &HashMap<String, language::LanguageParams>,
    uuid: String,
    language: String,
    time_limit_ms: i32,
    memory_limit_kib: i32,
    request: job::Judgement
) -> JobResult {
    let root_data = PathBuf::from("../data/");

    let language = supported_languages.get(&language);
    if let None = language {
        return JobResult {
            uuid,
            code: job_result::Code::InvalidLanguage.into(),
            which: None,
        };
    }
    let language = language.unwrap();

    let checker_language = supported_languages.get(&request.checker_language);
    if let None = checker_language {
        return JobResult {
            uuid,
            code: job_result::Code::InvalidLanguage.into(),
            which: None,
        };
    }
    let checker_language = checker_language.unwrap();

    let path_with_suffix = PathBuf::from("../data/").join(&request.checker_source_path);
    let path_without_suffix = path_with_suffix.with_extension("");

    if let Compile::Command(_, command, output) = &checker_language.compile {
        let output_path =
            output.replace("{.}", path_with_suffix.to_str().unwrap())
                .replace("{}", path_without_suffix.to_str().unwrap());
        if !root_data.join(&output_path).exists() {
            fs_extra::dir::copy(
                path_with_suffix.parent().unwrap(),
                &isolate_box.path,
                &fs_extra::dir::CopyOptions {
                    overwrite: false,
                    skip_exist: false,
                    buffer_size: 64000, //64kb
                    copy_inside: true,
                    content_only: true,
                    depth: 0,
                }
            ).unwrap();

            let command = CommandTuple {
                binary_path: command.binary_path.clone(),
                args: command
                    .args
                    .iter()
                    .map(|c|
                        c.replace("{.}", path_with_suffix.file_name().unwrap().to_str().unwrap())
                            .replace("{}", path_without_suffix.file_name().unwrap().to_str().unwrap()))
                    .collect(),
            };

            info!("Compiling: {:#?}", command);

            let compile_stats = isolate::compile(
                isolate_executable_path,
                isolate_box,
                CompileParams {
                    uuid: &uuid,
                    // 1GiB
                    memory_limit_kib: 1_024 * 1_024,
                    // 25 seconds
                    time_limit_ms: 25_000,
                    command: &command,
                },
            ).unwrap();

            if match compile_stats {
                RunStats {
                    exit_code: Some(c), ..
                } => c != 0,
                RunStats {
                    exit_code: None, ..
                } => true,
            } {
                fs_extra::dir::create(&isolate_box.path, true).unwrap();
                return JobResult {
                    uuid,
                    code: job_result::Code::Ok.into(),
                    which: Some(job_result::Which::RunCached(job_result::RunCached {
                        result: job_result::run_cached::Result::CompilationError.into(),
                        exit_code: compile_stats.exit_code.unwrap_or(42),
                        exit_signal: compile_stats.exit_signal,
                        memory_kib: compile_stats.memory_kib.unwrap(),
                        time_ms: compile_stats.time_ms.unwrap(),
                        time_wall_ms: compile_stats.time_wall_ms.unwrap(),
                        error_output: read_to_string(compile_stats.stderr_path).unwrap(),
                    }))
                }
            }

            fs::copy(&isolate_box.path.join(
                output.replace("{.}", path_with_suffix.file_name().unwrap().to_str().unwrap())
                .replace("{}", path_without_suffix.file_name().unwrap().to_str().unwrap())), &output_path).unwrap();

            fs_extra::dir::create(&isolate_box.path, true).unwrap();
        }
    }

    let judge_start_instant = Local::now().naive_utc();

    if let Compile::Command(transform, command, _) = &language.compile {
        let mut file = File::create(isolate_box.path.join(format!("x{}", language.suffix))).unwrap();
        file.write_all(transform(request.source_text, "x".into()).as_bytes()).unwrap();
        file.sync_data().unwrap();

        let command = CommandTuple {
            binary_path: command.binary_path.clone(),
            args: command
                .args
                .iter()
                .map(|c|
                    c.replace("{.}", &format!("x{}", language.suffix))
                        .replace("{}", "x".into()))
                .collect(),
        };

        let compile_stats = isolate::compile(
            &isolate_executable_path,
            &isolate_box,
            CompileParams {
                uuid: &uuid,
                // 1GiB
                memory_limit_kib: 1_024 * 1_024,
                // 25 seconds
                time_limit_ms: 25_000,
                command: &command,
            },
        )
        .expect("Crashed while compiling");

        info!("Compile finished: {:#?}", compile_stats);

        if match compile_stats {
            RunStats {
                exit_code: Some(c), ..
            } => c != 0,
            RunStats {
                exit_code: None, ..
            } => true,
        } {
            fs_extra::dir::create(&isolate_box.path, true).unwrap();
            return JobResult {
                uuid,
                code: job_result::Code::Ok.into(),
                which: Some(job_result::Which::RunCached(job_result::RunCached {
                    result: job_result::run_cached::Result::CompilationError.into(),
                    exit_code: compile_stats.exit_code.unwrap_or(42),
                    exit_signal: compile_stats.exit_signal,
                    memory_kib: compile_stats.memory_kib.unwrap(),
                    time_ms: compile_stats.time_ms.unwrap(),
                    time_wall_ms: compile_stats.time_wall_ms.unwrap(),
                    error_output: read_to_string(compile_stats.stderr_path).unwrap(),
                }))
            }
        }
    } else {
        let mut file = File::create(isolate_box.path.join(format!("x{}", language.suffix))).unwrap();
        file.write_all(request.source_text.as_bytes()).unwrap();
        file.sync_data().unwrap();
    }

    let mut last_execute_stats: Option<RunStats> = None;

    let command = CommandTuple {
        binary_path: language.run.binary_path.to_str().unwrap()
            .replace("{.}", &format!("x{}", language.suffix))
            .replace("{}", "x".into()).into(),
        args: language.run
            .args
            .iter()
            .map(|c|
            c.replace("{.}", &format!("x{}", language.suffix))
                .replace("{}", "x".into()))
            .collect(),
    };

    let mut error_output: Option<String> = None;
    let mut failed_test: i32 = 0;

    for i in 1..request.test_count + 1 {
        let stdin_path =
            format_width(&request.test_pattern, i.try_into().unwrap());
        let answer_path = format!("{}.a", stdin_path);
        info!(
            "Starting run {}/{} with test {:?}",
            i, request.test_count, stdin_path
        );
        let execute_stats = isolate::execute(
            &isolate_executable_path,
            &isolate_box,
            &command,
            &isolate::ExecuteParams {
                process_limit: language.process_limit,
                uuid: &uuid,
                memory_limit_kib: memory_limit_kib,
                time_limit_ms: time_limit_ms,
                stdin_path: Some(stdin_path.clone()),
            },
        )
        .expect("Crashed while running");
        info!("Run finished: {:#?}", execute_stats);

        if match execute_stats {
            RunStats {
                exit_code: Some(c), ..
            } => c != 0,
            RunStats {
                exit_code: None, ..
            } => true,
        } {
            error_output = Some(read_to_string(&execute_stats.stderr_path).unwrap());
            failed_test = i;
            last_execute_stats = Some(execute_stats);
            break;
        }

        fs::copy(&execute_stats.stdout_path, isolate_box.path.join("stdin")).expect("Copy");

        // TODO: Support non-compile based languages
        let command = CommandTuple {
            binary_path: PathBuf::from(format!("/data-{}/", &uuid))
                .join(&request.checker_source_path).with_extension(""),
            args: vec![
                PathBuf::from(format!("/data-{}/", &uuid))
                    .join(&stdin_path)
                    .to_str()
                    .expect("Should work")
                    .into(),
                PathBuf::from("/box/stdin")
                    .to_str()
                    .expect("Should work")
                    .into(),
                PathBuf::from(format!("/data-{}/", &uuid))
                    .join(&answer_path)
                    .to_str()
                    .expect("Should work")
                    .into(),
            ],
        };

        info!("Executing checker: {:?}", command);
        let checker_stats = isolate::execute(
            &isolate_executable_path,
            &isolate_box,
            &command,
            &isolate::ExecuteParams {
                uuid: &uuid,
                memory_limit_kib: memory_limit_kib,
                time_limit_ms: time_limit_ms,
                stdin_path: None,
                process_limit: 1,
            },
        )
        .expect("Crashed while running");
        if match checker_stats {
            RunStats {
                exit_code: Some(c), ..
            } => c != 0,
            RunStats {
                exit_code: None, ..
            } => true,
        } {
            error_output = Some(read_to_string(checker_stats.stderr_path).unwrap());
            failed_test = i;
            last_execute_stats = Some(execute_stats);
            break;
        }

        last_execute_stats = Some(execute_stats);
    }

    let judge_end_instant = Local::now().naive_utc();

    let last_execute_stats = last_execute_stats.unwrap();

    return JobResult {
        uuid: uuid,
        code: job_result::Code::Ok.into(),
        which: Some(job_result::Which::Judgement(
            job_result::Judgement {
                verdict: match last_execute_stats.status {
                    RunStatus::Ok => match failed_test {
                        0 => job_result::judgement::Verdict::Accepted.into(),
                        _ => job_result::judgement::Verdict::WrongAnswer.into(),
                    },
                    RunStatus::TimeLimitExceeded => job_result::judgement::Verdict::TimeLimitExceeded.into(),
                    RunStatus::MemoryLimitExceeded => job_result::judgement::Verdict::MemoryLimitExceeded.into(),
                    RunStatus::RuntimeError => job_result::judgement::Verdict::RuntimeError.into(),
                },
                failed_test,
                exit_signal: last_execute_stats.exit_signal,
                memory_kib: last_execute_stats.memory_kib.unwrap(),
                exit_code: last_execute_stats.exit_code.unwrap_or(42),
                time_ms: last_execute_stats.time_ms.unwrap(),
                time_wall_ms: last_execute_stats.time_wall_ms.unwrap(),
                error_output: error_output.unwrap_or("".into()),
                judge_start_instant: judge_start_instant.format("%Y-%m-%dT%H:%M:%S%.f").to_string(),
                judge_end_instant: judge_end_instant.format("%Y-%m-%dT%H:%M:%S%.f").to_string(),
            }
            ))
    }
}

async fn job_loop(
    isolate_executable_path: &PathBuf,
    isolate_box: &IsolateBox,
    supported_languages: &HashMap<String, language::LanguageParams>,
    mut client: JobQueueClient<Channel>
) -> Result<(), Status> {
    loop {
        log::info!("Waiting for job");
        let job = client.get_job(GetJobRequest {
            supported_languages: supported_languages.iter().map(|(key, language)| Language {
                key: key.clone(),
                name: language.name.clone(),
                order: language.order,
            }).collect(),
        }).await?.into_inner();
        log::info!("Got job uuid={}", job.uuid);

        match job.which {
            Some(job::Which::Judgement(judgement_request)) => {
                client.submit_job_result(judge(
                    isolate_executable_path,
                    isolate_box,
                    supported_languages,
                    job.uuid,
                    job.language,
                    job.time_limit_ms,
                    job.memory_limit_kib,
                    judgement_request
                )).await?;
            },
            Some(job::Which::RunCached(run_request)) => {
                client.submit_job_result(run_cached(
                    isolate_executable_path,
                    isolate_box,
                    supported_languages,
                    job.uuid,
                    job.language,
                    job.time_limit_ms,
                    job.memory_limit_kib,
                    run_request
                )).await?;
            },
            None => {
                log::info!("Empty job!");
            }
        }
    }
}

use std::collections::HashMap;

#[tokio::main]
async fn main() {
    std::env::set_var("RUST_LOG", "info");
    env_logger::init();

    let isolate_executable_path = get_isolate_executable_path();
    log::info!("Found isolate at {:?}", isolate_executable_path);
    let isolate_box = new_isolate_box(&isolate_executable_path, 0).expect("Couldn't create an isolate box");
    log::info!("Created an isolate box at {:?}", isolate_box.path);

    let supported_languages = language::get_supported_languages();
    log::info!("Loaded {} supported languages", supported_languages.len());
    for (key, language) in supported_languages.iter() {
        log::trace!("Supported language {} ({}): {}", key, language.order, language.name);
    }

    loop {
        match JobQueueClient::connect("http://jughisto:50051").await {
            Err(e) => { log::info!("Failed to connnect: {}, trying again in 3 seconds", e); },
            Ok(client) => {
                log::info!("Connected to jughisto");
                match job_loop(
                    &isolate_executable_path,
                    &isolate_box,
                    &supported_languages,
                    client
                ).await {
                    Err(e) => { log::error!("On job loop {}, trying again in 3 seconds", e); },
                    _ => {}
                }
            }
        }
        sleep(Duration::from_millis(3000)).await;
    }
}
