mod error;
mod host_api;
mod project;
mod runtime;
mod terminal;

use std::sync::Arc;

use project::{Project, ProjectStatus};
use runtime::lima::LimaRuntime;
use runtime::types::{RuntimeStatus, VmStatus};
use runtime::ContainerRuntime;
use terminal::TerminalManager;
use tokio::sync::Mutex;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use tauri::{Manager, State, WebviewUrl, WebviewWindowBuilder};
use uuid::Uuid;

pub struct AppState {
    projects: Mutex<Vec<Project>>,
    runtime: Arc<dyn ContainerRuntime>,
    terminals: TerminalManager,
    host_api_port: u16,
}

impl AppState {
    fn new(host_api_port: u16) -> Self {
        Self {
            projects: Mutex::new(Vec::new()),
            runtime: Arc::new(LimaRuntime::new()),
            terminals: TerminalManager::new(),
            host_api_port,
        }
    }
}

#[tauri::command]
async fn list_projects(state: State<'_, AppState>) -> Result<Vec<Project>, String> {
    Ok(state.projects.lock().await.clone())
}

#[tauri::command]
async fn create_project(name: String, state: State<'_, AppState>) -> Result<Project, String> {
    let id = Uuid::new_v4().to_string();

    // Insert project in Creating state
    let project = Project {
        id: id.clone(),
        name,
        status: ProjectStatus::Creating,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    state.projects.lock().await.push(project.clone());

    // Ensure VM is ready and create namespace
    if let Err(e) = state.runtime.ensure_vm_ready().await {
        let mut projects = state.projects.lock().await;
        if let Some(p) = projects.iter_mut().find(|p| p.id == id) {
            p.status = ProjectStatus::Error;
        }
        return Err(e.to_string());
    }

    if let Err(e) = state.runtime.create_namespace(&id).await {
        let mut projects = state.projects.lock().await;
        if let Some(p) = projects.iter_mut().find(|p| p.id == id) {
            p.status = ProjectStatus::Error;
        }
        return Err(e.to_string());
    }

    // Build dev image (fast no-op if already built)
    if let Err(e) = state.runtime.ensure_dev_image().await {
        let mut projects = state.projects.lock().await;
        if let Some(p) = projects.iter_mut().find(|p| p.id == id) {
            p.status = ProjectStatus::Error;
        }
        return Err(e.to_string());
    }

    // Load image into project namespace
    if let Err(e) = state.runtime.load_dev_image_into_namespace(&id).await {
        let mut projects = state.projects.lock().await;
        if let Some(p) = projects.iter_mut().find(|p| p.id == id) {
            p.status = ProjectStatus::Error;
        }
        return Err(e.to_string());
    }

    // Run the dev container
    if let Err(e) = state.runtime.run_dev_container(&id).await {
        let mut projects = state.projects.lock().await;
        if let Some(p) = projects.iter_mut().find(|p| p.id == id) {
            p.status = ProjectStatus::Error;
        }
        return Err(e.to_string());
    }

    // Update project status to Running
    let mut projects = state.projects.lock().await;
    if let Some(p) = projects.iter_mut().find(|p| p.id == id) {
        p.status = ProjectStatus::Running;
    }

    let project = projects.iter().find(|p| p.id == id).cloned();
    Ok(project.expect("Project was just inserted"))
}

#[tauri::command]
async fn delete_project(id: String, state: State<'_, AppState>) -> Result<(), String> {
    // Close any terminal session for this project
    state.terminals.close(&id).await;

    // Remove dev container first (best effort — VM might not be running)
    let _ = state.runtime.remove_dev_container(&id).await;

    // Then delete the namespace
    let _ = state.runtime.delete_namespace(&id).await;

    state.projects.lock().await.retain(|p| p.id != id);
    Ok(())
}

#[tauri::command]
async fn get_runtime_status(state: State<'_, AppState>) -> Result<RuntimeStatus, String> {
    Ok(state.runtime.runtime_status().await)
}

#[tauri::command]
async fn get_vm_status(state: State<'_, AppState>) -> Result<VmStatus, String> {
    Ok(state.runtime.vm_status().await)
}

#[tauri::command]
async fn start_vm(state: State<'_, AppState>) -> Result<(), String> {
    state
        .runtime
        .ensure_vm_ready()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn stop_vm(state: State<'_, AppState>) -> Result<(), String> {
    state.runtime.stop_vm().await.map_err(|e| e.to_string())
}

/// Check if global Claude Code auth credentials exist.
#[tauri::command]
async fn check_auth_status(state: State<'_, AppState>) -> Result<bool, String> {
    state
        .runtime
        .has_auth_credentials()
        .await
        .map_err(|e| e.to_string())
}

/// Run the OAuth flow headlessly: starts auth container, runs `claude auth login`,
/// opens browser via host-open, waits for completion.
#[tauri::command]
async fn start_oauth_flow(state: State<'_, AppState>) -> Result<(), String> {
    // Ensure VM is ready
    state
        .runtime
        .ensure_vm_ready()
        .await
        .map_err(|e| e.to_string())?;

    // Ensure dev image is built
    state
        .runtime
        .ensure_dev_image()
        .await
        .map_err(|e| e.to_string())?;

    // Ensure auth directory exists on VM
    state
        .runtime
        .ensure_auth_dir()
        .await
        .map_err(|e| e.to_string())?;

    // Determine the host API URL as seen from inside the VM
    let gateway_ip = state
        .runtime
        .host_gateway_ip()
        .await
        .map_err(|e| e.to_string())?;
    let host_api_url = format!("http://{}:{}", gateway_ip, state.host_api_port);

    // Run claude auth login headlessly
    state
        .runtime
        .run_auth_login(&host_api_url)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Cancel an in-progress OAuth flow by killing the auth container.
#[tauri::command]
async fn cancel_oauth_flow(state: State<'_, AppState>) -> Result<(), String> {
    state
        .runtime
        .cancel_auth_login()
        .await
        .map_err(|e| e.to_string())
}

/// Remove shared auth credentials (sign out of Claude Code).
#[tauri::command]
async fn sign_out(state: State<'_, AppState>) -> Result<(), String> {
    state
        .runtime
        .remove_auth_credentials()
        .await
        .map_err(|e| e.to_string())
}

/// Called from the dashboard to create/focus the terminal window.
/// Does NOT start the PTY — that happens when TerminalView calls start_terminal.
#[tauri::command]
async fn open_terminal(
    project_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Verify project exists and is Running
    {
        let projects = state.projects.lock().await;
        let project = projects
            .iter()
            .find(|p| p.id == project_id)
            .ok_or_else(|| format!("Project not found: {}", project_id))?;

        if project.status != ProjectStatus::Running {
            return Err(format!(
                "Project is not running (status: {:?})",
                project.status
            ));
        }
    }

    let window_label = format!("terminal-{}", project_id);

    // If the window already exists, focus it
    if let Some(window) = app.get_webview_window(&window_label) {
        let _ = window.set_focus();
        return Ok(());
    }

    // Create a new terminal window
    let url = format!("#/terminal/{}", project_id);
    let _window = WebviewWindowBuilder::new(&app, &window_label, WebviewUrl::App(url.into()))
        .title("Terrarium — Terminal".to_string())
        .inner_size(800.0, 600.0)
        .build()
        .map_err(|e| format!("Failed to create terminal window: {}", e))?;

    Ok(())
}

/// Check whether a project's dev container has previous Claude Code sessions.
#[tauri::command]
async fn check_claude_sessions(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    state.runtime.has_claude_sessions(&project_id).await.map_err(|e| e.to_string())
}

/// Called from TerminalView after event listeners are set up.
/// Starts (or reattaches to) the PTY session.
#[tauri::command]
async fn start_terminal(
    project_id: String,
    continue_session: bool,
    cols: u16,
    rows: u16,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let window_label = format!("terminal-{}", project_id);

    // If session already exists (e.g. window reopened), clean it up first
    if state.terminals.has_session(&project_id).await {
        state.terminals.close(&project_id).await;
    }

    // Determine the host API URL as seen from inside the VM
    let gateway_ip = state
        .runtime
        .host_gateway_ip()
        .await
        .map_err(|e| e.to_string())?;
    let host_api_url = format!("http://{}:{}", gateway_ip, state.host_api_port);

    // Get the terminal command from the runtime
    let (program, args) = state
        .runtime
        .terminal_command(&project_id, &host_api_url, continue_session)
        .await
        .map_err(|e| e.to_string())?;

    // Open the PTY session
    state
        .terminals
        .open(
            &project_id,
            program,
            args,
            cols,
            rows,
            app.clone(),
            window_label,
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
async fn write_terminal(
    session_id: String,
    data: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let bytes = BASE64
        .decode(&data)
        .map_err(|e| format!("Invalid base64: {}", e))?;

    state
        .terminals
        .write(&session_id, &bytes)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn resize_terminal(
    session_id: String,
    cols: u16,
    rows: u16,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .terminals
        .resize(&session_id, cols, rows)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn close_terminal(session_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.terminals.close(&session_id).await;
    Ok(())
}

use tauri::AppHandle;

const HOST_API_PORT: u16 = 7778;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            list_projects,
            create_project,
            delete_project,
            get_runtime_status,
            get_vm_status,
            start_vm,
            stop_vm,
            check_auth_status,
            start_oauth_flow,
            cancel_oauth_flow,
            sign_out,
            open_terminal,
            check_claude_sessions,
            start_terminal,
            write_terminal,
            resize_terminal,
            close_terminal,
        ])
        .setup(|app| {
            // Start host API server (synchronously block to get the port)
            let host_api_port = tauri::async_runtime::block_on(async {
                host_api::start(HOST_API_PORT).await
            })
            .map_err(|e| {
                eprintln!("Failed to start host API: {}", e);
                Box::<dyn std::error::Error>::from(e)
            })?;

            let app_state = AppState::new(host_api_port);
            app.manage(app_state);

            // Background VM start if it already exists
            let state = app.state::<AppState>();
            let runtime = state.runtime.clone();

            tauri::async_runtime::spawn(async move {
                let status = runtime.vm_status().await;
                if status == VmStatus::Stopped {
                    let _ = runtime.start_vm().await;
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                let label = window.label().to_string();
                if let Some(project_id) = label.strip_prefix("terminal-") {
                    let project_id = project_id.to_string();
                    let app = window.app_handle().clone();
                    tauri::async_runtime::spawn(async move {
                        let state = app.state::<AppState>();
                        state.terminals.close(&project_id).await;
                    });
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
