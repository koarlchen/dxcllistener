use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;

fn main() {
    let host1 = "example.com";
    let port1 = 1234;

    let host2 = "example.net";
    let port2 = 5678;

    let call = "INVALID";

    // Communication channel between listeners and receiver
    let (tx, rx) = mpsc::channel();

    // Stop signal
    let signal = Arc::new(AtomicBool::new(true));

    // Create two listener
    let mut listeners: Vec<dxcllistener::Listener> = Vec::new();
    listeners.push(dxcllistener::listen(host1.into(), port1, call.into(), tx.clone()).unwrap());
    listeners.push(dxcllistener::listen(host2.into(), port2, call.into(), tx).unwrap());

    // Handle incoming spots
    thread::spawn(move || {
        while let Ok(spot) = rx.recv() {
            println!("{}", spot.to_json());
        }
    });

    // Register ctrl-c handler to stop threads
    let sig = signal.clone();
    ctrlc::set_handler(move || {
        println!("Ctrl-C caught");
        sig.store(false, Ordering::Relaxed);
    })
    .expect("Failed to listen on Ctrl-C");

    while listeners.len() > 0 {
        // Check for application stop request
        if !signal.load(Ordering::Relaxed) {
            for l in listeners.iter_mut() {
                l.request_stop();
            }
            for l in listeners.iter_mut() {
                l.join().unwrap();
            }
            break;
        }

        // Check for unexpectedly stopped listeners
        listeners.retain_mut(|l| {
            if !l.is_running() {
                let res = l.join().unwrap_err();
                println!(
                    "Listener {}@{}:{} stopped unexpectedly ({})",
                    l.callsign, l.host, l.port, res
                );
                false
            } else {
                true
            }
        });

        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}
