use std::env;
use std::process;
use std::sync::{Arc, Mutex};

fn main() {
    let args: Vec<String> = env::args().collect();
    let retval;

    if args.len() != 4 {
        eprintln!("Invalid number of arguments.");
        eprintln!("Usage: {} <host> <port> <call>", args[0]);
        retval = 1;
    } else {
        let host = &args[1];
        let port = args[2].parse::<u16>().unwrap();
        let call = &args[3];

        let handler: Arc<dyn Fn(dxclparser::Spot) + Send + Sync> =
            Arc::new(|spot| println!("{}", spot.to_json()));

        let rec = dxclrecorder::record(host.into(), port, call.into(), handler).unwrap();
        let recorder: Arc<Mutex<dxclrecorder::Recorder>> = Arc::new(Mutex::new(rec));

        let rec = recorder.clone();
        ctrlc::set_handler(move || {
            println!("Ctrl-C caught");
            rec.lock().unwrap().request_stop();
        })
        .expect("Failed to listen on Ctrl-C");

        loop {
            {
                let mut r = recorder.lock().unwrap();
                if !r.is_running() {
                    r.join();
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(250))
        }

        match recorder.lock().unwrap().result.as_ref().unwrap() {
            Ok(_) => {
                retval = 0;
            }
            Err(err) => {
                eprintln!("{}", err);
                retval = 1;
            }
        };
    }

    process::exit(retval);
}
