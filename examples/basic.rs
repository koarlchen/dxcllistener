use std::env;
use std::sync::mpsc;
use std::time::Duration;

use dxcllistener::Listener;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 4 {
        eprintln!("Invalid number of arguments.");
        eprintln!("Usage: {} <host> <port> <call>", args[0]);
    } else {
        let host = &args[1];
        let port = args[2].parse::<u16>().unwrap();
        let call = &args[3];

        // Create communication channel
        let (tx, rx) = mpsc::channel();

        // Create and start listener
        let mut listener = Listener::new(host.into(), port, call.into());
        listener.listen(tx, Duration::from_secs(1)).unwrap();

        // Process spots
        while let Ok(spot) = rx.recv() {
            println!("{}", spot.to_json());
        }

        // Evaluate error reason why the listener stopped unexpectedly
        eprintln!("{}", listener.join().unwrap_err());
    }
}
