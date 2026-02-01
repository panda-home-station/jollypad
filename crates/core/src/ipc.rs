use std::env;
use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::thread;
use std::sync::mpsc::{channel, Receiver, Sender};

#[derive(Debug, Clone)]
pub enum HyprEvent {
    ActiveWindow { class: String, title: String },
    OpenWindow { address: String, workspace: String, class: String, title: String },
    CloseWindow { address: String },
    Workspace { id: String },
    Unknown(String),
}

pub struct HyprlandListener {
    rx: Receiver<HyprEvent>,
}

impl HyprlandListener {
    pub fn new() -> Option<Self> {
        let signature = env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
        let socket_path = PathBuf::from(format!("/tmp/hypr/{}/.socket2.sock", signature));

        if !socket_path.exists() {
            return None;
        }

        let (tx, rx) = channel();

        thread::spawn(move || {
            Self::listen_loop(socket_path, tx);
        });

        Some(Self { rx })
    }

    fn listen_loop(socket_path: PathBuf, tx: Sender<HyprEvent>) {
        let stream = match UnixStream::connect(socket_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to connect to Hyprland IPC: {}", e);
                return;
            }
        };

        let reader = BufReader::new(stream);

        for line in reader.lines() {
            if let Ok(l) = line {
                if let Some(event) = Self::parse_event(&l) {
                    if tx.send(event).is_err() {
                        break;
                    }
                }
            }
        }
    }

    fn parse_event(raw: &str) -> Option<HyprEvent> {
        let parts: Vec<&str> = raw.split(">>").collect();
        if parts.len() < 2 {
            return None;
        }

        let event_name = parts[0];
        let event_data = parts[1];

        match event_name {
            "activewindow" => {
                // Data: class,title
                let data_parts: Vec<&str> = event_data.splitn(2, ',').collect();
                if data_parts.len() == 2 {
                    Some(HyprEvent::ActiveWindow {
                        class: data_parts[0].to_string(),
                        title: data_parts[1].to_string(),
                    })
                } else {
                    // Sometimes activewindow sends empty or just one part if no window
                    Some(HyprEvent::ActiveWindow { class: "".to_string(), title: "".to_string() })
                }
            },
            "openwindow" => {
                // Data: address,workspace,class,title
                let data_parts: Vec<&str> = event_data.split(',').collect();
                if data_parts.len() >= 4 {
                    Some(HyprEvent::OpenWindow {
                        address: data_parts[0].to_string(),
                        workspace: data_parts[1].to_string(),
                        class: data_parts[2].to_string(),
                        title: data_parts[3].to_string(),
                    })
                } else {
                    None
                }
            },
            "closewindow" => {
                Some(HyprEvent::CloseWindow { address: event_data.to_string() })
            },
            "workspace" => {
                Some(HyprEvent::Workspace { id: event_data.to_string() })
            },
            _ => Some(HyprEvent::Unknown(raw.to_string())),
        }
    }

    pub fn try_recv(&self) -> Option<HyprEvent> {
        self.rx.try_recv().ok()
    }
}
