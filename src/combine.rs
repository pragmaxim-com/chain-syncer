use futures::future::join;
use std::future::Future;
use tokio::signal::unix::{signal, SignalKind};

pub async fn futures(future_a: impl Future<Output = ()>, future_b: impl Future<Output = ()>) {
    let mut sigint = signal(SignalKind::interrupt()).expect("Failed to listen for SIGINT");
    let mut sigterm = signal(SignalKind::terminate()).expect("Failed to listen for SIGTERM");

    let combined_fut = join(future_a, future_b);

    tokio::select! {
        _ = sigint.recv() => {
            println!("Received SIGINT, shutting down...");
        }
        _ = sigterm.recv() => {
            println!("Received SIGTERM, shutting down...");
        }
        _ = combined_fut => {

        }
    }
}