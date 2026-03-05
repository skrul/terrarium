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
        eprintln!("[read_loop] started for session={} window={}", session_id, window_label);
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    eprintln!("[read_loop] EOF for session={}", session_id);
                    break;
                }
                Ok(n) => {
                    let encoded = BASE64.encode(&buf[..n]);
                    if app.get_webview_window(&window_label).is_some() {
                        // emit_to targets only the specific window's listeners
                        let result = app.emit_to(&window_label, "terminal-output", &encoded);
                        if result.is_err() {
                            eprintln!("[read_loop] emit failed for session={}: {:?}", session_id, result);
                        }
                    } else {
                        eprintln!("[read_loop] window '{}' not found, breaking for session={}", window_label, session_id);
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("[read_loop] read error for session={}: {}", session_id, e);
                    break;
                }
            }
        }

        eprintln!("[read_loop] ended for session={}", session_id);
        // Notify the window the session has ended
        if app.get_webview_window(&window_label).is_some() {
            let _ = app.emit_to(&window_label, "terminal-exit", &session_id);
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
        eprintln!("[terminal] close called for session={}", session_id);
        let mut sessions = self.sessions.lock().await;
        if let Some(mut session) = sessions.remove(session_id) {
            eprintln!("[terminal] closing session={}, dropping writer and killing child", session_id);
            // Drop the writer to close stdin
            drop(session.writer);
            // Kill the child process
            let _ = session.child.kill();
            // Master will be dropped, closing the PTY
        } else {
            eprintln!("[terminal] close: session={} not found (already closed?)", session_id);
        }
    }

    pub async fn has_session(&self, session_id: &str) -> bool {
        self.sessions.lock().await.contains_key(session_id)
    }
}
