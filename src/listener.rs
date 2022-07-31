// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::fmt;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
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

    /// Thread was already joined
    AlreadyJoined,

    /// Receiver of parsed spots lost
    ReceiverLost,
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

    /// True if the listener shall run, false if the listener shall stop its execution.
    /// May already be false if an error occurred while listening.
    run: Arc<AtomicBool>,

    /// Handle to the listener thread
    handle: Option<JoinHandle<Result<(), ListenError>>>,
}

impl Listener {
    /// Request the stop of the listener.
    /// The timespan between the request and the actual stop of the trigger may take up to 250 ms.
    pub fn request_stop(&self) {
        self.run.store(false, Ordering::Relaxed);
    }

    /// Join the listener to get the result
    pub fn join(&mut self) -> Result<(), ListenError> {
        match self.handle.take() {
            Some(h) => h.join().unwrap(),
            None => Err(ListenError::AlreadyJoined),
        }
    }

    /// Check if the listener is running
    pub fn is_running(&self) -> bool {
        self.run.load(Ordering::Relaxed)
    }

    /// Create new instace of Listener
    ///
    /// ## Arguments
    ///
    /// * `host`: Host of server
    /// * `port`: Port of server
    /// * `callsign`: Callsign to use for authentication
    ///
    /// ## Result
    ///
    /// New instance of a Listener.
    pub fn new(host: String, port: u16, callsign: String) -> Self {
        Self {
            host,
            port,
            callsign,
            run: Arc::new(AtomicBool::new(true)),
            handle: None,
        }
    }

    /// Listen for data from dx cluster.
    ///
    /// ## Arguments
    ///
    /// * `channel`: Communication channel where to send received spots to
    ///
    /// ## Result
    ///
    /// The result shall be `Ok(())` if the listener has been started.
    /// An `Err(ListenError)` shall be returned in case something went wrong while initialization.
    pub fn listen(&mut self, channel: mpsc::Sender<dxclparser::Spot>) -> Result<(), ListenError> {
        self.run.store(true, Ordering::Relaxed);

        let thdname = format!("{}@{}:{}", self.callsign, self.host, self.port);
        let constring = format!("{}:{}", self.host, self.port);

        let call = self.callsign.clone();
        let flag = self.run.clone();
        let thd = thread::Builder::new()
            .name(thdname)
            .spawn(move || {
                let res = match TcpStream::connect(&constring) {
                    Ok(stream) => run(stream, channel, flag.clone(), &call),
                    Err(_) => Err(ListenError::ConnectionError),
                };
                flag.store(false, Ordering::Relaxed);
                res
            })
            .map_err(|_| ListenError::InternalError)?;

        self.handle = Some(thd);

        Ok(())
    }
}

/// Run the client.
/// First, authenticate at server with callsign.
/// Afterwards parse received spot and pass the parsed information to the callback function.
fn run(
    mut stream: TcpStream,
    callback: mpsc::Sender<dxclparser::Spot>,
    channel: Arc<AtomicBool>,
    callsign: &str,
) -> Result<(), ListenError> {
    // Enable timeout of tcp stream
    stream
        .set_read_timeout(Some(Duration::new(0, 250_000_000)))
        .map_err(|_| ListenError::InternalError)?;

    // Create reader
    let mut reader = BufReader::new(stream.try_clone().map_err(|_| ListenError::InternalError)?);

    // Returned result
    let res: Result<(), ListenError>;

    // Number of timeouts until return with error
    let mut timeout_counter = 20;

    // Current state of connection
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
                if !channel.load(Ordering::Relaxed) {
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
                                callback.send(spot).map_err(|_| ListenError::ReceiverLost)?;
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
