use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use catacomb_ipc::{ClientInfo, IpcMessage};
use serde_json;

pub struct CatacombClient;

impl CatacombClient {
    fn connect() -> std::io::Result<UnixStream> {
        let socket_name = env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".to_string());
        let socket_path = catacomb_ipc::socket_path(&socket_name);
        UnixStream::connect(socket_path)
    }

    pub fn send_message(msg: IpcMessage) -> std::io::Result<()> {
        let mut stream = Self::connect()?;
        let json = serde_json::to_string(&msg)?;
        stream.write_all(json.as_bytes())?;
        stream.write_all(b"\n")?;
        stream.flush()?;
        Ok(())
    }

    pub fn request<T, F>(msg: IpcMessage, extractor: F) -> Option<T>
    where
        F: Fn(IpcMessage) -> Option<T>,
    {
        let mut stream = Self::connect().ok()?;
        let json = serde_json::to_string(&msg).ok()?;
        stream.write_all(json.as_bytes()).ok()?;
        stream.write_all(b"\n").ok()?;
        stream.flush().ok()?;

        let mut reader = BufReader::new(stream);
        let mut response = String::new();
        reader.read_line(&mut response).ok()?;

        let reply: IpcMessage = serde_json::from_str(&response).ok()?;
        extractor(reply)
    }

    pub fn focus_window(app_id_regex: &str) {
        let _ = Self::send_message(IpcMessage::Focus {
            app_id: app_id_regex.to_string(),
        });
    }

    pub fn close_window(app_id_regex: &str) {
        let _ = Self::send_message(IpcMessage::CloseWindow {
            app_id: app_id_regex.to_string(),
        });
    }

    pub fn exec(command: &str) {
        if let Err(e) = Self::send_message(IpcMessage::Exec {
            command: command.to_string(),
        }) {
            eprintln!("CatacombClient: Failed to send Exec command: {}", e);
        } else {
             println!("CatacombClient: Sent Exec command: {}", command);
        }
    }

    pub fn exec_or_focus(command: &str, app_id_hint: Option<&str>, card_id: Option<&str>) {
        let msg = IpcMessage::ExecOrFocus {
            command: command.to_string(),
            app_id_hint: app_id_hint.map(|s| s.to_string()),
            card_id: card_id.map(|s| s.to_string()),
        };
        if let Err(e) = Self::send_message(msg) {
            eprintln!("CatacombClient: Failed to send ExecOrFocus: {}", e);
        } else {
            println!("CatacombClient: Sent ExecOrFocus: {}", command);
        }
    }
    pub fn get_active_window() -> Option<(String, String)> {
        Self::request(IpcMessage::GetActiveWindow, |reply| {
            if let IpcMessage::ActiveWindow { title, app_id } = reply {
                Some((title, app_id))
            } else {
                None
            }
        })
    }

    pub fn get_clients() -> Vec<ClientInfo> {
        Self::request(IpcMessage::GetClients, |reply| {
            if let IpcMessage::Clients { clients } = reply {
                Some(clients)
            } else {
                None
            }
        })
        .unwrap_or_default()
    }

    pub fn get_output_info() -> Option<(i32, i32, i32, f64, catacomb_ipc::Orientation)> {
        Self::request(IpcMessage::GetOutputInfo, |reply| {
            if let IpcMessage::OutputInfo { width, height, refresh, scale, orientation } = reply {
                Some((width, height, refresh, scale, orientation))
            } else {
                None
            }
        })
    }

    pub fn get_output_modes() -> Vec<catacomb_ipc::OutputMode> {
        Self::request(IpcMessage::GetOutputModes, |reply| {
            if let IpcMessage::OutputModes { modes } = reply {
                Some(modes)
            } else {
                None
            }
        })
        .unwrap_or_default()
    }

    pub fn set_system_role(role: &str, app_id_regex: &str) {
        let _ = Self::send_message(IpcMessage::SystemRole {
            role: role.to_string(),
            app_id: app_id_regex.to_string(),
        });
    }

    pub fn role_action(role: &str, action: &str, payload: Option<&str>) {
        let _ = Self::send_message(IpcMessage::RoleAction {
            role: role.to_string(),
            action: action.to_string(),
            payload: payload.map(|p| p.to_string()),
        });
    }
    
    pub fn home_select() {
        let _ = Self::send_message(IpcMessage::RoleAction {
            role: "home".to_string(),
            action: "select".to_string(),
            payload: None,
        });
    }
    
    pub fn home_navigate(dir: &str) {
        let _ = Self::send_message(IpcMessage::RoleAction {
            role: "home".to_string(),
            action: "navigate".to_string(),
            payload: Some(dir.to_string()),
        });
    }
    
    pub fn home_back() {
        let _ = Self::send_message(IpcMessage::RoleAction {
            role: "home".to_string(),
            action: "back".to_string(),
            payload: None,
        });
    }
    
    pub fn home_focus() {
        let _ = Self::send_message(IpcMessage::RoleAction {
            role: "home".to_string(),
            action: "focus".to_string(),
            payload: None,
        });
    }
}
