use crate::language;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Condvar;
use std::sync::Mutex;
use std::thread;

pub struct Submission {
    pub language: String,
    pub source_text: String,
}

pub struct SubmissionQueue {
    jobs: Mutex<VecDeque<Submission>>,
    cvar: Condvar,
}

pub fn enqueue_submission(queue: &SubmissionQueue, submission: Submission) {
    let mut jobs = queue.jobs.lock().unwrap();
    jobs.push_back(submission);
    queue.cvar.notify_all();
}

pub fn wait_for_submissions(queue: &SubmissionQueue) -> Submission {
    let mut jobs = queue.jobs.lock().unwrap();
    loop {
        match jobs.pop_front() {
            Some(job) => return job,
            None => jobs = queue.cvar.wait(jobs).unwrap(),
        }
    }
}

use crate::isolate;
use crate::language::LanguageParams;
use std::collections::HashMap;
use std::io::Cursor;
use std::io::Read;
use std::path::PathBuf;

pub fn setup_workers(
    isolate_executable_path: PathBuf,
    languages: HashMap<String, LanguageParams>,
) -> Arc<SubmissionQueue> {
    let submission_queue = Arc::new(SubmissionQueue {
        jobs: Mutex::new(VecDeque::new()),
        cvar: Condvar::new(),
    });

    thread::spawn({
        let submission_queue = submission_queue.clone();
        move || {
            let isolate_box =
                isolate::create_box(&isolate_executable_path, 0).expect("Couldn't create box");
            loop {
                let submission = wait_for_submissions(&submission_queue);
                let language = languages.get(&submission.language).unwrap();
                let compile_stats = language::compile_source(
                    &isolate_executable_path,
                    &isolate_box,
                    &language,
                    &mut Cursor::new(submission.source_text),
                )
                .expect("Crashed while compiling");

                for stats in compile_stats {
                    println!("{:#?}", stats);
                    let mut stdout_file = stats.stdout;
                    let mut stdout = String::new();
                    stdout_file.read_to_string(&mut stdout).unwrap_or(0);
                    println!("{:#?}", stdout);
                    let mut stderr_file = stats.stderr;
                    let mut stderr = String::new();
                    stderr_file.read_to_string(&mut stderr).unwrap_or(0);
                    println!("{:#?}", stderr);
                }

                let execute_stats = language::run(
                    &isolate_executable_path,
                    &isolate_box,
                    &language,
                    &language::ExecuteParams {
                        memory_limit_mib: 4,
                        time_limit_ms: 1_000,
                    },
                );

                println!("{:#?}", execute_stats);
            }
        }
    });

    return submission_queue.clone();
}
