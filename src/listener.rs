// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::fmt;
use std::str;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::{ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::task::JoinHandle;
use tokio::time;

// Authentication tokens sent by cluster servers.
const AUTH_TOKEN: [&str; 2] = ["login:", "Please enter your call:"];

/// Possible errors while listening
#[derive(Error, Debug)]
pub enum ListenError {
    #[error("unknown error")]
    UnknownError,

    #[error("connection to server lost")]
    ConnectionLost,

    #[error("failed to connect to server")]
    ConnectionError,

    #[error("timeout while connect to server")]
    ConnectionTimeout,

    #[error("failed to authenticate at server")]
    AuthenticationError,

    #[error("internal error")]
    InternalError,

    #[error("listener was already joined")]
    AlreadyJoined,

    #[error("receiver for parsed spots lost")]
    ReceiverLost,

    #[error("shutdown was already requested")]
    ShutdownAlreadyRequested,
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

    /// Shutdown signal
    shutdown: Option<UnboundedSender<()>>,
}

impl fmt::Display for Listener {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}@{}:{}", self.callsign, self.host, self.port)
    }
}

impl Listener {
    /// Request the stop of the listener
    pub fn request_stop(&mut self) -> Result<(), ListenError> {
        match self.shutdown.take() {
            Some(sd) => sd.send(()).map_err(|_| ListenError::InternalError),
            None => Err(ListenError::ShutdownAlreadyRequested),
        }
    }

    /// Join the listener to get the result
    pub async fn join(&mut self) -> Result<(), ListenError> {
        match self.handle.take() {
            Some(h) => h.await.map_err(|_| ListenError::InternalError)?,
            None => Err(ListenError::AlreadyJoined),
        }
    }

    /// Check if the listener is running
    pub fn is_running(&self) -> bool {
        self.run.load(Ordering::Relaxed)
    }

    /// Create new instace of `Listener`.
    ///
    /// # Arguments
    ///
    /// * `host`: Host of server
    /// * `port`: Port of server
    /// * `callsign`: Callsign to use for authentication
    ///
    /// # Result
    ///
    /// Returns a new instance of a `Listener`.
    pub fn new(host: String, port: u16, callsign: String) -> Self {
        Self {
            host,
            port,
            callsign,
            run: Arc::new(AtomicBool::new(false)),
            handle: None,
            shutdown: None,
        }
    }

    /// Listen for data from dx cluster.
    ///
    /// # Arguments
    ///
    /// * `channel`: Communication channel where to send received spots to
    /// * `conn_timeout`: Connection timeout to server
    ///
    /// # Result
    ///
    /// The result shall be `Ok(())` if the listener is connected and is waiting for spots.
    /// An `Err(ListenError)` shall be returned in case something went wrong while connecting.
    pub async fn listen(
        &mut self,
        channel: mpsc::UnboundedSender<String>,
        connection_timeout: std::time::Duration,
    ) -> Result<(), ListenError> {
        self.run.store(false, Ordering::Relaxed);

        let constring = format!("{}:{}", self.host, self.port);
        let call = self.callsign.clone();
        let flag = self.run.clone();

        // Open connection to server with configured timeout
        match time::timeout(connection_timeout, TcpStream::connect(constring))
            .await
            .map_err(|_| ListenError::ConnectionTimeout)?
        {
            Ok(stream) => {
                // Create communication channel to later request the shutdown of the task
                let (shutdown_tx, shutdown_rx) = mpsc::unbounded_channel();

                // Set listener-running flag to true
                flag.store(true, Ordering::Relaxed);

                // Start listener main task
                let tsk: JoinHandle<Result<(), ListenError>> = tokio::spawn(async move {
                    // Authenticate at server and start listening for spots
                    let res = run(stream, channel, shutdown_rx, &call).await;

                    // Set listener-running flag to false
                    flag.store(false, Ordering::Relaxed);
                    res
                });

                self.shutdown = Some(shutdown_tx);
                self.handle = Some(tsk);

                Ok(())
            }
            Err(_) => Err(ListenError::ConnectionError),
        }
    }
}

/// Run the client.
/// First, authenticate at server with callsign.
/// Afterwards parse received spot and pass the parsed information into the communication channel.
async fn run(
    mut stream: TcpStream,
    pipe: mpsc::UnboundedSender<String>,
    mut shutdown: mpsc::UnboundedReceiver<()>,
    callsign: &str,
) -> Result<(), ListenError> {
    // Split stream ins reading and writing half
    let (mut rx, mut tx) = stream.split();

    // Authenticate at server
    auth(&mut rx, &mut tx, callsign).await?;

    // Read incoming lines from server
    read(&mut rx, &mut shutdown, pipe).await?;

    Ok(())
}

/// Authenticate at server
async fn auth(
    rx: &mut ReadHalf<'_>,
    tx: &mut WriteHalf<'_>,
    callsign: &str,
) -> Result<(), ListenError> {
    // Configuration
    let mut retries = 5;

    // Create reader
    let mut reader = BufReader::new(rx);

    // Buffer
    let mut buf = vec![];

    loop {
        // Read data with timeout
        let res = time::timeout(
            time::Duration::from_millis(500),
            reader.read_until(b':', &mut buf),
        )
        .await;

        // Check for errors of read function
        if let Ok(inner) = res {
            check_read_result(&inner)?;
        }

        // Process read data
        if let Ok(line) = str::from_utf8(&buf) {
            // Check if the read string ends with the auth token
            if is_auth_token(line) {
                // Send callsign to server for authentication
                send_line(tx, callsign).await?;
                break;
            }
        } else {
            // Take care of endless loop
            retries -= 1;
            if retries == 0 {
                Err(ListenError::AuthenticationError)?;
            }
        }
    }

    Ok(())
}

/// Read and forward incoming lines
async fn read(
    rx: &mut ReadHalf<'_>,
    shutdown: &mut mpsc::UnboundedReceiver<()>,
    pipe: mpsc::UnboundedSender<String>,
) -> Result<(), ListenError> {
    // Create reader
    let mut reader = BufReader::new(rx);

    // Line buffer
    let mut line = String::with_capacity(100);

    loop {
        // Read line or wait for shutdown signal
        tokio::select! {
            res = reader.read_line(&mut line) => {
                check_read_result(&res)?;
            },
            res = shutdown.recv() => {
                if res.is_none() {
                    Err(ListenError::InternalError)?;
                }
                break;
            },
        }

        // Remove unwanted characters from received line
        let clean = clean_line(&line);

        // Push received line into channel
        pipe.send(clean.into())
            .map_err(|_| ListenError::ReceiverLost)?;

        // Clear buffer
        line.clear();
    }

    Ok(())
}

/// Check result from read function against possible errors.
fn check_read_result(res: &tokio::io::Result<usize>) -> Result<usize, ListenError> {
    match res {
        Ok(0) => Err(ListenError::ConnectionLost),
        Err(_) => Err(ListenError::InternalError),
        Ok(num) => Ok(*num),
    }
}

/// Clean line from unwanted characters.
/// Remove whitespace characters and bell characters (0x07) from the end of the string.
fn clean_line(line: &str) -> &str {
    line.trim_end().trim_end_matches('\u{0007}')
}

/// Check if a given string ends with one of the authentication tokens.
fn is_auth_token(token: &str) -> bool {
    for key in AUTH_TOKEN.iter() {
        if token.ends_with(key) {
            return true;
        }
    }

    false
}

/// Send a string through a tcp stream.
/// Appends '\r\n' to the given string before sending it.
async fn send_line(stream: &mut WriteHalf<'_>, data: &str) -> Result<(), ListenError> {
    stream
        .write_all(format!("{}\r\n", data).as_bytes())
        .await
        .map_err(|_| ListenError::UnknownError)
}
