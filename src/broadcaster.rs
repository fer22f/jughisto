use actix_web::rt::time::{interval_at, Instant};
use actix_web::web::{Bytes, Data};
use actix_web::Error;
use futures::Stream;
use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::broadcast;
use crate::queue::job_protocol::{JobResult, job_result};

pub struct Broadcaster {
    clients: Vec<Sender<Bytes>>,
}

impl Broadcaster {
    pub fn create(job_result_receiver: broadcast::Receiver<JobResult>) -> Data<Mutex<Self>> {
        let me = Data::new(Mutex::new(Broadcaster::new()));
        Broadcaster::spawn_ping(me.clone());
        Broadcaster::spawn_receiver(me.clone(), job_result_receiver);
        me
    }

    pub fn new() -> Self {
        Broadcaster {
            clients: Vec::new(),
        }
    }

    fn spawn_receiver(me: Data<Mutex<Self>>, mut job_result_receiver: broadcast::Receiver<JobResult>) {
        actix_web::rt::spawn(async move {
            loop {
                let job_result = job_result_receiver.recv().await.unwrap();
                if let JobResult {
                    which: Some(job_result::Which::Judgement(_judgement)),
                    ..
                } = job_result {
                    me.lock().unwrap().send("update_submission", "");
                }
            }
        });
    }

    fn spawn_ping(me: Data<Mutex<Self>>) {
        actix_web::rt::spawn(async move {
            let mut task = interval_at(Instant::now(), Duration::from_secs(3));
            loop {
                task.tick().await;
                me.lock().unwrap().remove_stale_clients();
            }
        });
    }

    fn remove_stale_clients(&mut self) {
        let mut ok_clients = Vec::new();
        for client in self.clients.iter() {
            let result = client.clone().try_send(Bytes::from("data: ping\n\n"));

            if let Ok(()) = result {
                ok_clients.push(client.clone());
            }
        }
        self.clients = ok_clients;
    }

    pub fn new_client(&mut self) -> Client {
        let (tx, rx) = channel(100);

        tx.clone()
            .try_send(Bytes::from("data: connected\n\n"))
            .unwrap();

        self.clients.push(tx);
        Client(rx)
    }

    pub fn send(&self, event: &str, msg: &str) {
        let msg = Bytes::from(["event:", event, "\n", "data: ", msg, "\n\n"].concat());

        for client in self.clients.iter() {
            client.clone().try_send(msg.clone()).unwrap_or(());
        }
    }
}

// wrap Receiver in own type, with correct error type
pub struct Client(Receiver<Bytes>);

impl Stream for Client {
    type Item = Result<Bytes, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.0.poll_recv(cx) {
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Ready(Some(v)) => Poll::Ready(Some(Ok(v))),
            Poll::Pending => Poll::Pending,
        }
    }
}
