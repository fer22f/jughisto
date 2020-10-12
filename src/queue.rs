use crate::language;
use crossbeam::channel::{unbounded, Receiver, Sender};
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
    pub test_path: PathBuf,
}

use crate::isolate;
use crate::isolate::IsolateBox;
use crate::isolate::RunStats;
use crate::isolate::RunStatus;
use crate::language::LanguageParams;
use crate::models::submission::SubmissionCompletion;
use chrono::prelude::*;
use std::collections::HashMap;
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
    let compile_stats = language::compile_source(
        &isolate_executable_path,
        &isolate_box,
        &language,
        &submission.uuid.to_string(),
        &mut Cursor::new(submission.source_text),
    )
    .expect("Crashed while compiling");
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
        let judge_end_instant = Local::now().naive_local();

        let mut last_stderr = String::new();

        for stats in compile_stats {
            let mut stderr_file = stats.stderr;
            let mut stdout_file = stats.stdout;
            let mut stderr = String::new();
            stderr_file.read_to_string(&mut stderr).unwrap_or(0);
            stdout_file.read_to_string(&mut stderr).unwrap_or(0);
            last_stderr = stderr;
        }

        submission_completion_sender
            .send(SubmissionCompletion {
                uuid: submission.uuid.to_string(),
                verdict: "CE".into(),
                judge_start_instant,
                judge_end_instant,
                memory_kib: None,
                time_ms: None,
                time_wall_ms: None,
                error_output: Some(last_stderr),
            })
            .expect("Couldn't send back submission completion");

        return;
    }

    fn count_digits(number: i32) -> usize {
        let mut radix = 1;
        let mut pw10 = 10;
        while number >= pw10 {
            radix += 1;
            pw10 *= 10;
        }
        radix
    }

    let mut last_execute_stats: Option<RunStats<File>> = None;

    let width = count_digits(submission.test_count);
    for i in 1..submission.test_count + 1 {
        let stdin_path = submission
            .test_path
            .join(format!("{:0width$}", i, width = width));
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
        )
        .expect("Crashed while running");
        info!("Run finished: {:#?}", execute_stats);
        last_execute_stats = Some(execute_stats);
    }

    let judge_end_instant = Local::now().naive_local();

    let last_execute_stats = last_execute_stats.unwrap();

    let mut stderr_file = &last_execute_stats.stderr;
    let mut stderr = String::new();
    stderr_file.read_to_string(&mut stderr).unwrap_or(0);

    submission_completion_sender
        .send(SubmissionCompletion {
            uuid: submission.uuid.to_string(),
            verdict: match last_execute_stats.status {
                RunStatus::Ok => "AC",
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
            error_output: Some(stderr),
        })
        .expect("Coudln't send back submission completion");

    isolate::reset(isolate_executable_path, 0);
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
