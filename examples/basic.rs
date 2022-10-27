use std::env;
use std::time::Duration;
use tokio::sync::mpsc;

use dxcllistener::Listener;

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 4 {
        eprintln!("Invalid number of arguments.");
        eprintln!("Usage: {} <host> <port> <call>", args[0]);
    } else {
        let host = &args[1];
        let port = args[2].parse::<u16>().unwrap();
        let call = &args[3];

        // Create communication channel
        let (tx, mut rx) = mpsc::unbounded_channel();

        // Create and start listener
        let mut listener = Listener::new(host.into(), port, call.into());
        listener
            .listen(tx, Duration::from_millis(1000))
            .await
            .unwrap();

        // Process spots
        while let Some(spot) = rx.recv().await {
            println!("{}", spot);
        }

        // Evaluate error reason why the listener stopped unexpectedly
        eprintln!("{}", listener.join().await.unwrap_err());
    }
}
