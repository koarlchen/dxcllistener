use std::env;
use std::process;
use std::sync::{mpsc, Arc};

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

        let (tx, rx) = mpsc::channel();

        ctrlc::set_handler(move || {
            println!("Ctrl-C caught");
            tx.send(()).expect("Failed to send signal on channel");
        })
        .expect("Failed to listen on Ctrl-C");

        let handler: Arc<dyn Fn(dxclparser::Spot) + Send + Sync> =
            Arc::new(|spot| println!("{}", spot.to_json()));

        match dxclrecorder::record(host.into(), port, call.into(), handler, rx)
            .join()
            .unwrap()
        {
            Ok(_) => {
                retval = 0;
            }
            Err(err) => {
                eprintln!("{}", err);
                retval = 1;
            }
        }
    }

    process::exit(retval);
}
