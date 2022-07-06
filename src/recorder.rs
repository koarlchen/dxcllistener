use std::fmt;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::str;
use std::sync::atomic::{AtomicBool, Ordering};
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

    /// Internal error
    InternalError,
}

impl fmt::Display for RecordError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Error while recording: {:?}", self)
    }
}

/// State of communication
#[derive(PartialEq)]
enum State {
    /// Authenticate at server
    Auth,

    /// Receive and parse spots from server
    Parse,
}

pub struct Recorder {
    /// Host of the cluster server
    pub host: String,

    /// Port of the cluster server
    pub port: u16,

    /// Callsign to use for authentication
    pub callsign: String,

    /// True if the recorder shall run, false if the recorder shall stop its execution
    run: Arc<AtomicBool>,

    /// Handle to the recorder thread
    handle: Option<JoinHandle<Result<(), RecordError>>>,
}

impl Recorder {
    /// Request the stop of the recorder.
    pub fn request_stop(&self) {
        self.run.store(false, std::sync::atomic::Ordering::Relaxed);
    }

    /// Join the recorder to get the result.
    pub fn join(&mut self) -> Result<(), RecordError> {
        self.handle.take().unwrap().join().unwrap()
    }

    /// Check if the recorder is running.
    pub fn is_running(&mut self) -> bool {
        self.run.load(Ordering::Relaxed)
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
/// * `signal`: Signal to request execution stop of the thread
///
/// ## Result
///
/// The result shall be `Ok(Recorder)` if the recorder has been started.
/// An `Err(RecordError)` shall be returned in case something went wrong while initialization.
pub fn record(
    host: String,
    port: u16,
    callsign: String,
    callback: Arc<dyn Fn(dxclparser::Spot) + Send + Sync>,
) -> Result<Recorder, RecordError> {
    let exec = Arc::new(AtomicBool::new(true));

    let thdname = format!("{}@{}:{}", callsign, host, port);
    let constring = format!("{}:{}", host, port);

    let call = callsign.clone();
    let flag = exec.clone();
    let thd = thread::Builder::new()
        .name(thdname)
        .spawn(move || match TcpStream::connect(&constring) {
            Ok(stream) => run(stream, callback, flag, &call),
            Err(_) => Err(RecordError::ConnectionError),
        })
        .map_err(|_| RecordError::InternalError)?;

    Ok(Recorder {
        host,
        port,
        callsign,
        run: exec,
        handle: Some(thd),
    })
}

/// Run the client.
/// First, authenticate at server with callsign.
/// Afterwards parse received spot and pass the parsed information to the callback function.
fn run(
    mut stream: TcpStream,
    callback: Arc<dyn Fn(dxclparser::Spot)>,
    signal: Arc<AtomicBool>,
    callsign: &str,
) -> Result<(), RecordError> {
    // Enable timeout of tcp stream
    stream
        .set_read_timeout(Some(Duration::new(0, 250_000_000)))
        .map_err(|_| RecordError::InternalError)?;

    let mut reader = BufReader::new(stream.try_clone().map_err(|_| RecordError::InternalError)?);

    let res: Result<(), RecordError>;
    let mut timeout_counter = 10;
    let mut state = State::Auth;

    // Line buffer
    let mut line = String::new();

    // Communication loop
    loop {
        // Read line, may timout after configured duration
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
                // Check for signal to stop thread
                if !signal.load(Ordering::Relaxed) {
                    res = Ok(());
                    stream
                        .shutdown(std::net::Shutdown::Both)
                        .map_err(|_| RecordError::InternalError)?;
                    break;
                }

                match state {
                    // Authenticate at server
                    State::Auth => {
                        // New line or timed out, check for authentication string
                        if is_auth_token(&line) {
                            // Send callsign
                            if let Err(err) = send_line(&mut stream, callsign) {
                                res = Err(err);
                                break;
                            }
                            // Swith state to parse incoming spots
                            state = State::Parse;
                        } else {
                            // Check for timeout while authentication
                            if ret.is_err() {
                                timeout_counter -= 1;
                                // Prevent endless loop, cancel authentication after a few timeouts
                                if timeout_counter == 0 {
                                    res = Err(RecordError::AuthenticationError);
                                    break;
                                }
                            } else {
                                // Clear buffer in case no timeout occurred which means a complete line ending with newline was received
                                line.clear();
                            }
                        }
                    }
                    // Parse incoming spots
                    State::Parse => {
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
        }
    }

    res
}

/// Clean line from unwanted characters.
/// Remove whitespace characters and bell characters (0x07) from the end of the string.
fn clean_line(line: &str) -> &str {
    line.trim_end().trim_end_matches('\u{0007}')
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
