use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

use crate::error::TerrariumError;

struct TerminalSession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
}

pub struct TerminalManager {
    sessions: Mutex<HashMap<String, TerminalSession>>,
}

impl TerminalManager {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub async fn open(
        &self,
        session_id: &str,
        program: PathBuf,
        args: Vec<String>,
        cols: u16,
        rows: u16,
        app: AppHandle,
        window_label: String,
    ) -> Result<(), TerrariumError> {
        let pty_system = native_pty_system();

        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| TerrariumError::TerminalError {
                message: format!("Failed to open PTY: {}", e),
            })?;

        let mut cmd = CommandBuilder::new(&program);
        cmd.args(&args);

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| TerrariumError::TerminalError {
                message: format!("Failed to spawn command: {}", e),
            })?;

        // Drop the slave side — we only need the master
        drop(pair.slave);

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| TerrariumError::TerminalError {
                message: format!("Failed to get PTY writer: {}", e),
            })?;

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| TerrariumError::TerminalError {
                message: format!("Failed to get PTY reader: {}", e),
            })?;

        let session = TerminalSession {
            master: pair.master,
            writer,
            child,
        };

        self.sessions
            .lock()
            .await
            .insert(session_id.to_string(), session);

        // Start read loop in background
        let sid = session_id.to_string();
        std::thread::spawn(move || {
            Self::read_loop(reader, app, window_label, sid);
        });

        Ok(())
    }

    fn read_loop(
        mut reader: Box<dyn Read + Send>,
        app: AppHandle,
        window_label: String,
        session_id: String,
    ) {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let encoded = BASE64.encode(&buf[..n]);
                    if let Some(window) = app.get_webview_window(&window_label) {
                        let _ = window.emit("terminal-output", &encoded);
                    } else {
                        break;
                    }
                }
                Err(_) => break,
            }
        }

        // Notify the window the session has ended
        if let Some(window) = app.get_webview_window(&window_label) {
            let _ = window.emit("terminal-exit", &session_id);
        }
    }

    pub async fn write(&self, session_id: &str, data: &[u8]) -> Result<(), TerrariumError> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| TerrariumError::TerminalError {
                message: format!("No terminal session: {}", session_id),
            })?;

        session
            .writer
            .write_all(data)
            .map_err(|e| TerrariumError::TerminalError {
                message: format!("Write failed: {}", e),
            })?;

        Ok(())
    }

    pub async fn resize(
        &self,
        session_id: &str,
        cols: u16,
        rows: u16,
    ) -> Result<(), TerrariumError> {
        let sessions = self.sessions.lock().await;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| TerrariumError::TerminalError {
                message: format!("No terminal session: {}", session_id),
            })?;

        session
            .master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| TerrariumError::TerminalError {
                message: format!("Resize failed: {}", e),
            })?;

        Ok(())
    }

    pub async fn close(&self, session_id: &str) {
        let mut sessions = self.sessions.lock().await;
        if let Some(mut session) = sessions.remove(session_id) {
            // Drop the writer to close stdin
            drop(session.writer);
            // Kill the child process
            let _ = session.child.kill();
            // Master will be dropped, closing the PTY
        }
    }

    pub async fn has_session(&self, session_id: &str) -> bool {
        self.sessions.lock().await.contains_key(session_id)
    }
}
