use std::fmt;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::str;
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
    host: &str,
    port: u16,
    callsign: &str,
    callback: Box<dyn Fn(dxclparser::Spot)>,
) -> Result<(), RecordError> {
    let constring = format!("{}:{}", host, port);

    match TcpStream::connect(&constring) {
        Ok(stream) => handle_client(stream, callsign, callback),
        Err(_) => Err(RecordError::ConnectionError),
    }
}

/// Handle client connection to dx cluster server.
/// First authenticate with callsign and then start processing incoming spots.
fn handle_client(
    mut stream: TcpStream,
    callsign: &str,
    callback: Box<dyn Fn(dxclparser::Spot)>,
) -> Result<(), RecordError> {
    handle_auth(&mut stream, callsign)?;
    process_data(&mut stream, callback)?;

    Ok(())
}

/// Process data received from cluster server.
fn process_data(
    stream: &mut TcpStream,
    callback: Box<dyn Fn(dxclparser::Spot)>,
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
            Ok(_) => {
                let clean = clean_line(&line);
                if let Ok(spot) = dxclparser::parse(clean) {
                    callback(spot);
                }
            }
            Err(_) => {
                // Error
                res = Err(RecordError::UnknownError);
                break;
            }
        }
        line.clear();
    }

    res
}

/// Clean line from unwanted characters.
/// Remove whitespace characters from the end and remove bell character (0x07)
fn clean_line(line: &str) -> &str {
    line.trim_end().trim_end_matches('\u{0007}')
}

/// Handle authentication at remote cluster server with callsign.
fn handle_auth(stream: &mut TcpStream, callsign: &str) -> Result<(), RecordError> {
    stream
        .set_read_timeout(Some(Duration::new(0, 500_000_000)))
        .unwrap();

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
                // Unknown error
                res = Err(RecordError::UnknownError);
                break;
            }
            ret => {
                // New line or timed out
                if is_auth_token(&data) {
                    res = send_line(stream, callsign);
                    break;
                }

                if ret.is_err() {
                    timeout_counter -= 1;
                    if timeout_counter == 0 {
                        res = Err(RecordError::AuthenticationError);
                        break;
                    }
                }
            }
        }
        data.clear();
    }

    stream.set_read_timeout(None).unwrap();

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
