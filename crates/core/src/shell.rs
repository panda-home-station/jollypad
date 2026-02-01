use crate::CatacombClient;
use std::process::Command;
use std::thread;
use std::time::Duration;
use std::env;

pub fn dispatch_exec(cmd: &str, card_id: Option<&str>) {
    if cmd.is_empty() { return; }

    if cmd.trim().starts_with("game-launcher ") {
        let app_id = cmd.trim().strip_prefix("game-launcher ").unwrap().trim().to_string();
        let card_id = card_id.map(|s| s.to_string());
        
        thread::spawn(move || {
            match crate::game_launcher::prepare_game_launch(&app_id) {
                Ok(info) => {
                    let mut final_cmd = String::new();
                    // Envs
                    for (k, v) in info.envs {
                        final_cmd.push_str(&format!("{}={} ", k, shell_words::quote(&v)));
                    }
                    // Program
                    final_cmd.push_str(&shell_words::quote(&info.program.to_string_lossy()));
                    // Args
                    for arg in info.args {
                        final_cmd.push_str(" ");
                        final_cmd.push_str(&shell_words::quote(&arg));
                    }
                    
                    println!("Launching game via library: {}", final_cmd);
                    CatacombClient::exec_or_focus(&final_cmd, Some(&app_id), card_id.as_deref());
                }
                Err(e) => {
                    eprintln!("Failed to prepare game launch for {}: {}", app_id, e);
                }
            }
        });
        return;
    }

    let trimmed = cmd.trim();
    
    fn extract_flatpak_app_id(cmd: &str) -> Option<String> {
        let tokens: Vec<&str> = cmd.split_whitespace().collect();
        if tokens.len() >= 2 && tokens[0] == "flatpak" && tokens[1] == "run" {
            for tok in tokens.iter().skip(2) {
                let s = *tok;
                if !s.starts_with('-') && s.contains('.') {
                    return Some(s.to_string());
                }
            }
            return tokens.last().map(|s| s.to_string());
        }
        if tokens.len() >= 3 && tokens[0] == "/usr/bin/flatpak" && tokens[1] == "run" {
            for tok in tokens.iter().skip(2) {
                let s = *tok;
                if !s.starts_with('-') && s.contains('.') {
                    return Some(s.to_string());
                }
            }
            return tokens.last().map(|s| s.to_string());
        }
        None
    }
    
    let hint = extract_flatpak_app_id(trimmed);
    CatacombClient::exec_or_focus(cmd, hint.as_deref(), card_id);
}

pub fn show_desktop() {
    CatacombClient::focus_window("^(JollyPad-Desktop|jolly-home|JollyPad-Launcher)$");
}

pub fn show_nav_overlay() {
    let mut nav_cmd = "jolly-nav".to_string();
    if let Ok(exe_path) = env::current_exe() {
        if let Some(dir) = exe_path.parent() {
            let local_nav = dir.join("jolly-nav");
            if local_nav.exists() {
                if let Some(path_str) = local_nav.to_str() {
                    nav_cmd = path_str.to_string();
                }
            }
        }
    }
    
    // Check if running
    if !is_nav_running() {
        CatacombClient::exec(&nav_cmd);
    }
    
    // Ensure focus
    thread::spawn(move || {
        for _ in 0..10 {
            thread::sleep(Duration::from_millis(50));
            CatacombClient::focus_window("jolly-nav");
        }
    });
}

pub fn is_nav_running() -> bool {
    // Check if jolly-nav is running using pgrep, but exclude zombie processes
    if let Ok(output) = Command::new("pgrep").arg("-x").arg("jolly-nav").output() {
        let pids = String::from_utf8_lossy(&output.stdout);
        for pid_str in pids.lines() {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                // Check state of this PID
                if let Ok(state_output) = Command::new("ps").arg("-o").arg("state=").arg("-p").arg(pid.to_string()).output() {
                    let state = String::from_utf8_lossy(&state_output.stdout).trim().to_string();
                    // Z is zombie, T is stopped, etc. We consider it running if it's not a Zombie.
                    if !state.contains('Z') {
                        return true;
                    }
                }
            }
        }
    }
    false
}
