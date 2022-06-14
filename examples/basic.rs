use std::env;
use std::process;

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

        let handler: Box<dyn Fn(dxclparser::Spot)> =
            Box::new(|spot| println!("{}", spot.to_json()));

        match dxclrecorder::record(host, port, call, handler) {
            Ok(_) => retval = 0,
            Err(err) => {
                eprintln!("{}", err);
                retval = 1;
            }
        }
    }

    process::exit(retval);
}
