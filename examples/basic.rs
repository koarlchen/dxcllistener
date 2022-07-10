use std::env;
use std::process;
use std::sync::Arc;

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
        let mut listener = dxcllistener::listen(host.into(), port, call.into(), handler).unwrap();

        // Join listener thread
        match listener.join() {
            Ok(_) => retval = 0,
            Err(e) => {
                eprintln!("{}", e);
                retval = 1;
            }
        }
    }

    process::exit(retval);
}
