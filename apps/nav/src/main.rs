use std::rc::Rc;
use std::cell::RefCell;
use slint::VecModel;
use slint::ComponentHandle;
use slint::Image;
use std::thread;
use std::time::{Duration, Instant};

use jollypad_core::shell;
use jollypad_core::clients;
// use jollypad_ui::{NavOverlay, PadItem};

use jollypad_core::CatacombClient;

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    println!("DEBUG: jolly-nav starting...");
    let ui = NavOverlay::new()?;

    // Register system role for Nav overlay (optional)
    CatacombClient::set_system_role("nav", "JollyPad-Overlay");

    // Navbar Items
    let nav_model: Rc<VecModel<PadItem>> = Rc::new(VecModel::default());
    let nav_items_list = vec![
        ("主页", "home"),
        ("概览", "view-app-grid"),
        ("设置", "cog-outline"),
        ("手柄", "controller"),
        ("用户", "account"),
        ("关机", "power"),
    ];
    
    // User info for avatar
    let (_, user_avatar, _) = get_user_info();
    
    for (name, icon_name) in nav_items_list {
        let mut icon = load_icon(icon_name).unwrap_or_default();
        let mut has_icon = load_icon(icon_name).is_some();
        
        // Fallback for overview icon
        if name == "概览" && !has_icon {
             icon = load_icon("dock-window").unwrap_or_default();
             has_icon = load_icon("dock-window").is_some();
        }

        if name == "用户" && user_avatar.is_some() {
             icon = user_avatar.clone().unwrap();
             has_icon = true;
        }
        
        nav_model.push(PadItem {
            name: name.into(),
            icon: icon,
            exec: "".into(),
            app_id: "".into(),
            has_icon: has_icon,
        });
    }
    ui.set_nav_items(nav_model.into());
    
    // Window Items
    let windows_model: Rc<VecModel<PadItem>> = Rc::new(VecModel::default());
    ui.set_window_items(windows_model.clone().into());

    // Power Items
    let power_model: Rc<VecModel<PadItem>> = Rc::new(VecModel::default());
    let power_items_list = vec![
        ("关机", "power", "poweroff"),
        ("重启", "restart", "reboot"),
        ("挂起", "power-sleep", "systemctl suspend"),
        ("注销", "logout", "wlogout"), 
        ("锁定", "monitor-lock", "loginctl lock-session"),
    ];
    for (name, icon_name, exec) in power_items_list {
        let icon = load_icon(icon_name).unwrap_or_else(|| load_icon("application-default-icon").unwrap_or_default());
        power_model.push(PadItem {
            name: name.into(),
            icon: icon,
            exec: exec.into(),
            app_id: "".into(),
            has_icon: true,
        });
    }
    ui.set_power_items(power_model.into());

    // Register SIGUSR1 handler for external toggle
    use std::sync::atomic::{AtomicBool, Ordering};
    static TOGGLE_REQUESTED: AtomicBool = AtomicBool::new(false);
    extern "C" fn handle_sigusr1(_: i32) {
        TOGGLE_REQUESTED.store(true, Ordering::SeqCst);
    }
    unsafe {
        libc::signal(libc::SIGUSR1, handle_sigusr1 as *const () as usize);
    }

    // State monitor to handle external toggle via signal
    {
        let ui_weak = ui.as_weak();
        thread::spawn(move || {
            let mut last_signal_time = Instant::now() - Duration::from_secs(10);
            loop {
                thread::sleep(Duration::from_millis(50));
                
                if TOGGLE_REQUESTED.swap(false, Ordering::SeqCst) {
                    println!("DEBUG: TOGGLE_REQUESTED received");
                    if last_signal_time.elapsed() < Duration::from_millis(1000) {
                        println!("DEBUG: Debouncing toggle request");
                        continue;
                    }
                    last_signal_time = Instant::now();

                    let ui_weak = ui_weak.clone();
                    
                    // Check if active
                    let active_info = CatacombClient::get_active_window();
                    println!("DEBUG: Active window info: {:?}", active_info);
                    let is_active = if let Some((title, _)) = active_info {
                        title == "JollyPad-Overlay"
                    } else {
                        false
                    };
                    println!("DEBUG: is_active determined as: {}", is_active);

                    let _ = slint::invoke_from_event_loop(move || {
                        let ui = match ui_weak.upgrade() {
                            Some(ui) => ui,
                            None => {
                                println!("DEBUG: UI upgrade failed");
                                return;
                            },
                        };

                        if is_active {
                            println!("DEBUG: Closing overlay");
                            // Closing: Fade out -> Toggle -> Reset
                            ui.set_ready(false);
                            let ui_weak = ui_weak.clone();
                            thread::spawn(move || {
                                thread::sleep(Duration::from_millis(100));
                                jollypad_core::CatacombClient::role_action("overlay", "back", None);
                                // Reset while hidden so next open is clean
                                thread::sleep(Duration::from_millis(100));
                                let _ = slint::invoke_from_event_loop(move || {
                                    if let Some(ui) = ui_weak.upgrade() {
                                        ui.invoke_reset();
                                        ui.set_ready(true); 
                                    }
                                });
                            });
                        } else {
                            println!("DEBUG: Opening overlay");
                            // Opening: Reset -> Toggle -> Fade In
                            // Note: Previous close should have left it reset and ready=true, 
                            // but we double check to be sure.
                            ui.invoke_soft_reset(); 
                            // We set ready=false momentarily if we want to force fade in, 
                            // but if it's already hidden, ready=true is fine as it will just appear.
                            // If we want a fade in animation on open, we should set ready=false, toggle, then ready=true.
                            
                            // Let's try explicit fade in for smoothness
                            ui.set_ready(false); 
                            println!("DEBUG: UI ready set to false");
                            
                            let ui_weak = ui_weak.clone();
                            thread::spawn(move || {
                                println!("DEBUG: Sending toggle-window command");
                                let _ = std::process::Command::new("catacomb")
                                    .arg("msg")
                                    .arg("toggle-window")
                                    .arg("JollyPad-Overlay")
                                    .spawn();
                                
                                let start = Instant::now();
                                let mut focused = false;
                                for _ in 0..20 {
                                    thread::sleep(Duration::from_millis(20));
                                    if let Some((title, _)) = jollypad_core::CatacombClient::get_active_window() {
                                        if title == "JollyPad-Overlay" {
                                            focused = true;
                                            break;
                                        }
                                    }
                                }
                                if !focused {
                                    thread::sleep(Duration::from_millis(40));
                                }
                                thread::sleep(Duration::from_millis(100));
                                let _ = slint::invoke_from_event_loop(move || {
                                    if let Some(ui) = ui_weak.upgrade() {
                                        println!("DEBUG: UI ready set to true (delay {:?})", start.elapsed());
                                        ui.set_ready(true);
                                    } else {
                                        println!("DEBUG: UI upgrade failed in thread");
                                    }
                                });
                            });
                        }
                    });
                }
            }
        });
    }

    // Actions
    let last_nav_action = Rc::new(RefCell::new(Instant::now()));
    ui.on_on_nav_action({
        let ui_weak = ui.as_weak();
        let windows_model = windows_model.clone();
        let last_action = last_nav_action.clone();
        move |index| {
            if last_action.borrow().elapsed() < Duration::from_millis(300) {
                return;
            }
            *last_action.borrow_mut() = Instant::now();

            // Helper to hide window with delay for reset animation
            let hide_window_delayed = {
                let ui_weak = ui_weak.clone();
                move || {
                    // Step 1: Fade out content
                    if let Some(ui) = ui_weak.upgrade() {
                        ui.set_ready(false);
                    }
                    
                    let ui_weak = ui_weak.clone();
                    thread::spawn(move || {
                        // Step 2: Wait for fade out (50ms duration in slint + 50ms buffer)
                        thread::sleep(Duration::from_millis(100));
                        
                        // Step 3: Hide window via compositor
                        // Use role_action "back" which should explicitly hide the overlay
                        jollypad_core::CatacombClient::role_action("overlay", "back", None);
                            
                        // Step 4: Wait for window to be hidden
                        thread::sleep(Duration::from_millis(100));
                        
                        // Step 5: Reset state and restore opacity for next time
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = ui_weak.upgrade() {
                                ui.invoke_reset();
                                ui.set_ready(true);
                            }
                        });
                    });
                }
            };

            match index {
                0 => { 
                    // Special handling for Home: Hide window first, then show desktop
                    if let Some(ui) = ui_weak.upgrade() {
                        ui.set_ready(false);
                    }
                    let ui_weak = ui_weak.clone();
                    thread::spawn(move || {
                        thread::sleep(Duration::from_millis(100));
                        jollypad_core::CatacombClient::role_action("nav", "toggle", None);
                            
                        thread::sleep(Duration::from_millis(100));
                        shell::show_desktop(); 
                        
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = ui_weak.upgrade() {
                                ui.invoke_reset();
                                ui.set_ready(true);
                            }
                        });
                    });
                }
                1 => {
                    // Overview - Fetch windows only. Layout expansion is handled by UI.
                    let clients = clients::get_clients();
                    let mut items = Vec::new();
                    
                    // Filter out system windows
                    let ignored_apps = ["jolly-home", "jolly-nav", "catacomb"];

                    for c in clients {
                        if ignored_apps.iter().any(|&app| c.class.contains(app)) {
                            continue;
                        }

                         let ws_label = if c.workspace.id < 0 {
                             "S".to_string()
                         } else {
                             c.workspace.id.to_string()
                         };

                         let name = format!("[{}] {} - {}", ws_label, c.class, c.title);
                         // Try to load icon
                         let icon_name = c.class.to_lowercase();
                         let icon = load_icon(&icon_name).unwrap_or_else(|| load_icon("application-default-icon").unwrap_or_default());
                         
                         items.push(PadItem {
                             name: name.into(),
                             icon,
                             exec: c.address.into(),
                             app_id: "".into(),
                             has_icon: true, 
                         });
                    }
                    windows_model.set_vec(items);
                }, 
                2 => { 
                    let mut cmd = "jolly-settings".to_string();
                    // Try to find jolly-settings in the same directory as the current executable
                    if let Ok(exe_path) = std::env::current_exe() {
                        if let Some(dir) = exe_path.parent() {
                            let local_settings = dir.join("jolly-settings");
                            if local_settings.exists() {
                                if let Some(path_str) = local_settings.to_str() {
                                    cmd = path_str.to_string();
                                }
                            }
                        }
                    }

                    let id = make_nav_card_id("jolly-settings");
                    
                    // Hide first to ensure we don't cover the new window
                    // and to avoid focus race conditions
                    if let Some(ui) = ui_weak.upgrade() {
                        ui.set_ready(false);
                    }
                    
                    let cmd_clone = cmd.clone();
                    let ui_weak_clone = ui_weak.clone();
                    thread::spawn(move || {
                        // Hide overlay immediately
                        jollypad_core::CatacombClient::role_action("overlay", "back", None);
                        
                        // Give compositor a moment to process hide
                        thread::sleep(Duration::from_millis(50));
                        
                        // Then launch settings
                        shell::dispatch_exec(&cmd_clone, Some(&id));
                        
                        // Reset UI state for next time
                        thread::sleep(Duration::from_millis(150));
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = ui_weak_clone.upgrade() {
                                ui.invoke_reset();
                                ui.set_ready(true);
                            }
                        });
                    });
                }
                3 => { 
                    let id = make_nav_card_id("antimicrox");
                    shell::dispatch_exec("antimicrox", Some(&id)); 
                    hide_window_delayed();
                }
                4 => {}, // User
                5 => {}, // Power - Handled by UI
                _ => {}
            }
        }
    });

    // Power Action Handler
    ui.on_on_power_action({
        let ui_weak = ui.as_weak();
        move |exec| {
             // Execute command
             let _ = std::process::Command::new("sh").arg("-c").arg(&exec).spawn();
             
             // Hide window
             if let Some(ui) = ui_weak.upgrade() {
                 ui.set_ready(false);
             }
             let ui_weak = ui_weak.clone();
             thread::spawn(move || {
                 thread::sleep(Duration::from_millis(100));
                 jollypad_core::CatacombClient::role_action("overlay", "back", None);
                 thread::sleep(Duration::from_millis(100));
                 let _ = slint::invoke_from_event_loop(move || {
                     if let Some(ui) = ui_weak.upgrade() {
                         ui.invoke_reset();
                         ui.set_ready(true);
                     }
                 });
             });
        }
    });

    let last_window_action = Rc::new(RefCell::new(Instant::now()));
    ui.on_on_window_action({
        let ui_weak = ui.as_weak();
        move |address| {
             if last_window_action.borrow().elapsed() < Duration::from_millis(300) {
                return;
             }
             *last_window_action.borrow_mut() = Instant::now();
    
             clients::focus_window(&address);
             
             // Reset and hide
              if let Some(ui) = ui_weak.upgrade() {
                 ui.set_ready(false);
              }
              let ui_weak = ui_weak.clone();
              thread::spawn(move || {
                 thread::sleep(Duration::from_millis(100));
                 jollypad_core::CatacombClient::role_action("overlay", "back", None);

                 thread::sleep(Duration::from_millis(100));
                 slint::invoke_from_event_loop(move || {
                     if let Some(ui) = ui_weak.upgrade() {
                         ui.invoke_reset();
                         ui.set_ready(true);
                     }
                 }).unwrap();
              });
        }
    });
    
    let last_close = Rc::new(RefCell::new(Instant::now()));
    ui.on_close_requested({
        let ui_weak = ui.as_weak();
        move || {
            if last_close.borrow().elapsed() < Duration::from_millis(500) {
                println!("Debounced close request");
                return;
            }
            *last_close.borrow_mut() = Instant::now();
    
            // Reset and hide
              if let Some(ui) = ui_weak.upgrade() {
                 ui.set_ready(false);
              }
              let ui_weak = ui_weak.clone();
              thread::spawn(move || {
                 thread::sleep(Duration::from_millis(100));
                 jollypad_core::CatacombClient::role_action("overlay", "back", None);

                 thread::sleep(Duration::from_millis(100));
                 slint::invoke_from_event_loop(move || {
                     if let Some(ui) = ui_weak.upgrade() {
                         ui.invoke_reset();
                         ui.set_ready(true);
                     }
                 }).unwrap();
              });
        }
    });
    
    ui.on_debug_log(|msg| {
        println!("SLINT DEBUG: '{}' (Bytes: {:?})", msg, msg.as_bytes());
    });

    // Focus handling
    // ui.window().request_focus(); // Not available in current Slint version

    ui.run()
}

// Reuse icon loading logic
fn load_icon(icon_name: &str) -> Option<Image> {
    use std::path::PathBuf;
    
    fn resolve_icon_path(icon_name: &str) -> Option<PathBuf> {
        if icon_name.is_empty() { return None; }
        if std::path::Path::new(icon_name).is_file() {
            return Some(PathBuf::from(icon_name));
        }
        
        let search_paths = vec![
            "/home/jolly/phs/jollypad/assets/icons",
            "/usr/share/icons/hicolor/512x512/apps",
            "/usr/share/icons/hicolor/256x256/apps",
            "/usr/share/icons/hicolor/128x128/apps",
            "/usr/share/icons/hicolor/64x64/apps",
            "/usr/share/icons/Adwaita/64x64/places",
            "/usr/share/icons/Adwaita/64x64/apps",
            "/usr/share/pixmaps",
        ];

        let extensions = ["png", "svg", "xpm"];

        for path in &search_paths {
            for ext in &extensions {
                let p = PathBuf::from(format!("{}/{}.{}", path, icon_name, ext));
                if p.is_file() {
                    return Some(p);
                }
            }
        }
        None
    }

    let path = resolve_icon_path(icon_name);
    if let Some(p) = path {
        if p.is_file() {
            if let Ok(img) = Image::load_from_path(&p) {
                return Some(img);
            }
        }
    }
    None
}

fn make_nav_card_id(exec: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    exec.hash(&mut h);
    format!("nav:{:016x}", h.finish())
}

fn get_user_info() -> (String, Option<Image>, String) {
    use std::env;
    use std::path::PathBuf;
    use std::fs;

    let user_name = env::var("USER").unwrap_or_else(|_| "User".to_string());
    let initial = user_name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "U".to_string());

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
