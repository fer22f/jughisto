use crate::language;
use log::info;
use std::thread;
use uuid::Uuid;

pub struct Submission {
    pub uuid: Uuid,
    pub language: String,
    pub source_text: String,
    pub memory_limit_kib: i32,
    pub time_limit_ms: i32,
    pub test_count: i32,
    pub test_pattern: String,
    pub checker_binary_path: PathBuf,
}

use crate::import_contest;
use crate::isolate;
use crate::isolate::CommandTuple;
use crate::isolate::IsolateBox;
use crate::isolate::RunStats;
use crate::isolate::RunStatus;
use crate::language::LanguageParams;
use crate::models::submission::SubmissionCompletion;
use chrono::prelude::*;
use std::collections::HashMap;
use std::convert::TryInto;
use std::fs;
use std::fs::File;
use std::io::Cursor;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

fn run_loop(
    isolate_executable_path: &PathBuf,
    isolate_box: &IsolateBox,
    languages: &Arc<HashMap<String, LanguageParams>>,
    receiver: &Receiver<Submission>,
    submission_completion_sender: &Sender<SubmissionCompletion>,
) {
    let submission = receiver.recv().expect("Failed to recv in queue channel");

    let judge_start_instant = Local::now().naive_local();

    info!("Starting to compile");
    let language = languages.get(&submission.language).unwrap();
    let compile_source_result = language::compile_source(
        &isolate_executable_path,
        &isolate_box,
        &language,
        &submission.uuid.to_string(),
        &mut Cursor::new(submission.source_text),
    )
    .expect("Crashed while compiling");
    let compile_stats = compile_source_result.compile_stats;
    info!("Compile finished: {:#?}", compile_stats);

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

        let judge_end_instant = Local::now().naive_local();

        let mut stderr = String::new();
        File::open(stats.stderr_path)
            .expect("Stderr should exist")
            .read_to_string(&mut stderr)
            .unwrap_or(0);
        File::open(stats.stdout_path)
            .expect("Stdout should exist")
            .read_to_string(&mut stderr)
            .unwrap_or(0);

        submission_completion_sender
            .send(SubmissionCompletion {
                uuid: submission.uuid.to_string(),
                verdict: "CE".into(),
                judge_start_instant,
                judge_end_instant,
                memory_kib: None,
                time_ms: None,
                time_wall_ms: None,
                error_output: Some(stderr),
            })
            .expect("Couldn't send back submission completion");

        return;
    }

    let mut last_execute_stats: Option<RunStats> = None;

    let mut stderr: Option<String> = None;
    for i in 1..submission.test_count + 1 {
        let stdin_path =
            import_contest::format_width(&submission.test_pattern, i.try_into().unwrap());
        let answer_path = stdin_path.with_extension("a");
        info!(
            "Starting run {}/{} with test {:?}",
            i, submission.test_count, stdin_path
        );
        let execute_stats = language::run(
            &isolate_executable_path,
            &isolate_box,
            &language::ExecuteParams {
                uuid: &submission.uuid.to_string(),
                language: &language,
                memory_limit_kib: submission.memory_limit_kib,
                time_limit_ms: submission.time_limit_ms,
                stdin_path: &stdin_path,
            },
            &compile_source_result.source_path,
            &compile_source_result.program_path,
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
            let mut dest = String::new();
            File::open(&execute_stats.stderr_path)
                .expect("No stderr")
                .read_to_string(&mut dest)
                .unwrap_or(0);
            stderr = Some(dest);
            last_execute_stats = Some(execute_stats);
            break;
        }

        fs::copy(&execute_stats.stdout_path, isolate_box.path.join("stdin")).expect("Copy");

        let checker_stats = isolate::execute(
            &isolate_executable_path,
            &isolate_box,
            &CommandTuple {
                binary_path: PathBuf::from(format!("/data-{}/", &submission.uuid.to_string()))
                    .join(
                        submission
                            .checker_binary_path
                            .strip_prefix("./data")
                            .expect("Should work"),
                    ),
                args: vec![
                    PathBuf::from(format!("/data-{}/", &submission.uuid.to_string()))
                        .join(stdin_path.strip_prefix("./data").expect("Should work"))
                        .to_str()
                        .expect("Should work")
                        .into(),
                    PathBuf::from("/box/stdin")
                        .to_str()
                        .expect("Should work")
                        .into(),
                    PathBuf::from(format!("/data-{}/", &submission.uuid.to_string()))
                        .join(answer_path.strip_prefix("./data").expect("Should work"))
                        .to_str()
                        .expect("Should work")
                        .into(),
                ],
            },
            &isolate::ExecuteParams {
                uuid: &submission.uuid.to_string(),
                memory_limit_kib: submission.memory_limit_kib,
                time_limit_ms: submission.time_limit_ms,
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
            let mut dest = String::new();
            File::open(checker_stats.stderr_path)
                .expect("No stderr")
                .read_to_string(&mut dest)
                .unwrap_or(0);
            stderr = Some(dest);
            last_execute_stats = Some(execute_stats);
            break;
        }

        last_execute_stats = Some(execute_stats);
    }

    let judge_end_instant = Local::now().naive_local();

    let last_execute_stats = last_execute_stats.unwrap();

    submission_completion_sender
        .send(SubmissionCompletion {
            uuid: submission.uuid.to_string(),
            verdict: match last_execute_stats.status {
                RunStatus::Ok if stderr == None => "AC",
                RunStatus::Ok => "WA",
                RunStatus::TimeLimitExceeded => "TL",
                RunStatus::RuntimeError => "RE",
                RunStatus::Signal => "RE",
                RunStatus::MemoryLimitExceeded => "ML",
                RunStatus::FailedToStart => "RE",
            }
            .into(),
            judge_start_instant,
            judge_end_instant,
            memory_kib: last_execute_stats.memory_kib,
            time_ms: last_execute_stats.time_ms,
            time_wall_ms: last_execute_stats.time_wall_ms,
            error_output: stderr,
        })
        .expect("Coudln't send back submission completion");

    isolate::reset(isolate_executable_path, 0).expect("Reset failed");
}

pub fn setup_workers(
    isolate_executable_path: PathBuf,
    languages: Arc<HashMap<String, LanguageParams>>,
) -> (Sender<Submission>, Receiver<SubmissionCompletion>) {
    let (sender, receiver) = unbounded::<Submission>();
    let (submission_completion_sender, submission_completion_receiver) =
        unbounded::<SubmissionCompletion>();

    thread::spawn({
        move || {
            let isolate_box =
                isolate::new_isolate_box(&isolate_executable_path, 0).expect("Couldn't create box");
            let receiver = receiver.clone();
            let submission_completion_sender = submission_completion_sender.clone();
            loop {
                run_loop(
                    &isolate_executable_path,
                    &isolate_box,
                    &languages,
                    &receiver,
                    &submission_completion_sender,
                )
            }
        }
    });

    (sender, submission_completion_receiver)
}
