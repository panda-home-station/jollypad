use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Cursor};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tar::Archive;

use crate::CatacombClient;

#[derive(Deserialize, Debug)]
struct Asset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize, Debug)]
struct Release {
    tag_name: String,
    assets: Option<Vec<Asset>>,
}

#[derive(Debug)]
pub struct GameLaunchInfo {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub envs: HashMap<String, String>,
}

pub fn get_running_game() -> Option<String> {
    let clients = CatacombClient::get_clients();
    for client in clients {
        if !client.app_id.is_empty() {
             if let Ok(is_game) = is_game_app(&client.app_id) {
                 if is_game {
                     return Some(client.app_id);
                 }
             }
        }
    }
    None
}

pub fn is_game_app(app_id: &str) -> Result<bool> {
    let home = dirs::home_dir().context("Could not find home directory")?;
    let jolly_dir = home.join(".jolly");
    let ini_path = jolly_dir.join("app").join(format!("{}.ini", app_id));
    if !ini_path.exists() {
        return Ok(false);
    }
    let (_, is_game) = parse_app_ini(&ini_path)?;
    Ok(is_game)
}

pub fn prepare_game_launch(app_id: &str) -> Result<GameLaunchInfo> {
    // Setup paths
    let home = dirs::home_dir().context("Could not find home directory")?;
    let jolly_dir = home.join(".jolly");
    let tools_dir = jolly_dir.join("tools");
    let components_dir = jolly_dir.join("components");
    let runners_dir = jolly_dir.join("runners");
    
    fs::create_dir_all(&tools_dir).context("Failed to create tools directory")?;
    fs::create_dir_all(&components_dir).context("Failed to create components directory")?;
    fs::create_dir_all(&runners_dir).context("Failed to create runners directory")?;

    // Check and update dependencies
    if let Err(e) = check_and_update_umu(&tools_dir) {
        eprintln!("Warning: Failed to update UMU Launcher: {}", e);
    }
    if let Err(e) = check_and_update_components(&components_dir) {
        eprintln!("Warning: Failed to update components: {}", e);
    }
    if let Err(e) = check_and_update_proton(&runners_dir) {
        eprintln!("Warning: Failed to update Proton: {}", e);
    }

    // Load configuration
    let config = load_config(&jolly_dir)?;

    // Prepare environment variables
    let mut envs = HashMap::new();
    set_environment_vars(&mut envs, &config, &components_dir, &jolly_dir, app_id);

    // Get App Exec
    let ini_path = jolly_dir.join("app").join(format!("{}.ini", app_id));
    if !ini_path.exists() {
        return Err(anyhow::anyhow!("Configuration file not found: {:?}", ini_path));
    }

    let (exec_path, is_game) = parse_app_ini(&ini_path)?;
    
    if is_game {
        // Game mode: use umu-run
        let umu_run = tools_dir.join("umu-run");
        let args = vec![exec_path];
        
        Ok(GameLaunchInfo {
            program: umu_run,
            args,
            envs,
        })
    } else {
        // App mode: direct exec (via sh)
        let args = vec!["-c".to_string(), exec_path];
        Ok(GameLaunchInfo {
            program: PathBuf::from("sh"),
            args,
            envs,
        })
    }
}

fn parse_app_ini(path: &Path) -> Result<(String, bool)> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut exec = String::new();
    let mut is_game = false;

    for line in reader.lines() {
        let line = line?;
        let t = line.trim();
        if t.starts_with('[') {
            if t.eq_ignore_ascii_case("[Game]") {
                is_game = true;
            }
            continue;
        }
        
        if let Some((k,v)) = t.split_once('=') {
            let key = k.trim();
            let val = v.trim();
            if key == "Exec" {
                exec = val.to_string();
            }
        }
    }
    
    if exec.is_empty() {
        return Err(anyhow::anyhow!("No Exec line found in {:?}", path));
    }
    
    Ok((exec, is_game))
}


fn load_config(jolly_dir: &Path) -> Result<HashMap<String, String>> {
    let mut config = HashMap::new();
    
    // Default config values
    config.insert("discrete-gpu".to_string(), "False".to_string());
    config.insert("wayland-driver".to_string(), "False".to_string());
    config.insert("enable-hdr".to_string(), "False".to_string());
    config.insert("enable-wow64".to_string(), "False".to_string());
    config.insert("default-runner".to_string(), "Proton-GE Latest".to_string());

    let config_path = jolly_dir.join("config.ini");

    if config_path.exists() {
        let file = File::open(config_path)?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim().to_string();
                let value = value.trim().trim_matches('"').to_string();
                config.insert(key, value);
            }
        }
    }

    Ok(config)
}

fn set_environment_vars(envs: &mut HashMap<String, String>, config: &HashMap<String, String>, components_dir: &Path, jolly_dir: &Path, app_id: &str) {
    let eac_dir = components_dir.join("eac");
    let be_dir = components_dir.join("be");
    let runners_dir = jolly_dir.join("runners");

    envs.insert("PROTON_EAC_RUNTIME".to_string(), eac_dir.to_string_lossy().to_string());
    envs.insert("PROTON_BATTLEYE_RUNTIME".to_string(), be_dir.to_string_lossy().to_string());
    envs.insert("STEAM_COMPAT_TOOLS_PATHS".to_string(), runners_dir.to_string_lossy().to_string());

    if config.get("discrete-gpu").map(|v| v == "True").unwrap_or(false) {
        envs.insert("DRI_PRIME".to_string(), "1".to_string());
    }

    if config.get("wayland-driver").map(|v| v == "True").unwrap_or(false) {
        envs.insert("PROTON_ENABLE_WAYLAND".to_string(), "1".to_string());
        if config.get("enable-hdr").map(|v| v == "True").unwrap_or(false) {
            envs.insert("PROTON_ENABLE_HDR".to_string(), "1".to_string());
        }
    }

    if config.get("enable-wow64").map(|v| v == "True").unwrap_or(false) {
        envs.insert("PROTON_USE_WOW64".to_string(), "1".to_string());
    }

    // Audio fixes for Proton:
    // 1. Force higher latency to prevent buffer underruns/dropouts which cause HDMI resync
    envs.insert("PULSE_LATENCY_MSEC".to_string(), "60".to_string());
    // 2. Ensure we use PulseAudio backend (which pipes to PipeWire)
    envs.insert("SDL_AUDIODRIVER".to_string(), "pulseaudio".to_string());

    // 3. Force FSR disabled for Steam/Games to prevent scaling weirdness unless requested
    // This helps ensure games see the real resolution
    envs.insert("WINE_FULLSCREEN_FSR".to_string(), "0".to_string());
    
    // 4. Suppress pressure-vessel 32-bit warnings if possible (cosmetic but clean logs)
    // envs.insert("PRESSURE_VESSEL_VERBOSE".to_string(), "0".to_string());

    // Set WINEPREFIX to ~/.jolly/prefixes/<app_id>
    // This fixes the pending task to move WINEPREFIX to ~/.jolly/
    let prefix_dir = jolly_dir.join("prefixes").join(app_id);
    if let Err(e) = fs::create_dir_all(&prefix_dir) {
        eprintln!("Failed to create prefix directory: {}", e);
    }
    envs.insert("WINEPREFIX".to_string(), prefix_dir.to_string_lossy().to_string());
    
    // Set PROTONPATH if not already set
    if env::var("PROTONPATH").is_err() {
        let runner = config.get("default-runner").map(|s| s.as_str()).unwrap_or("Proton-GE Latest");
        let runner = convert_runner(runner);
        if !runner.is_empty() {
            envs.insert("PROTONPATH".to_string(), runner);
        }
    }
}

fn check_and_update_umu(tools_dir: &Path) -> Result<()> {
    let umu_run_path = tools_dir.join("umu-run");
    let version_file = tools_dir.join("umu-version.txt");

    let client = reqwest::blocking::Client::new();
    let releases_url = "https://api.github.com/repos/Faugus/umu-launcher/releases";
    
    // Get installed version
    let installed_version = if version_file.exists() {
        fs::read_to_string(&version_file).ok().map(|s| s.trim().to_string())
    } else {
        None
    };

    // Get latest version
    let resp = client.get(releases_url)
        .header("User-Agent", "jolly-game-launcher")
        .send()
        .context("Failed to fetch UMU releases")?;
    
    let releases: Vec<Release> = resp.json().context("Failed to parse UMU releases")?;
    if releases.is_empty() {
        return Ok(());
    }
    
    let latest_version = &releases[0].tag_name;

    if Some(latest_version.clone()) != installed_version || !umu_run_path.exists() {
        println!("Updating UMU-Launcher to {}...", latest_version);
        
        let url = format!("https://github.com/Faugus/umu-launcher/releases/download/{}/umu-run", latest_version);
        let resp = client.get(&url).send().context("Failed to download umu-run")?;
        
        if !resp.status().is_success() {
             return Err(anyhow::anyhow!("Download failed with status: {}", resp.status()));
        }
        
        let content = resp.bytes()?;
        fs::write(&umu_run_path, content)?;
        
        // chmod +x
        let mut perms = fs::metadata(&umu_run_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&umu_run_path, perms)?;
        
        fs::write(&version_file, latest_version)?;
        println!("UMU-Launcher updated.");
    } else {
        // println!("UMU-Launcher is up to date ({})", latest_version);
    }

    Ok(())
}

fn check_and_update_proton(runners_dir: &Path) -> Result<()> {
    let proton_dir = runners_dir.join("Proton-GE Latest");
    if proton_dir.exists() {
        let vdf_path = proton_dir.join("compatibilitytool.vdf");
        if !vdf_path.exists() {
            let vdf_content = r#""compatibilitytools"
{
  "compat_tools"
  {
    "Proton-GE Latest"
    {
      "install_path" "."
      "display_name" "Proton-GE Latest"
      "from_oslist"  "windows"
      "to_oslist"    "linux"
    }
  }
}"#;
            fs::write(vdf_path, vdf_content)?;
        }
        return Ok(());
    }

    let version_file = runners_dir.join("version.txt");
    let client = reqwest::blocking::Client::new();
    let latest_url = "https://api.github.com/repos/GloriousEggroll/proton-ge-custom/releases/latest";

    // Get installed version
    let installed_version = if version_file.exists() {
        fs::read_to_string(&version_file).ok().map(|s| s.trim().to_string())
    } else {
        None
    };

    // Get latest version
    let resp = client.get(latest_url)
        .header("User-Agent", "jolly-game-launcher")
        .send()
        .context("Failed to fetch Proton releases")?;
    
    let release: Release = resp.json().context("Failed to parse Proton release")?;
    let latest_version = &release.tag_name;

    if Some(latest_version.clone()) != installed_version {
        println!("Updating Proton to {}...", latest_version);

        let assets = release.assets.ok_or(anyhow::anyhow!("No assets found in Proton release"))?;
        let asset = assets.iter()
            .find(|a| a.name.ends_with(".tar.gz") && !a.name.contains("sha512"))
            .ok_or(anyhow::anyhow!("No .tar.gz asset found"))?;

        println!("Downloading {}...", asset.name);
        let resp = client.get(&asset.browser_download_url).send().context("Failed to download Proton")?;
        
        if !resp.status().is_success() {
             return Err(anyhow::anyhow!("Download failed with status: {}", resp.status()));
        }

        let content = resp.bytes()?;
        let decoder = GzDecoder::new(Cursor::new(content));
        let mut archive = Archive::new(decoder);
        
        archive.unpack(runners_dir).context("Failed to extract Proton")?;

        let dest_path = runners_dir.join("Proton-GE Latest");

        if dest_path.exists() {
            fs::remove_dir_all(&dest_path)?;
        }
        
        // Find extracted directory
        let mut found = false;
        for entry in fs::read_dir(runners_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with("GE-Proton") && name != "Proton-GE Latest" {
                        fs::rename(path, &dest_path)?;
                        found = true;
                        break;
                    }
                }
            }
        }
        
        if !found {
             // If we can't find it, maybe it was extracted as "Proton-GE Latest" (unlikely) or something went wrong.
             // Or maybe the tarball structure is different.
             // But we just unpacked it.
             // Let's assume it worked if no error from unpack.
             // But we need to rename it for "Proton-GE Latest" config to work.
             // If not found, we can't proceed with VDF creation in the right place.
             return Err(anyhow::anyhow!("Could not find extracted Proton directory"));
        }

        let vdf_content = r#""compatibilitytools"
{
  "compat_tools"
  {
    "Proton-GE Latest"
    {
      "install_path" "."
      "display_name" "Proton-GE Latest"
      "from_oslist"  "windows"
      "to_oslist"    "linux"
    }
  }
}"#;

        fs::write(dest_path.join("compatibilitytool.vdf"), vdf_content)?;
        
        fs::write(&version_file, latest_version)?;
        println!("Proton updated.");
    }

    Ok(())
}

fn convert_runner(runner: &str) -> String {
    match runner {
        // "Proton-GE Latest" => "GE-Proton Latest (default)".to_string(),
        // "GE-Proton Latest (default)" => "Proton-GE Latest".to_string(),
        "UMU-Proton Latest" => "".to_string(),
        "" => "UMU-Proton Latest".to_string(),
        _ => runner.to_string(),
    }
}

fn check_and_update_components(components_dir: &Path) -> Result<()> {
    let version_file = components_dir.join("version.txt");
    let client = reqwest::blocking::Client::new();
    let latest_url = "https://api.github.com/repos/Faugus/components/releases/latest";

    // Get installed version
    let installed_version = if version_file.exists() {
        fs::read_to_string(&version_file).ok().map(|s| s.trim().to_string())
    } else {
        None
    };

    // Get latest version
    let resp = client.get(latest_url)
        .header("User-Agent", "jolly-game-launcher")
        .send()
        .context("Failed to fetch components release")?;
    
    let release: Release = resp.json().context("Failed to parse components release")?;
    let latest_version = &release.tag_name;

    if Some(latest_version.clone()) != installed_version {
        println!("Updating components to {}...", latest_version);

        let base_url = format!("https://github.com/Faugus/components/releases/download/{}", latest_version);
        let files = ["eac.tar.gz", "be.tar.gz"];

        for file in files {
            let url = format!("{}/{}", base_url, file);

            let resp = client.get(&url).send().context(format!("Failed to download {}", file))?;
             if !resp.status().is_success() {
                 eprintln!("Failed to download {}: {}", file, resp.status());
                 continue;
            }

            let content = resp.bytes()?;
            let decoder = GzDecoder::new(Cursor::new(content));
            let mut archive = Archive::new(decoder);
            
            archive.unpack(components_dir).context(format!("Failed to extract {}", file))?;
        }

        fs::write(&version_file, latest_version)?;
        println!("Components updated.");
    } else {
        // println!("Components are up to date ({})", latest_version);
    }

    Ok(())
}
