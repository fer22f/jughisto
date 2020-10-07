use crate::language;
use crossbeam::channel::{unbounded, Sender, Receiver};
use std::thread;
use uuid::Uuid;

pub struct Submission {
    pub uuid: Uuid,
    pub language: String,
    pub source_text: String,
}

use crate::isolate;
use crate::isolate::RunStatus;
use crate::language::LanguageParams;
use crate::models::submission::SubmissionCompletion;
use chrono::prelude::*;
use std::collections::HashMap;
use std::io::Cursor;
use std::io::Read;
use std::path::PathBuf;

pub fn setup_workers(
    isolate_executable_path: PathBuf,
    languages: HashMap<String, LanguageParams>,
) -> (Sender<Submission>, Receiver<SubmissionCompletion>) {
    let (sender, receiver) = unbounded::<Submission>();
    let (
        submission_completion_sender,
        submission_completion_receiver
    ) = unbounded::<SubmissionCompletion>();

    thread::spawn({
        let channel = receiver.clone();
        move || {
            let isolate_box =
                isolate::create_box(&isolate_executable_path, 0).expect("Couldn't create box");
            loop {
                let submission = channel.recv().expect("Failed to recv in queue channel");

                let judge_start_instant = Local::now().naive_local();

                let language = languages.get(&submission.language).unwrap();
                let compile_stats = language::compile_source(
                    &isolate_executable_path,
                    &isolate_box,
                    &language,
                    &mut Cursor::new(submission.source_text),
                )
                .expect("Crashed while compiling");

                if match compile_stats.last().unwrap().exit_code {
                    Some(c) => c != 0,
                    None => true,
                } {
                    let judge_end_instant = Local::now().naive_local();

                    let mut last_stderr = String::new();

                    for stats in compile_stats {
                        let mut stderr_file = stats.stderr;
                        let mut stderr = String::new();
                        stderr_file.read_to_string(&mut stderr).unwrap_or(0);
                        last_stderr = stderr;
                    }

                    submission_completion_sender.send(
                        SubmissionCompletion {
                            uuid: submission.uuid.to_string(),
                            verdict: "CE".into(),
                            judge_start_instant,
                            judge_end_instant,
                            memory_kib: None,
                            time_ms: None,
                            time_wall_ms: None,
                            compilation_stderr: Some(last_stderr),
                        },
                    ).expect("Couldn't send back submission completion");

                    continue;
                }

                let execute_stats = language::run(
                    &isolate_executable_path,
                    &isolate_box,
                    &language,
                    &language::ExecuteParams {
                        memory_limit_mib: 4,
                        time_limit_ms: 1_000,
                    },
                )
                .expect("Crashed while running");

                let judge_end_instant = Local::now().naive_local();

                submission_completion_sender.send(
                    SubmissionCompletion {
                        uuid: submission.uuid.to_string(),
                        verdict: match execute_stats.status {
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
                        memory_kib: execute_stats.memory_kib,
                        time_ms: execute_stats.time_ms,
                        time_wall_ms: execute_stats.time_wall_ms,
                        compilation_stderr: None,
                    },
                ).expect("Coudln't send back submission completion");
            }
        }
    });

    (sender, submission_completion_receiver)
}
