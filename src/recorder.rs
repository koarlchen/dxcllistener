use std::fmt;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::str;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

/// Possible errors while recording
#[derive(Debug, PartialEq)]
pub enum RecordError {
    /// Unknown error
    UnknownError,

    /// Connection to server lost
    ConnectionLost,

    /// Connection error (failed to connect)
    ConnectionError,

    /// Authentication error
    AuthenticationError,

    /// Stop request
    StopRequest,
}

impl fmt::Display for RecordError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Error while recording: {:?}", self)
    }
}

/// Record data from dx cluster.
///
/// ## Arguments
///
/// * `host`: Host of server
/// * `port`: Port of server
/// * `callsign`: Callsign to use for authentication
/// * `callback`: Function callback that will be called each time a new spot is parsed successfully
///
/// ## Result
///
/// TODO
pub fn record(
    host: String,
    port: u16,
    callsign: String,
    callback: Arc<dyn Fn(dxclparser::Spot) + Send + Sync>,
    signal: mpsc::Receiver<()>,
) -> JoinHandle<Result<(), RecordError>> {
    let thdname = format!("{}@{}:{}", callsign, host, port);

    thread::Builder::new()
        .name(thdname)
        .spawn(move || {
            let constring = format!("{}:{}", host, port);

            match TcpStream::connect(&constring) {
                Ok(stream) => handle_client(stream, &callsign, callback, signal),
                Err(_) => Err(RecordError::ConnectionError),
            }
        })
        .expect("Failed to spawn thread")
}

/// Handle client connection to dx cluster server.
/// First authenticate with callsign and then start processing incoming spots.
fn handle_client(
    mut stream: TcpStream,
    callsign: &str,
    callback: Arc<dyn Fn(dxclparser::Spot)>,
    signal: mpsc::Receiver<()>,
) -> Result<(), RecordError> {
    stream
        .set_read_timeout(Some(Duration::new(0, 500_000_000)))
        .unwrap();

    handle_auth(&mut stream, callsign, &signal)?;
    process_data(&mut stream, callback, &signal)?;

    Ok(())
}

/// Continously listen on the tcp stream for new spots.
/// For each received and successfully parsed spot the callback function will be called with the spot as the argument.
/// Timeouts are used to regulary check for the stop signal.
fn process_data(
    stream: &mut TcpStream,
    callback: Arc<dyn Fn(dxclparser::Spot)>,
    signal: &mpsc::Receiver<()>,
) -> Result<(), RecordError> {
    let mut reader = BufReader::new(stream.try_clone().unwrap());

    let res;

    let mut line = String::new();
    loop {
        match reader.read_line(&mut line) {
            Ok(0) => {
                // EOF
                res = Err(RecordError::ConnectionLost);
                break;
            }
            Err(err) if err.kind() != std::io::ErrorKind::WouldBlock => {
                // Catch all errors, except for timeout
                res = Err(RecordError::UnknownError);
                break;
            }
            ret => {
                // Check for signal to stop execution
                if signal.try_recv().is_ok() {
                    res = Err(RecordError::StopRequest);
                    break;
                }

                // If no timeout occurred, parse data
                if ret.is_ok() {
                    let clean = clean_line(&line);
                    if let Ok(spot) = dxclparser::parse(clean) {
                        callback(spot);
                    }
                    line.clear();
                }
            }
        }
    }

    res
}

/// Clean line from unwanted characters.
/// Remove whitespace characters and bell characters (0x07) from the end of the string.
fn clean_line(line: &str) -> &str {
    line.trim_end().trim_end_matches('\u{0007}')
}

/// Handle authentication at remote cluster server with callsign.
/// Depending on the software running on the cluster server, the authentication token may contain a newline an the end.
/// Therefore use timeouts to check for authentication tokens that are not ending with a newline.
/// Timeouts are further used to regulary check for the stop signal.
fn handle_auth(
    stream: &mut TcpStream,
    callsign: &str,
    signal: &mpsc::Receiver<()>,
) -> Result<(), RecordError> {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let res;
    let mut timeout_counter = 10;

    let mut data = String::new();
    loop {
        match reader.read_line(&mut data) {
            Ok(0) => {
                // EOF
                res = Err(RecordError::ConnectionLost);
                break;
            }
            Err(err) if err.kind() != std::io::ErrorKind::WouldBlock => {
                // Catch all errors, except for timeout
                res = Err(RecordError::UnknownError);
                break;
            }
            ret => {
                // Check for signal to stop execution
                if signal.try_recv().is_ok() {
                    res = Err(RecordError::StopRequest);
                    break;
                }

                // New line or timed out
                if is_auth_token(&data) {
                    res = send_line(stream, callsign);
                    break;
                }

                // Check for timeout (WouldBlock)
                if ret.is_err() {
                    timeout_counter -= 1;
                    if timeout_counter == 0 {
                        res = Err(RecordError::AuthenticationError);
                        break;
                    }
                } else {
                    data.clear();
                }
            }
        }
    }

    res
}

/// Check if a given string starts with one of the authentication tokens.
fn is_auth_token(token: &str) -> bool {
    let auth_keys = ["login:", "Please enter your call:"];

    for key in auth_keys.iter() {
        if token.starts_with(key) {
            return true;
        }
    }

    false
}

/// Send a string through tcp stream.
/// Appends '\r\n' to the string before sending.
fn send_line(stream: &mut TcpStream, data: &str) -> Result<(), RecordError> {
    match stream.write(format!("{}\r\n", data).as_bytes()) {
        Ok(0) => {
            // EOF
            Err(RecordError::ConnectionLost)
        }
        Ok(_) => Ok(()),
        Err(_) => {
            // Error
            Err(RecordError::UnknownError)
        }
    }
}
