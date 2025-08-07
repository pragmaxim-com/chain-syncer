use std::future::Future;
use futures::future::join;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::watch;

pub async fn futures<A, B, OA, OB>(future_a: A, future_b: B, shutdown_tx: watch::Sender<bool>)
where
    A: Future<Output = OA> + Send + 'static,
    B: Future<Output = OB> + Send + 'static,
{
    let mut sigint = signal(SignalKind::interrupt()).expect("Failed to listen for SIGINT");
    let mut sigterm = signal(SignalKind::terminate()).expect("Failed to listen for SIGTERM");

    tokio::select! {
        _ = sigint.recv() => {
            println!("Received SIGINT, shutting down...");
            let _ = shutdown_tx.send(true);
        }
        _ = sigterm.recv() => {
            println!("Received SIGTERM, shutting down...");
            let _ = shutdown_tx.send(true);
        }
        _ = join(future_a, future_b) => {}
    }
}
