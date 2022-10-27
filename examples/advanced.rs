use std::time::Duration;
use tokio::signal;
use tokio::sync::mpsc;
use tokio::task;
use tokio::time;

use dxcllistener::Listener;

#[tokio::main]
async fn main() {
    // Create two listeners
    let mut listeners: Vec<dxcllistener::Listener> = vec![
        Listener::new("example.com".into(), 1234, "INVALID-1".into()),
        Listener::new("example.net".into(), 5678, "INVALID-2".into()),
    ];

    // Create channel to implement graceful shutdown
    let (shtdwn_tx, mut shtdwn_rx) = mpsc::unbounded_channel::<()>();

    // Register ctrl-c handler to stop tasks
    task::spawn(async move {
        signal::ctrl_c().await.expect("Failed to listen for ctrl-c");
        drop(shtdwn_tx);
    });

    // Communication channel between listeners and receiver
    let (spot_tx, mut spot_rx) = mpsc::unbounded_channel::<String>();

    // Handle incoming spots
    let receiver = task::spawn(async move {
        while let Some(spot) = spot_rx.recv().await {
            println!("{}", spot);
        }
    });

    // Start listening for spots
    for lis in listeners.iter_mut() {
        lis.listen(spot_tx.clone(), Duration::from_millis(1000))
            .await
            .unwrap();
    }

    // Main loop
    while !listeners.is_empty() {
        // Check for unexpectedly stopped listeners
        if let Some(pos) = listeners.iter_mut().position(|x| !x.is_running()) {
            let res = listeners[pos].join().await.unwrap_err();
            println!(
                "Listener {}@{}:{} stopped unexpectedly ({})",
                listeners[pos].callsign, listeners[pos].host, listeners[pos].port, res
            );
            listeners.remove(pos);
        }

        // Either wait a few milliseconds or receive signal to stop listeners
        tokio::select! {
            _ = time::sleep(time::Duration::from_millis(250)) => (),
            _ = shtdwn_rx.recv() => {
                for l in listeners.iter_mut() {
                    l.request_stop().unwrap();
                }
                for l in listeners.iter_mut() {
                    l.join().await.unwrap();
                }
                break;
            }
        }
    }

    // Drop last spot sender to quit receiving task
    drop(spot_tx);

    // Receiver will stop its execution after the last listener stopped
    receiver.await.unwrap();
}
