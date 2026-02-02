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

    // 4. Audio Setup
    setup_audio()?;

    // 5. Spawn background apps
    spawn_apps()?;

    println!("✅ Jolly Startup completed.");
    Ok(())
}

fn setup_audio() -> Result<()> {
    println!("🔊 Setting up audio...");
    
    // Unload modules that cause issues (RAOP casting, Suspend)
    let _ = Command::new("pactl").args(&["unload-module", "module-raop-discover"]).status();
    let _ = Command::new("pactl").args(&["unload-module", "module-suspend-on-idle"]).status();
    
    // Retry finding HDMI sink for up to 5 seconds
    let script = "
        echo \"🔊 Initializing Audio...\"
        
        # 1. Find NVIDIA card and set profile to HDMI
        # We need this because sometimes it defaults to 'off' or 'pro-audio' which has no HDMI sink
        card=$(pactl list cards short | grep -i \"nvidia\" | cut -f2 | head -n1)
        if [ ! -z \"$card\" ]; then
            echo \"Found NVIDIA card: $card\"
            echo \"Setting profile to output:hdmi-stereo...\"
            pactl set-card-profile \"$card\" output:hdmi-stereo
        else
            echo \"⚠️ Could not find NVIDIA card in PulseAudio\"
        fi
        
        sleep 1

        for i in {1..10}; do
            sink=$(pactl list short sinks | grep 'hdmi' | cut -f2 | head -n1)
            if [ ! -z \"$sink\" ]; then
                pactl set-default-sink \"$sink\"
                echo \"Found HDMI sink: $sink\"
                exit 0
            fi
            sleep 0.5
        done
        echo \"No HDMI sink found\"
        exit 1
    ";
    
    let status = Command::new("bash")
        .args(&["-c", script])
        .status()
        .context("Failed to execute audio setup script")?;

    if status.success() {
        println!("✅ HDMI Audio set as default");
    } else {
        println!("⚠️ Failed to find HDMI Audio sink");
    }
    
    // Force unmute all HDMI IEC958 switches on NVidia card (card 1 usually)
    // We try to find the card index for "NVidia" just in case
    let card_script = "aplay -l | grep 'NVidia' | head -n1 | cut -d' ' -f2 | tr -d ':'";
    let card_output = Command::new("bash").args(&["-c", card_script]).output();
    
    if let Ok(output) = card_output {
        let card = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !card.is_empty() {
             println!("🔧 Unmuting HDMI switches on card {}...", card);
             let _ = Command::new("amixer")
                .args(&["-c", &card, "sset", "IEC958", "on"])
                .status();
             let _ = Command::new("amixer")
                .args(&["-c", &card, "sset", "IEC958,1", "on"])
                .status();
             let _ = Command::new("amixer")
                .args(&["-c", &card, "sset", "IEC958,2", "on"])
                .status();
             let _ = Command::new("amixer")
                .args(&["-c", &card, "sset", "IEC958,3", "on"])
                .status();
        }
    }

    // Set volume to 100% and unmute
    let _ = Command::new("pactl")
        .args(&["set-sink-volume", "@DEFAULT_SINK@", "100%"])
        .status();

    let _ = Command::new("pactl")
        .args(&["set-sink-mute", "@DEFAULT_SINK@", "0"])
        .status();

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
        arguments: vec!["set-sink-volume".to_string(), "@DEFAULT_SINK@".to_string(), "+5%".to_string()],
    })?;
    
    send(IpcMessage::BindKey {
        app_id: "*".to_string(),
        mods: None,
        trigger: KeyTrigger::Repeat,
        keys: Keysyms::from_str("XF86AudioLowerVolume")?,
        program: "pactl".to_string(),
        arguments: vec!["set-sink-volume".to_string(), "@DEFAULT_SINK@".to_string(), "-5%".to_string()],
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
