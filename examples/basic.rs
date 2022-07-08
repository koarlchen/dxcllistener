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

        // Handler for new spots
        let handler: Arc<dyn Fn(dxclparser::Spot) + Send + Sync> =
            Arc::new(|spot| println!("{}", spot.to_json()));

        // Create listener
        let rec = dxcllistener::listen(host.into(), port, call.into(), handler).unwrap();
        let listener: Arc<Mutex<dxcllistener::Listener>> = Arc::new(Mutex::new(rec));

        // Register ctrl-c handler to stop threads
        let rec = listener.clone();
        ctrlc::set_handler(move || {
            println!("Ctrl-C caught");
            rec.lock().unwrap().request_stop();
        })
        .expect("Failed to listen on Ctrl-C");

        // Actively wait until both listeners finished their execution
        loop {
            {
                let mut r = listener.lock().unwrap();
                if !r.is_running() {
                    r.join();
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(250))
        }

        // Evaluate returned result of listener
        match listener.lock().unwrap().result.as_ref().unwrap() {
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
