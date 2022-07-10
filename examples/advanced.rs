use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn main() {
    let host1 = "example.com";
    let port1 = 1234;

    let host2 = "example.net";
    let port2 = 5678;

    let call = "INVALID";

    // Handler for new spots
    let handler: Arc<dyn Fn(dxclparser::Spot) + Send + Sync> =
        Arc::new(|spot| println!("{}", spot.to_json()));

    let signal = Arc::new(AtomicBool::new(true));

    // Create two listener
    let mut listeners: Vec<dxcllistener::Listener> = Vec::new();
    listeners
        .push(dxcllistener::listen(host1.into(), port1, call.into(), handler.clone()).unwrap());
    listeners.push(dxcllistener::listen(host2.into(), port2, call.into(), handler).unwrap());

    // Register ctrl-c handler to stop threads
    let sig = signal.clone();
    ctrlc::set_handler(move || {
        println!("Ctrl-C caught");
        sig.store(false, Ordering::Relaxed);
    })
    .expect("Failed to listen on Ctrl-C");

    // Actively wait until both listeners finished their execution
    while listeners.len() > 0 {
        // Check for application stop request
        if !signal.load(Ordering::Relaxed) {
            // Send stop request to listeners
            for l in listeners.iter_mut() {
                l.request_stop();
            }
            // Join listeners
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

        std::thread::sleep(std::time::Duration::from_millis(250))
    }
}
