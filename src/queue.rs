use job_protocol::{GetJobRequest, JobResult, JobResultConfirmation, Job, Language};
use job_protocol::job_queue_server::JobQueue;
use tonic::{Request, Response, Status};
use async_channel::Receiver;
use tokio::sync::broadcast;
use dashmap::DashMap;
use std::sync::Arc;
use log::info;

pub mod job_protocol {
    tonic::include_proto!("job_protocol");
}

#[derive(Debug)]
pub struct JobQueuer {
    pub job_receiver: Receiver<Job>,
    pub job_result_sender: broadcast::Sender<JobResult>,
    pub languages: Arc<DashMap<String, Language>>,
}

#[tonic::async_trait]
impl JobQueue for JobQueuer {
    async fn get_job(
        &self,
        request: Request<GetJobRequest>,
    ) -> Result<Response<Job>, Status> {
        info!("Got GetJob");
        let request = request.into_inner();
        for language in request.supported_languages {
            self.languages.insert(language.key.clone(), language);
        }

        info!("Waiting for job to send");
        let job = self.job_receiver.recv().await.expect("Failed to receive from job queue");
        Ok(Response::new(job))
    }

    async fn submit_job_result(
        &self,
        request: Request<JobResult>,
    ) -> Result<Response<JobResultConfirmation>, Status> {
        let request = request.into_inner();
        println!("{:?}", request);
        self.job_result_sender.send(request).expect("Failed to send to job result broadcast");
        Ok(Response::new(JobResultConfirmation {}))
    }
}
