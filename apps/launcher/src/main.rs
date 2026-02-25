use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::thread;
use std::time::Duration;
use sysinfo::{ProcessRefreshKind, RefreshKind, System};

mod startup;

fn main() -> Result<()> {
    println!("🚀 Starting JollyPad Launcher...");

    // Set the Wayland display socket name for all children (Catacomb and Startup)
    env::set_var("WAYLAND_DISPLAY", "wayland-0");

    // 1. Clean up initrc (to ensure catacomb doesn't run it automatically)
    cleanup_initrc()?;

    // 2. Kill potential conflicting processes
    println!("🧹 Cleaning up old processes...");
    cleanup_processes();
    
    // 3. Wait a moment for cleanup
    thread::sleep(Duration::from_secs(2));

    // 4. Start Startup Script in background
    println!("� Starting startup script in background...");
    
    // Spawn startup function in a separate thread
    thread::spawn(|| {
        // Give catacomb a moment to initialize the socket
        thread::sleep(Duration::from_secs(1));
        println!("🚀 Running startup sequence...");
        if let Err(e) = startup::run() {
            eprintln!("❌ Startup failed: {}", e);
        }
    });

    // 5. Start Catacomb (BLOCKING)
    println!("👻 Starting catacomb (embedded)...");
    
    // This will block until the compositor exits
    catacomb::run();
    
    println!("👻 Catacomb exited normally.");
    Ok(())
}

fn cleanup_initrc() -> Result<()> {
    let config_dir = dirs::config_dir()
        .context("Could not find config directory")?
        .join("catacomb");
    let initrc_path = config_dir.join("initrc");
    
    if initrc_path.exists() {
        println!("🧹 Removing old initrc at {:?}", initrc_path);
        fs::remove_file(&initrc_path)?;
    }
    Ok(())
}


fn cleanup_processes() {
    let mut system = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
    );
    system.refresh_processes();

    let targets = [
        "catacomb",
        "jolly-home",
        "epitaph",
        "sway",
        "weston",
        "Xorg",
        "gnome-shell",
        // also kill the old script if it's somehow running? unlikely
    ];

    let current_pid = std::process::id();

    for process in system.processes().values() {
        if process.pid().as_u32() == current_pid {
            continue;
        }
        
        let name = process.name();
        for target in targets {
            if name.contains(target) {
                println!("   Killing {} (PID: {})", name, process.pid());
                process.kill();
            }
        }
    }
}
