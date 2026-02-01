use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;
use sysinfo::{ProcessRefreshKind, RefreshKind, System};

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

    // 4. Start Catacomb
    println!("👻 Starting catacomb...");
    let mut catacomb_process = start_catacomb()?;

    // 5. Start Startup Script
    // Give catacomb a moment to initialize the socket
    thread::sleep(Duration::from_secs(1));
    println!("🚀 Starting startup script...");
    let mut startup_process = start_startup()?;

    // 6. Monitor Loop
    // We want to keep running as long as catacomb is alive.
    // We also want to reap startup process if it finishes.
    loop {
        // Check catacomb status
        match catacomb_process.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    anyhow::bail!("Catacomb exited with error: {}", status);
                } else {
                    println!("👻 Catacomb exited normally.");
                    break;
                }
            },
            Ok(None) => {
                // Catacomb is still running
            },
            Err(e) => anyhow::bail!("Error waiting for catacomb: {}", e),
        }

        // Check startup status (to prevent zombie)
        match startup_process.try_wait() {
            Ok(Some(_)) => {
                // Startup finished, that's fine.
            },
            Ok(None) => {
                // Startup still running
            },
            Err(_) => {
                // Ignore errors here
            },
        }
        
        thread::sleep(Duration::from_millis(500));
    }
    
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

fn start_catacomb() -> Result<std::process::Child> {
    // Add target/debug directories to PATH so catacomb can find helpers (like jolly-nav, etc if needed)
    let cwd = env::current_dir()?;
    let mut new_path = env::var("PATH").unwrap_or_default();
    
    // Add all potential binary locations
    // Prioritize workspace target/release then debug
    let bin_dirs = vec![
        cwd.join("target/release"),
        cwd.join("catacomb/target/release"),
        cwd.join("jolly/target/release"),
        cwd.join("target/debug"),
        cwd.join("catacomb/target/debug"),
        cwd.join("jolly/target/debug"),
    ];

    for dir in bin_dirs.iter().rev() {
        if dir.exists() {
            new_path = format!("{}:{}", dir.display(), new_path);
        }
    }

    // We assume catacomb binary is in target/debug/catacomb
    // But since we are in a workspace, it should be in target/debug/catacomb
    
    let catacomb_bin = "catacomb"; // Assumes it's in the PATH we just constructed or user built it

    let child = Command::new(catacomb_bin)
        .env("PATH", new_path)
        .env("RUST_LOG", "info") // Set default log level
        .spawn()
        .context("Failed to start catacomb")?;

    Ok(child)
}

fn start_startup() -> Result<std::process::Child> {
    // We want to use our compiled jolly-startup binary
    // Assuming we are running from the workspace root
    let cwd = env::current_dir()?;
    let mut startup_binary = cwd.join("target/debug/jolly-startup");
    
    if !startup_binary.exists() {
        let release_binary = cwd.join("target/release/jolly-startup");
        if release_binary.exists() {
            startup_binary = release_binary;
        }
    }
    
    if !startup_binary.exists() {
        println!("⚠️  Warning: Startup binary not found at {:?}", startup_binary);
        println!("   Please run 'cargo build' first!");
    }

    // We MUST set WAYLAND_DISPLAY so startup (and its children) know where to connect.
    // Catacomb typically defaults to wayland-0 if available.
    // If this fails, we might need a more robust way to discover the socket.
    let child = Command::new(startup_binary)
        .env("WAYLAND_DISPLAY", "wayland-0") 
        .spawn()
        .context("Failed to spawn jolly-startup")?;
        
    Ok(child)
}

