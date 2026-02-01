use anyhow::{Context, Result};
use catacomb_ipc::{IpcMessage, KeyTrigger, Keysyms};
use std::fs;
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::thread;
use std::time::Duration;

pub fn run() -> Result<()> {
    println!("🚀 Jolly Startup running...");

    // Wait for socket to be ready (retry loop)
    wait_for_socket()?;

    // 1. Key Bindings
    setup_key_bindings()?;

    // 2. System Roles
    setup_roles()?;

    // 3. Settings
    setup_settings()?;

    // 4. Spawn background apps
    spawn_apps()?;

    println!("✅ Jolly Startup completed.");
    Ok(())
}

fn wait_for_socket() -> Result<()> {
    let max_retries = 50;
    for _i in 0..max_retries {
        // Try to get output info as a ping
        let msg = IpcMessage::GetOutputInfo;
        if catacomb_ipc::send_message(&msg).is_ok() {
            println!("Connected to Catacomb IPC.");
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    anyhow::bail!("Timed out waiting for Catacomb socket");
}

fn send(msg: IpcMessage) -> Result<()> {
    match catacomb_ipc::send_message(&msg) {
        Ok(_) => Ok(()),
        Err(e) => anyhow::bail!("Failed to send IPC message: {}", e),
    }
}

fn setup_key_bindings() -> Result<()> {
    // Ignore power button (handled by inhibiting)
    Command::new("systemd-inhibit")
        .args(&["--what", "handle-power-key", "sleep", "infinity"])
        .spawn()
        .context("Failed to spawn systemd-inhibit")?;

    // Power Press
    send(IpcMessage::BindKey {
        app_id: "*".to_string(),
        mods: None,
        trigger: KeyTrigger::Press,
        keys: Keysyms::from_str("XF86PowerOff")?,
        program: "bash".to_string(),
        arguments: vec![
            "-c".to_string(),
            "sleep 0.5 && if [ \"$(catacomb msg dpms)\" == \"on\" ]; then (tremor 150 0 1; tzompantli); fi".to_string()
        ],
    })?;

    // Power Release (DPMS toggle)
    send(IpcMessage::BindKey {
        app_id: "*".to_string(),
        mods: None,
        trigger: KeyTrigger::Release,
        keys: Keysyms::from_str("XF86PowerOff")?,
        program: "bash".to_string(),
        arguments: vec![
            "-c".to_string(),
            "if pkill -xf -SIGINT \"sleep 0.5\"; then if [ \"$(catacomb msg dpms)\" == \"on\" ]; then catacomb msg dpms off; else catacomb msg dpms on; fi; fi".to_string()
        ],
    })?;

    // Xbox Button -> Jolly Nav
    send(IpcMessage::BindKey {
        app_id: "*".to_string(),
        mods: None,
        trigger: KeyTrigger::Press,
        keys: Keysyms::from_str("btn_mode")?,
        program: "bash".to_string(),
        arguments: vec![
            "-c".to_string(),
            "if ! ps -C jolly-nav -o stat= | grep -v \"Z\" | grep -q .; then jolly-nav; else pkill -USR1 -x jolly-nav; fi".to_string()
        ],
    })?;

    // Screenshot (Power + VolDown)
    send(IpcMessage::BindKey {
        app_id: "*".to_string(),
        mods: None,
        trigger: KeyTrigger::Press,
        keys: Keysyms::from_str("XF86PowerOff+XF86AudioLowerVolume")?,
        program: "bash".to_string(),
        arguments: vec![
            "-c".to_string(),
            "pkill -xf -SIGINT \"sleep 0.5\"; tremor 150 0 1; geometry=$(slurp 2>&1); if [[ $geometry == \"selection cancelled\" ]]; then grim /tmp/screenshot.png; else grim -g \"$geometry\" /tmp/screenshot.png; fi".to_string()
        ],
    })?;

    // Virtual Keyboard
    send(IpcMessage::BindKey {
        app_id: "*".to_string(),
        mods: None,
        trigger: KeyTrigger::Press,
        keys: Keysyms::from_str("EnableVirtualKeyboard")?,
        program: "busctl".to_string(),
        arguments: vec![
            "call".to_string(), "--user".to_string(), "sm.puri.OSK0".to_string(), 
            "/sm/puri/OSK0".to_string(), "sm.puri.OSK0".to_string(), "SetVisible".to_string(), 
            "b".to_string(), "true".to_string()
        ],
    })?;
    
    send(IpcMessage::BindKey {
        app_id: "*".to_string(),
        mods: None,
        trigger: KeyTrigger::Press,
        keys: Keysyms::from_str("AutoVirtualKeyboard")?,
        program: "busctl".to_string(),
        arguments: vec![
            "call".to_string(), "--user".to_string(), "sm.puri.OSK0".to_string(), 
            "/sm/puri/OSK0".to_string(), "sm.puri.OSK0".to_string(), "SetVisible".to_string(), 
            "b".to_string(), "false".to_string()
        ],
    })?;

    // Volume Keys
    send(IpcMessage::BindKey {
        app_id: "*".to_string(),
        mods: None,
        trigger: KeyTrigger::Repeat,
        keys: Keysyms::from_str("XF86AudioRaiseVolume")?,
        program: "pactl".to_string(),
        arguments: vec!["set-sink-volume".to_string(), "0".to_string(), "+5%".to_string()],
    })?;
    
    send(IpcMessage::BindKey {
        app_id: "*".to_string(),
        mods: None,
        trigger: KeyTrigger::Repeat,
        keys: Keysyms::from_str("XF86AudioLowerVolume")?,
        program: "pactl".to_string(),
        arguments: vec!["set-sink-volume".to_string(), "0".to_string(), "-5%".to_string()],
    })?;

    Ok(())
}

fn setup_roles() -> Result<()> {
    send(IpcMessage::SystemRole {
        role: "nav".to_string(),
        app_id: "(?i)jellyfin.*".to_string(),
    })?;
    Ok(())
}

fn setup_settings() -> Result<()> {
    Command::new("gsettings")
        .args(&["set", "org.gnome.desktop.a11y.applications", "screen-keyboard-enabled", "true"])
        .spawn()?
        .wait()?;
    Ok(())
}

fn spawn_apps() -> Result<()> {
    // squeekboard
    // Command::new("squeekboard").spawn()?;

    // Jolly Home
    // We can just try to spawn jolly-home, assuming it is in PATH
    let log_path = if let Ok(home) = std::env::var("HOME") {
        format!("{}/jolly-home.log", home)
    } else {
        "/tmp/jolly-home.log".to_string()
    };
    println!("📝 Jolly Home logs will be written to: {}", log_path);

    let log_file = fs::File::create(&log_path).context("Failed to create log file")?;
    let log_file_err = log_file.try_clone().context("Failed to clone log file handle")?;

    // Find jolly-home binary
    let cwd = std::env::current_dir()?;
    let mut home_binary = cwd.join("target/debug/jolly-home");
    if !home_binary.exists() {
        let release_binary = cwd.join("target/release/jolly-home");
        if release_binary.exists() {
            home_binary = release_binary;
        } else {
            // Fallback to expecting it in PATH if not found in target
            home_binary = std::path::PathBuf::from("jolly-home");
        }
    }

    println!("🏠 Spawning Jolly Home from: {:?}", home_binary);

    Command::new(home_binary)
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_file_err))
        .spawn()
        .context("Failed to spawn jolly-home")?;
        
    Ok(())
}
