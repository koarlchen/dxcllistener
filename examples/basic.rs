use std::env;
use std::sync::mpsc;

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

        // Create listener
        let mut listener = dxcllistener::listen(host.into(), port, call.into(), tx).unwrap();

        // Process spots
        while let Ok(spot) = rx.recv() {
            println!("{}", spot.to_json());
        }

        // Evaluate error reason why the listener stopped unexpectedly
        eprintln!("{}", listener.join().unwrap_err());
    }
}
