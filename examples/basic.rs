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
        match dxclrecorder::record(&args[1], args[2].parse::<u16>().unwrap(), &args[3]) {
            Ok(_) => retval = 0,
            Err(_) => retval = 1,
        }
    }

    process::exit(retval);
}
