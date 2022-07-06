use std::sync::{Arc, Mutex};

fn main() {
    let host1 = "example.com";
    let port1 = 1234;

    let host2 = "example.net";
    let port2 = 5678;

    let call = "INVALID";

    // Handler for new spots
    let handler: Arc<dyn Fn(dxclparser::Spot) + Send + Sync> =
        Arc::new(|spot| println!("{}", spot.to_json()));

    // Create two recorders
    let recorder: Arc<Mutex<Vec<dxclrecorder::Recorder>>> = Arc::new(Mutex::new(Vec::new()));
    recorder
        .lock()
        .unwrap()
        .push(dxclrecorder::record(host1.into(), port1, call.into(), handler.clone()).unwrap());
    recorder
        .lock()
        .unwrap()
        .push(dxclrecorder::record(host2.into(), port2, call.into(), handler).unwrap());

    // Register ctrl-c handler to stop threads
    let recs = recorder.clone();
    ctrlc::set_handler(move || {
        println!("Ctrl-C caught");
        for r in recs.lock().unwrap().iter() {
            r.request_stop()
        }
    })
    .expect("Failed to listen on Ctrl-C");

    loop {
        let mut run = false;
        for rec in recorder.lock().unwrap().iter_mut() {
            if !rec.is_running() {
                rec.join().unwrap();
            } else {
                run = true;
            }
        }
        if !run {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250))
    }
}
