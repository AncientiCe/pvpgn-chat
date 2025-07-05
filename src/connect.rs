use std::net::{TcpStream};
use std::io::{Read, Write};
use std::time::{Duration, Instant};
use tracing::{info, debug};

#[derive(Debug)]
pub struct Connect {
    stream: TcpStream,
}

impl Connect {
    pub(crate) fn new(stream: TcpStream) -> Connect {
        stream.set_nodelay(true).unwrap();
        Connect {stream}
    }

    fn waitfor(&mut self, wait_s: &str) -> Result<Vec<String>, String> {
        let mut buffer = [0; 1024];
        let mut collected = String::new();
        let mut extra_messages = Vec::new();
        let timeout = Duration::from_secs(10); // 10 second timeout
        let start_time = Instant::now();

        // Set stream to non-blocking mode
        self.stream.set_nonblocking(true).map_err(|e| format!("Failed to set non-blocking: {}", e))?;

        while !collected.contains(wait_s) {
            // Check for timeout
            if start_time.elapsed() > timeout {
                self.stream.set_nonblocking(false).ok(); // Reset to blocking
                return Err(format!("Timeout waiting for '{}'", wait_s));
            }

            match self.stream.read(&mut buffer) {
                Ok(0) => {
                    self.stream.set_nonblocking(false).ok(); // Reset to blocking
                    return Err("Connection closed by server".to_string());
                }
                Ok(n) => {
                    match std::str::from_utf8(&buffer[..n]) {
                        Ok(part) => {
                            collected.push_str(part);
                            debug!("Received: {}", part.trim()); // Debug output
                        }
                        Err(_) => {
                            self.stream.set_nonblocking(false).ok(); // Reset to blocking
                            return Err("Invalid UTF-8 received from server".to_string());
                        }
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // No data available, sleep briefly and continue
                    std::thread::sleep(Duration::from_millis(50));
                    continue;
                }
                Err(e) => {
                    self.stream.set_nonblocking(false).ok(); // Reset to blocking
                    return Err(format!("Read error: {}", e));
                }
            }
        }

        // Parse the collected data and extract any extra messages beyond what we were waiting for
        let lines: Vec<&str> = collected.split("\r\n").collect();
        let mut found_target = false;

        for line in lines {
            if line.is_empty() {
                continue;
            }

            if line.contains(wait_s) {
                found_target = true;
                continue;
            }

            // If we found our target and there are additional messages, save them
            if found_target {
                extra_messages.push(line.to_string());
            }
        }

        // Reset to blocking mode
        self.stream.set_nonblocking(false).map_err(|e| format!("Failed to reset blocking: {}", e))?;
        Ok(extra_messages)
    }

    fn wait_for_login_response(&mut self, username: &str) -> Result<Vec<String>, String> {
        let mut buffer = [0; 1024];
        let mut collected = String::new();
        let mut extra_messages = Vec::new();
        let timeout = Duration::from_secs(10); // 10 second timeout
        let start_time = Instant::now();
        let expected_success = format!("2010 NAME {}", username);

        // Set stream to non-blocking mode
        self.stream.set_nonblocking(true).map_err(|e| format!("Failed to set non-blocking: {}", e))?;

        while !collected.contains(&expected_success) && !collected.contains("Login failed.") {
            // Check for timeout
            if start_time.elapsed() > timeout {
                self.stream.set_nonblocking(false).ok(); // Reset to blocking
                return Err("Timeout waiting for login response".to_string());
            }

            match self.stream.read(&mut buffer) {
                Ok(0) => {
                    self.stream.set_nonblocking(false).ok(); // Reset to blocking
                    return Err("Connection closed by server".to_string());
                }
                Ok(n) => {
                    match std::str::from_utf8(&buffer[..n]) {
                        Ok(part) => {
                            collected.push_str(part);
                            debug!("Received: {}", part.trim()); // Debug output
                        }
                        Err(_) => {
                            self.stream.set_nonblocking(false).ok(); // Reset to blocking
                            return Err("Invalid UTF-8 received from server".to_string());
                        }
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // No data available, sleep briefly and continue
                    std::thread::sleep(Duration::from_millis(50));
                    continue;
                }
                Err(e) => {
                    self.stream.set_nonblocking(false).ok(); // Reset to blocking
                    return Err(format!("Read error: {}", e));
                }
            }
        }

        // Check if login failed
        if collected.contains("Login failed.") {
            self.stream.set_nonblocking(false).ok(); // Reset to blocking
            return Err("Login failed.".to_string());
        }

        // Parse the collected data and extract any extra messages beyond the login success
        let lines: Vec<&str> = collected.split("\r\n").collect();
        let mut found_login_success = false;

        for line in lines {
            if line.is_empty() {
                continue;
            }

            if line.contains(&expected_success) {
                found_login_success = true;
                continue;
            }

            // If we found our login success and there are additional messages, save them
            if found_login_success {
                extra_messages.push(line.to_string());
            }
        }

        // Reset to blocking mode
        self.stream.set_nonblocking(false).map_err(|e| format!("Failed to reset blocking: {}", e))?;
        Ok(extra_messages)
    }

    pub(crate) fn connect(&mut self, username: &str, password: &str) -> Result<Vec<String>, String> {
        if username.is_empty() || password.is_empty() {
            return Err("Username and password cannot be empty".to_string());
        }

        let mut all_extra_messages = Vec::new();

        // Send initial connection byte
        self.stream.write_all(&[3]).map_err(|e| format!("Failed to send initial byte: {}", e))?;
        self.stream.flush().map_err(|e| format!("Failed to flush stream: {}", e))?;

        info!("Waiting for username prompt...");
        let mut extra = self.waitfor("Username:")?;
        all_extra_messages.append(&mut extra);

        info!("Sending username: {}", username);
        self.stream.write_all(format!("{}\r\n", username).as_bytes())
            .map_err(|e| format!("Failed to send username: {}", e))?;
        self.stream.flush().map_err(|e| format!("Failed to flush stream: {}", e))?;

        info!("Waiting for password prompt...");
        let mut extra = self.waitfor("Password:")?;
        all_extra_messages.append(&mut extra);

        info!("Sending password...");
        self.stream.write_all(format!("{}\r\n", password).as_bytes())
            .map_err(|e| format!("Failed to send password: {}", e))?;
        self.stream.flush().map_err(|e| format!("Failed to flush stream: {}", e))?;

        info!("Waiting for login response...");
        // Wait for either login success (2010 NAME) or login failure
        let login_response = self.wait_for_login_response(username)?;
        all_extra_messages.extend(login_response);

        info!("Joining channel...");
        self.stream.write_all(format!("{}\r\n", "/join w3").as_bytes())
            .map_err(|e| format!("Failed to send join command: {}", e))?;
        self.stream.flush().map_err(|e| format!("Failed to flush stream: {}", e))?;

        info!("Waiting for join confirmation...");
        let mut extra = self.waitfor("1007")?; // Wait for CHANNEL message confirmation
        all_extra_messages.append(&mut extra);

        info!("Connection established successfully!");
        info!("Collected {} extra messages during connection", all_extra_messages.len());
        Ok(all_extra_messages)
    }

    pub fn send(&mut self, msg: String) {
        // println!("Sending: {}", msg);
        self.stream.write_all(format!("{}\r\n", msg).as_bytes()).unwrap();
        self.stream.flush().unwrap();
    }

    pub fn get_stream_clone(&self) -> Result<TcpStream, std::io::Error> {
        self.stream.try_clone()
    }

    pub fn close(&mut self) -> Result<(), std::io::Error> {
        info!("Closing connection...");
        self.stream.shutdown(std::net::Shutdown::Both)?;
        info!("Connection closed successfully");
        Ok(())
    }
}
