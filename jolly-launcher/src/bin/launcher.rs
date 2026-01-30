use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;
use sysinfo::{ProcessRefreshKind, RefreshKind, System};

fn main() -> Result<()> {
    println!("🚀 Starting JollyPad Launcher...");

    // 1. Setup initrc
    setup_initrc()?;

    // 2. Kill potential conflicting processes
    println!("🧹 Cleaning up old processes...");
    cleanup_processes();
    
    // 3. Wait a moment for cleanup
    thread::sleep(Duration::from_secs(2));

    // 4. Start Catacomb
    println!("👻 Starting catacomb...");
    start_catacomb()?;

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

fn setup_initrc() -> Result<()> {
    let config_dir = dirs::config_dir()
        .context("Could not find config directory")?
        .join("catacomb");
    
    if !config_dir.exists() {
        fs::create_dir_all(&config_dir)?;
    }

    let initrc_path = config_dir.join("initrc");
    
    // We want to use our compiled jolly-startup binary as the initrc
    // Assuming we are running from the workspace root
    let cwd = env::current_dir()?;
    let startup_binary = cwd.join("target/debug/jolly-startup");
    
    if !startup_binary.exists() {
        println!("⚠️  Warning: Startup binary not found at {:?}", startup_binary);
        println!("   Please run 'cargo build' first!");
    }

    if initrc_path.exists() {
        fs::remove_file(&initrc_path)?;
    }

    #[cfg(unix)]
    std::os::unix::fs::symlink(&startup_binary, &initrc_path)?;
    #[cfg(not(unix))]
    fs::copy(&startup_binary, &initrc_path)?;

    println!("🔗 Linked initrc to {:?}", startup_binary);
    Ok(())
}

fn start_catacomb() -> Result<()> {
    // Add target/debug directories to PATH so catacomb can find helpers (like jolly-nav, etc if needed)
    let cwd = env::current_dir()?;
    let mut new_path = env::var("PATH").unwrap_or_default();
    
    // Add all potential binary locations
    let bin_dirs = vec![
        cwd.join("target/debug"),
        cwd.join("catacomb/target/debug"),
        cwd.join("jolly/target/debug"),
    ];

    for dir in bin_dirs {
        if dir.exists() {
            new_path = format!("{}:{}", dir.display(), new_path);
        }
    }

    // We assume catacomb binary is in target/debug/catacomb
    // But since we are in a workspace, it should be in target/debug/catacomb
    
    let catacomb_bin = "catacomb"; // Assumes it's in the PATH we just constructed or user built it

    let status = Command::new(catacomb_bin)
        .env("PATH", new_path)
        .env("RUST_LOG", "info") // Set default log level
        .status()
        .context("Failed to start catacomb")?;

    if !status.success() {
        anyhow::bail!("Catacomb exited with error");
    }
    Ok(())
}
