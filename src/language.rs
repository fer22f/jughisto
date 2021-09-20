use crate::queue::job_protocol::job_result;
use crate::queue::job_protocol::{job, Job, JobResult};
use async_channel::Sender;
use tokio::sync::broadcast;
use uuid::Uuid;
use thiserror::Error;
use log::info;

#[derive(Error, Debug, Clone)]
#[error("job failed")]
struct JobFailedError;

pub async fn run_cached(
    job_sender: &Sender<Job>,
    job_result_sender: &broadcast::Sender<JobResult>,
    language: &String,
    source_path: String,
    arguments: Vec<String>,
    stdin_path: Option<String>,
    stdout_path: Option<String>,
    memory_limit_kib: i32,
    time_limit_ms: i32,
) -> Result<job_result::RunCached, Box<dyn std::error::Error>> {
    let uuid = Uuid::new_v4();
    let mut job_result_receiver = job_result_sender.subscribe();
    info!("Sending job");
    job_sender.send(Job {
        uuid: uuid.to_string(),
        language: language.to_string(),
        memory_limit_kib,
        time_limit_ms,
        which: Some(job::Which::RunCached(job::RunCached {
            source_path,
            arguments,
            stdin_path,
            stdout_path,
        }))
    }).await?;
    info!("Sent");

    loop {
        info!("Polling for job result");
        let job_result = job_result_receiver.recv().await?;
        if job_result.uuid == uuid.to_string() {
            if let JobResult {
                which: Some(job_result::Which::RunCached(compile)),
                ..
            } = job_result {
                return Ok(compile);
            }
            return Err(Box::new(JobFailedError));
        }
    }
}

pub async fn judge(
    job_sender: &Sender<Job>,
    job_result_sender: &broadcast::Sender<JobResult>,
    language: &String,
    source_text: String,
    test_count: i32,
    test_pattern: String,
    checker_language: String,
    checker_source_path: String,
    memory_limit_kib: i32,
    time_limit_ms: i32,
) -> Result<job_result::Judgement, Box<dyn std::error::Error>> {
    let uuid = Uuid::new_v4();
    let mut job_result_receiver = job_result_sender.subscribe();
    job_sender.send(Job {
        uuid: uuid.to_string(),
        language: language.to_string(),
        memory_limit_kib,
        time_limit_ms,
        which: Some(job::Which::Judgement(job::Judgement {
            source_text,
            test_count,
            test_pattern,
            checker_language,
            checker_source_path,
        }))
    }).await?;

    loop {
        let job_result = job_result_receiver.recv().await?;
        if job_result.uuid == uuid.to_string() {
            if let JobResult {
                which: Some(job_result::Which::Judgement(compile)),
                ..
            } = job_result {
                return Ok(compile);
            }
            return Err(Box::new(JobFailedError));
        }
    }
}
