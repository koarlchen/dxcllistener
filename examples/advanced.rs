use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use dxcllistener::Listener;

fn main() {
    // Create two listeners
    let mut listeners: Vec<dxcllistener::Listener> = vec![
        Listener::new("example.com".into(), 1234, "INVALID-1".into()),
        Listener::new("example.net".into(), 5678, "INVALID-2".into()),
    ];

    // Communication channel between listeners and receiver
    let (tx, rx) = mpsc::channel();

    // Start listening for spots
    for lis in listeners.iter_mut() {
        lis.listen(tx.clone(), Duration::from_secs(1)).unwrap();
    }

    // Stop signal
    let signal = Arc::new(AtomicBool::new(true));

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

    while !listeners.is_empty() {
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
