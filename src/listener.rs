use std::fmt;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::str;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

/// Possible errors while listening
#[derive(Debug, PartialEq)]
pub enum ListenError {
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

impl fmt::Display for ListenError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Error while listening: {:?}", self)
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

pub struct Listener {
    /// Host of the cluster server
    pub host: String,

    /// Port of the cluster server
    pub port: u16,

    /// Callsign to use for authentication
    pub callsign: String,

    /// Result of finished thread
    pub result: Option<Result<(), ListenError>>,

    /// True if the listener shall run, false if the listener shall stop its execution.
    /// May already be false if an error occurred while listening.
    run: Arc<AtomicBool>,

    /// Handle to the listener thread
    handle: Option<JoinHandle<Result<(), ListenError>>>,
}

impl Listener {
    /// Request the stop of the listener
    pub fn request_stop(&self) {
        self.run.store(false, Ordering::Relaxed);
    }

    /// Join the listener to get the result
    pub fn join(&mut self) {
        self.result = Some(self.handle.take().unwrap().join().unwrap());
    }

    /// Check if the listener is running
    pub fn is_running(&self) -> bool {
        self.run.load(Ordering::Relaxed)
    }
}

/// Listen for data from dx cluster.
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
/// The result shall be `Ok(Listener)` if the listener has been started.
/// An `Err(ListenError)` shall be returned in case something went wrong while initialization.
pub fn listen(
    host: String,
    port: u16,
    callsign: String,
    callback: Arc<dyn Fn(dxclparser::Spot) + Send + Sync>,
) -> Result<Listener, ListenError> {
    let exec = Arc::new(AtomicBool::new(true));

    let thdname = format!("{}@{}:{}", callsign, host, port);
    let constring = format!("{}:{}", host, port);

    let call = callsign.clone();
    let flag = exec.clone();
    let thd = thread::Builder::new()
        .name(thdname)
        .spawn(move || match TcpStream::connect(&constring) {
            Ok(stream) => run(stream, callback, flag, &call),
            Err(_) => {
                flag.store(false, Ordering::Relaxed);
                Err(ListenError::ConnectionError)
            }
        })
        .map_err(|_| ListenError::InternalError)?;

    Ok(Listener {
        host,
        port,
        callsign,
        result: None,
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
) -> Result<(), ListenError> {
    // Enable timeout of tcp stream
    stream
        .set_read_timeout(Some(Duration::new(0, 250_000_000)))
        .map_err(|_| ListenError::InternalError)?;

    let mut reader = BufReader::new(stream.try_clone().map_err(|_| ListenError::InternalError)?);

    let res: Result<(), ListenError>;
    let mut timeout_counter = 20;
    let mut state = State::Auth;

    // Line buffer
    let mut line = String::new();

    // Communication loop
    loop {
        // Read line, may timout after configured duration
        match reader.read_line(&mut line) {
            Ok(0) => {
                // EOF
                res = Err(ListenError::ConnectionLost);
                break;
            }
            Err(err) if err.kind() != std::io::ErrorKind::WouldBlock => {
                // Catch all errors, except for timeout
                res = Err(ListenError::UnknownError);
                break;
            }
            ret => {
                // Check for signal to stop thread
                if !signal.load(Ordering::Relaxed) {
                    res = Ok(());
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
                            // Switch state to parse incoming spots
                            state = State::Parse;
                        } else {
                            // Check for timeout while authentication
                            if ret.is_err() {
                                timeout_counter -= 1;
                                // Prevent endless loop, cancel authentication after a few timeouts
                                if timeout_counter == 0 {
                                    res = Err(ListenError::AuthenticationError);
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

    if res.is_err() {
        signal.store(false, Ordering::Relaxed);
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
fn send_line(stream: &mut TcpStream, data: &str) -> Result<(), ListenError> {
    match stream.write(format!("{}\r\n", data).as_bytes()) {
        Ok(0) => {
            // EOF
            Err(ListenError::ConnectionLost)
        }
        Ok(_) => Ok(()),
        Err(_) => {
            // Error
            Err(ListenError::UnknownError)
        }
    }
}
