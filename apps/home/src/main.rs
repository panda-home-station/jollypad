use std::rc::Rc;
use slint::VecModel;
use slint::ComponentHandle;
use slint::Image;
use std::thread;
use std::time::Duration;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::fs;
use std::env;
use std::process::Command;

use jollypad_core::{shell, get_pad_items, CatacombClient, pad::IconLoader};
use jollypad_core::game_launcher::{get_running_game, is_game_app};
// use jollypad_ui::{MainWindow, PadItem};
use std::sync::{Arc, Mutex};

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    println!("DEBUG: Jolly Home Starting...");
    std::io::stdout().flush().unwrap();

    let active_window = Arc::new(Mutex::new(String::new()));
    let active_class = Arc::new(Mutex::new(String::new()));

    let active_window_clone = active_window.clone();
    let active_class_clone = active_class.clone();
    thread::spawn(move || {
        loop {
            if let Some((title, app_id)) = CatacombClient::get_active_window() {
                if !title.is_empty() || !app_id.is_empty() {
                    if let Ok(mut w) = active_window_clone.lock() {
                        if *w != title {
                            *w = title;
                        }
                    }
                    if let Ok(mut c) = active_class_clone.lock() {
                        if *c != app_id {
                            *c = app_id;
                        }
                    }
                } else {
                    // Empty strings -> Desktop / No Active Window
                    if let Ok(mut w) = active_window_clone.lock() {
                        if !w.is_empty() { w.clear(); }
                    }
                    if let Ok(mut c) = active_class_clone.lock() {
                        if !c.is_empty() { c.clear(); }
                    }
                }
            }
            thread::sleep(Duration::from_millis(100));
        }
    });

    let icon_loader = Arc::new(IconLoader::new());
    thread::spawn(move || {
        preload_runtime();
    });
    run_app(active_window, active_class, icon_loader)
}

fn launch_app_helper(exec: &str, name: &str, _app_id: &str, ui_weak: slint::Weak<MainWindow>) {
    fn make_card_id(exec: &str, name: &str) -> String {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        exec.hash(&mut h);
        name.hash(&mut h);
        format!("card:{:016x}", h.finish())
    }

    let ui_weak_local = ui_weak.clone();
    let start = std::time::Instant::now();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui_weak_local.upgrade() {
            ui.set_is_launching(true);
        }
    });
    
    let card_id = make_card_id(exec, name);
    shell::dispatch_exec(exec, Some(&card_id));
    
    let ui_weak_local = ui_weak.clone();
    thread::spawn(move || {
        loop {
            if let Some((_, active_app_id)) = CatacombClient::get_active_window() {
                if active_app_id != "jolly-home" && !active_app_id.is_empty() {
                    let elapsed = start.elapsed().as_millis();
                    println!("LAUNCH: active_window_changed app_id='{}' elapsed={}ms", active_app_id, elapsed);
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = ui_weak_local.upgrade() {
                            ui.set_is_launching(false);
                        }
                    });
                    break;
                }
            }
            thread::sleep(Duration::from_millis(50));
        }
    });
}

// --------------------------------------------------------
// Mode: Full Desktop (Monolithic)
// --------------------------------------------------------
fn run_app(_active_window: Arc<Mutex<String>>, _active_class: Arc<Mutex<String>>, icon_loader: Arc<IconLoader>) -> Result<(), slint::PlatformError> {
    // Load Data
    let pad_items_data = get_pad_items(&icon_loader);
    println!("DEBUG: Loaded {} pad items", pad_items_data.len());
    for item in &pad_items_data {
        println!("DEBUG: Item '{}', icon='{}'", item.name, item.icon);
    }
    std::io::stdout().flush().unwrap();

    let ui = MainWindow::new()?;

    // Register system role for Home
    CatacombClient::set_system_role("home", "^(JollyPad-Desktop|jolly-home)$");

    // User info
    let (user_name, user_avatar, user_initial) = get_user_info();

    let pad_model: Rc<VecModel<PadItem>> = Rc::new(VecModel::default());
    for item in &pad_items_data {
        let icon_opt = load_icon(&icon_loader, &item.icon);
        pad_model.push(PadItem {
            name: item.name.clone().into(),
            icon: icon_opt.clone().unwrap_or_default(),
            exec: item.exec.clone().into(),
            app_id: item.app_id.clone().into(),
            has_icon: icon_opt.is_some(),
        });
    }

    ui.set_pad_items(pad_model.into());
    ui.set_user_name(user_name.into());
    ui.set_has_user_avatar(user_avatar.is_some());
    if let Some(img) = user_avatar {
        ui.set_user_avatar(img);
    }
    ui.set_user_initial(user_initial.into());
    ui.set_controller_count(0);
    if let Some(img) = load_controller_icon(&icon_loader) {
        ui.set_controller_icon(img);
        ui.set_has_controller_icon(true);
    } else {
        ui.set_has_controller_icon(false);
    }

    let ui_weak = ui.as_weak();
    
    struct PendingLaunch {
        exec: String,
        name: String,
        app_id: String,
        running_game_id: String,
    }
    let pending_launch = Rc::new(std::cell::RefCell::new(None::<PendingLaunch>));

    let pending_launch_on_pad = pending_launch.clone();
    let ui_weak_on_pad = ui_weak.clone();
    ui.on_on_pad_action(move |exec_cmd: slint::SharedString, name: slint::SharedString, app_id: slint::SharedString| {
        if name.as_str() == "Add Card" {
            println!("TODO: Open add card dialog");
        } else {
            let target_app_id = app_id.as_str();
            let clients = CatacombClient::get_clients();
            
            // 1. Check if THIS app is already running
            if let Some(client) = clients.iter().find(|c| {
                let app = c.app_id.to_lowercase();
                let target = target_app_id.to_lowercase();
                if app.is_empty() { return false; }
                app == target || app.contains(&target) || target.contains(&app)
            }) {
                CatacombClient::focus_window(&client.app_id);
                return;
            }

            // 2. Check if it's a game and another game is running
            let is_game = is_game_app(target_app_id).unwrap_or(false);
            if is_game {
                if let Some(running_id) = get_running_game() {
                    *pending_launch_on_pad.borrow_mut() = Some(PendingLaunch {
                        exec: exec_cmd.to_string(),
                        name: name.to_string(),
                        app_id: app_id.to_string(),
                        running_game_id: running_id.clone(),
                    });
                    
                    if let Some(ui) = ui_weak_on_pad.upgrade() {
                        ui.set_confirm_message(format!("{} is currently running. Do you want to close it and start {}?", running_id, name).into());
                        ui.set_is_confirming(true);
                    }
                    return;
                }
            }

            // 3. Normal Launch
            launch_app_helper(exec_cmd.as_str(), name.as_str(), app_id.as_str(), ui_weak_on_pad.clone());
        }
    });

    let pending_launch_confirm = pending_launch.clone();
    let ui_weak_confirm = ui_weak.clone();
    ui.on_on_confirm(move || {
        if let Some(launch) = pending_launch_confirm.borrow_mut().take() {
            CatacombClient::close_window(&launch.running_game_id);
            launch_app_helper(&launch.exec, &launch.name, &launch.app_id, ui_weak_confirm.clone());
        }
        if let Some(ui) = ui_weak_confirm.upgrade() {
            ui.set_is_confirming(false);
        }
    });

    let pending_launch_cancel = pending_launch.clone();
    let ui_weak_cancel = ui_weak.clone();
    ui.on_on_cancel(move || {
        pending_launch_cancel.borrow_mut().take();
        if let Some(ui) = ui_weak_cancel.upgrade() {
            ui.set_is_confirming(false);
        }
    });

    let ui_weak2 = ui.as_weak();
    ui.on_on_island_action(move |exec_cmd: slint::SharedString| {
        let cmd = exec_cmd.as_str();
        let clients = CatacombClient::get_clients();
        if let Some(client) = clients.iter().find(|c| c.app_id == cmd) {
            CatacombClient::focus_window(&client.app_id);
        } else {
            let ui_weak_local = ui_weak2.clone();
            let start = std::time::Instant::now();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_weak_local.upgrade() {
                    ui.set_is_launching(true);
                }
            });
            shell::dispatch_exec(cmd, Some(cmd));
            let ui_weak_local = ui_weak2.clone();
            thread::spawn(move || {
                loop {
                    if let Some((_, app_id)) = CatacombClient::get_active_window() {
                        if app_id != "jolly-home" && !app_id.is_empty() {
                            let elapsed = start.elapsed().as_millis();
                            println!("LAUNCH: active_window_changed app_id='{}' elapsed={}ms", app_id, elapsed);
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(ui) = ui_weak_local.upgrade() {
                                    ui.set_is_launching(false);
                                }
                            });
                            break;
                        }
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            });
        }
    });
    
    // 轮询当前打开应用以更新“灵动岛”的应用图标
    let ui_weak_for_island = ui.as_weak();
    let icon_loader_for_island = icon_loader.clone();
    thread::spawn(move || {
        let mut last_ids: Vec<String> = Vec::new();
        
        loop {
            let clients = CatacombClient::get_clients();
            
            // Check if changed (simple comparison)
            // Note: ClientInfo needs to implement PartialEq, which it does in catacomb_ipc
            // However, we need to verify if the order matters or if we should sort.
            // catacomb usually returns clients in z-order or list order. 
            // If z-order changes, clients list changes. Dock order should probably be stable?
            // For now, let's update if anything changes.
            
            // Compare by app_id set for island windows
            // 过滤系统窗口与 JollyPad 自身窗口
            let ignored_apps = ["jolly-home", "jolly-nav", "catacomb"];
            let filtered: Vec<jollypad_core::ClientInfo> = clients.into_iter()
                .filter(|c| {
                    let id = c.app_id.to_lowercase();
                    let title = c.title.to_lowercase();
                    if id.is_empty() {
                        return false;
                    }
                    if ignored_apps.iter().any(|app| id.contains(app)) {
                        return false;
                    }
                    if title.contains("jollypad-desktop") || title.contains("jollypad-overlay") || title.contains("jollypad-launcher") {
                        return false;
                    }
                    true
                })
                .collect();

            // 使用过滤后的 ID 集合进行对比
            let mut current_ids: Vec<String> = filtered.iter().map(|c| c.app_id.clone()).collect();
            current_ids.sort();
            let mut prev_ids = last_ids.clone();
            prev_ids.sort();

            if current_ids != prev_ids {
                last_ids = current_ids.clone();
                // 过滤系统窗口
                let icon_loader = icon_loader_for_island.clone();
                let ui_weak = ui_weak_for_island.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_weak.upgrade() {
                        let mut new_models = Vec::new();
                        for client in filtered {
                        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
                            let icon_opt = load_icon(&icon_loader, &client.app_id);
                            if !seen.insert(client.app_id.clone()) {
                                continue;
                            }
                            new_models.push(PadItem {
                                name: client.title.into(),
                                icon: icon_opt.clone().unwrap_or_default(),
                                exec: client.app_id.clone().into(),
                                app_id: client.app_id.into(),
                                has_icon: icon_opt.is_some(),
                            });
                        }
                        let vec_model = Rc::new(VecModel::from(new_models));
                        ui.set_island_windows(vec_model.into());
                    }
                });
            }
            
            thread::sleep(Duration::from_millis(500));
        }
    });


    // 轮询手柄连接状态以更新“灵动岛”手柄数量
    let ui_weak_for_pad = ui.as_weak();
    let icon_loader_for_pad = icon_loader.clone();
    thread::spawn(move || {
        loop {
            let count = count_gamepads();
            let ui_weak = ui_weak_for_pad.clone();
            let icon_loader = icon_loader_for_pad.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_weak.upgrade() {
                    ui.set_controller_count(count as i32);
                    // 构建重复的手柄图标
                    let icon_opt = load_controller_icon(&icon_loader);
                    let mut icons: Vec<PadItem> = Vec::new();
                    for _ in 0..count {
                        icons.push(PadItem {
                            name: "".into(),
                            icon: icon_opt.clone().unwrap_or_default(),
                            exec: "".into(),
                            app_id: "".into(),
                            has_icon: icon_opt.is_some(),
                        });
                    }
                    let vec_model = Rc::new(VecModel::from(icons));
                    ui.set_controller_icons(vec_model.into());
                }
            });
            thread::sleep(Duration::from_millis(800));
        }
    });

    ui.run()
}

fn get_user_info() -> (String, Option<Image>, String) {
    use std::env;
    use std::path::PathBuf;
    use std::fs;

    let user_name = env::var("USER").unwrap_or_else(|_| "User".to_string());
    let initial = user_name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "U".to_string());

    // Candidate avatar paths
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(home) = env::var("HOME") {
        candidates.push(PathBuf::from(format!("{}/.face", home)));
    }
    candidates.push(PathBuf::from(format!("/var/lib/AccountsService/icons/{}", user_name)));

    for p in candidates {
        if fs::metadata(&p).is_ok() {
            if let Ok(img) = Image::load_from_path(&p) {
                return (user_name, Some(img), initial);
            }
        }
    }

    (user_name, None, initial)
}


fn load_icon(loader: &IconLoader, icon_name: &str) -> Option<Image> {
    if icon_name.is_empty() { return None; }

    // Handle tilde expansion
    let expanded_name = if icon_name.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            format!("{}/{}", home, &icon_name[2..])
        } else {
            icon_name.to_string()
        }
    } else {
        icon_name.to_string()
    };

    println!("DEBUG: load_icon input='{}' expanded='{}'", icon_name, expanded_name);
            std::io::stdout().flush().unwrap();

            // 1) Try absolute path directly
    if Path::new(&expanded_name).is_absolute() {
        let p = PathBuf::from(&expanded_name);
        if p.is_file() {
                    println!("DEBUG: Found absolute file {:?}", p);
                    match Image::load_from_path(&p) {
                        Ok(img) => return Some(img),
                        Err(e) => println!("DEBUG: Failed to load image from {:?}: {}", p, e),
                    }
                } else {
                     println!("DEBUG: Absolute path is not a file: {:?}", p);
                }
            } else {
                println!("DEBUG: Path is not absolute: {}", expanded_name);
            }
            std::io::stdout().flush().unwrap();

            // 2) Try IconLoader best candidate
    if let Some(p) = loader.icon_path(icon_name) {
        let p = p.to_path_buf();
        if p.is_file() {
            if let Ok(img) = Image::load_from_path(&p) {
                return Some(img);
            }
        }
    }
    // 3) Fallback: prefer PNG/XPM in known dirs
    if let Some(p) = resolve_icon_path(icon_name) {
        if p.is_file() {
            if let Ok(img) = Image::load_from_path(&p) {
                return Some(img);
            }
        }
    }
    None
}

fn load_controller_icon(loader: &IconLoader) -> Option<Image> {
    let candidates = [
        "controller",
        "gamepad",
        "input-gaming",
        "input-gamepad",
        "applications-games",
    ];
    for name in candidates {
        if let Some(img) = load_icon(loader, name) {
            return Some(img);
        }
    }
    None
}

fn preload_runtime() {
    let xdg_config_home = env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
        let home = env::var("HOME").unwrap_or_default();
        format!("{}/.config", home)
    });
    let xdg_data_home = env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
        let home = env::var("HOME").unwrap_or_default();
        format!("{}/.local/share", home)
    });
    let cfg_path = format!("{}/faugus-launcher/config.ini", xdg_config_home);
    let mut default_prefix = format!("{}/Faugus", env::var("HOME").unwrap_or_default());
    let mut default_runner = String::from("Proton-GE Latest");
    if let Ok(content) = fs::read_to_string(&cfg_path) {
        for line in content.lines() {
            if let Some(v) = line.strip_prefix("default-prefix=") {
                default_prefix = v.trim().trim_matches('"').to_string();
            }
            if let Some(v) = line.strip_prefix("default-runner=") {
                default_runner = v.trim().trim_matches('"').to_string();
            }
        }
    }
    let mut proton_flag = String::new();
    let lr = default_runner.to_lowercase();
    if lr.contains("ge-proton") {
        proton_flag = String::from("--ge");
    } else if lr.contains("proton-em") {
        proton_flag = String::from("--em");
    }
    let umu_run = format!("{}/faugus-launcher/umu-run", xdg_data_home);
    let mut parts: Vec<String> = Vec::new();
    parts.push(String::from("python3 -m faugus.components"));
    if !proton_flag.is_empty() {
        parts.push(format!("python3 -m faugus.proton_downloader {}", proton_flag));
    }
    let mut envs: Vec<String> = Vec::new();
    let wp = format!("{}/default", default_prefix);
    envs.push(format!("WINEPREFIX={}", shell_quote(&wp)));
    if lr.contains("linux-native") {
    } else if lr.contains("cachyos") {
        envs.push(String::from("PROTONPATH=/usr/share/steam/compatibilitytools.d/proton-cachyos-slr/"));
    } else {
        envs.push(format!("PROTONPATH='{}'", default_runner));
    }
    let env_cmd = envs.join(" ");
    if env_cmd.is_empty() {
        parts.push(format!("'{}' wineboot -u", umu_run));
    } else {
        parts.push(format!("{} '{}' wineboot -u", env_cmd, umu_run));
    }
    let cmd = parts.join("; ");
    let _ = Command::new("sh").arg("-c").arg(cmd).spawn();
}

fn shell_quote(s: &str) -> String {
    if s.contains('\'') || s.contains(' ') {
        format!("'{}'", s.replace('\'', "'\"'\"'"))
    } else {
        s.to_string()
    }
}

fn resolve_icon_path(icon_name: &str) -> Option<std::path::PathBuf> {
    if icon_name.is_empty() { return None; }
    let p = std::path::PathBuf::from(icon_name);
    if p.is_absolute() && p.is_file() {
        return Some(p);
    }
    
    // Simple lookup in common dirs
    let dirs = [
        "/home/jolly/phs/jollypad/assets/icons",
        "/usr/share/pixmaps",
        "/usr/share/icons/hicolor/512x512/apps",
        "/usr/share/icons/hicolor/256x256/apps",
        "/usr/share/icons/hicolor/128x128/apps",
        "/usr/share/icons/hicolor/48x48/apps",
        "/usr/share/icons/hicolor/scalable/apps",
        "/usr/share/icons/Adwaita/48x48/apps",
        "/usr/share/icons/Adwaita/scalable/apps",
    ];
    
    let extensions = ["png", "svg", "xpm"];
    
    for dir in dirs {
        for ext in extensions {
            let candidate = std::path::Path::new(dir).join(format!("{}.{}", icon_name, ext));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        // Try without extension
        let candidate = std::path::Path::new(dir).join(icon_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn count_gamepads() -> usize {
    use std::fs;
    // 优先使用 js* 设备作为“手柄”计数，避免与 event-joystick 同时计数造成重复
    if let Ok(entries) = fs::read_dir("/dev/input") {
        let mut js_count = 0usize;
        for e in entries.flatten() {
            if let Ok(name) = e.file_name().into_string() {
                if name.to_lowercase().starts_with("js") {
                    js_count += 1;
                }
            }
        }
        if js_count > 0 {
            return js_count;
        }
    }

    // 如果没有 js*，再根据 by-id 的 -joystick / -event-joystick 进行计数（去重）
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    if let Ok(entries) = fs::read_dir("/dev/input/by-id") {
        for e in entries.flatten() {
            let fname = e.file_name().to_string_lossy().to_lowercase();
            if fname.contains("-joystick") || fname.contains("event-joystick") {
                if let Ok(target) = fs::read_link(e.path()) {
                    if let Some(base) = target.file_name() {
                        seen.insert(base.to_string_lossy().into_owned());
                    } else {
                        seen.insert(target.to_string_lossy().into_owned());
                    }
                } else {
                    seen.insert(fname);
                }
            }
        }
    }
    seen.len()
}
