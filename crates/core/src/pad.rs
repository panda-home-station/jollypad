use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{HashMap, hash_map::Entry};
use std::fs;
use std::path::{Path, PathBuf};
use dirs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppItem {
    pub name: String,
    pub icon: String, // icon name or absolute path
    pub exec: String,
    pub app_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ImageType {
    SizedBitmap(u32),
    Bitmap,
    Scalable,
    Symbolic,
}

impl Ord for ImageType {
    fn cmp(&self, other: &Self) -> Ordering {
        if self == other { return Ordering::Equal; }
        match (self, other) {
            (Self::Scalable, _) => Ordering::Greater,
            (_, Self::Scalable) => Ordering::Less,
            (Self::SizedBitmap(size), Self::SizedBitmap(other_size)) => size.cmp(other_size),
            (Self::SizedBitmap(_), _) => Ordering::Greater,
            (_, Self::SizedBitmap(_)) => Ordering::Less,
            _ => Ordering::Equal,
        }
    }
}
impl PartialOrd for ImageType {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}

#[derive(Debug)]
pub struct IconLoader {
    icons: HashMap<String, HashMap<ImageType, PathBuf>>,
}

impl IconLoader {
    pub fn new() -> Self {
        let mut icons: HashMap<String, HashMap<ImageType, PathBuf>> = HashMap::new();

        // Local assets fallback (assets/icons)
        // Check relative to CWD and relative to executable location
        let mut local_search_paths: Vec<PathBuf> = Vec::new();
        if let Ok(cwd) = std::env::current_dir() {
            // Check current dir
            local_search_paths.push(cwd.join("assets/icons"));
        }
        
        for dir in local_search_paths {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let file = entry.path();
                    if !file.is_file() { continue; }
                    let fname = match entry.file_name().into_string() { Ok(s)=>s, Err(_)=>continue };
                    let (name, img_type) = match fname.rsplit_once('.') {
                        Some((n,"svg")) => (n.to_string(), ImageType::Scalable),
                        Some((n,"png")) => (n.to_string(), ImageType::Bitmap),
                        _ => continue,
                    };
                    icons.entry(name).or_default().entry(img_type).or_insert(file);
                }
            }
        }

        // Known icon bases including Flatpak exports
        let mut bases: Vec<PathBuf> = Vec::new();
        if let Some(home) = dirs::home_dir() {
            bases.push(home.join(".local/share/flatpak/exports/share/icons"));
        }
        bases.push(PathBuf::from("/var/lib/flatpak/exports/share/icons"));
        bases.push(PathBuf::from("/usr/share/icons"));

        // Scan hicolor theme sizes and categories
        let sizes = ["512x512","256x256","128x128","64x64","48x48","scalable","symbolic"];
        let categories = ["apps","places"];
        let extensions = ["png","svg","xpm"];

        for base in bases {
            for size in sizes {
                for cat in categories {
                    let dir = base.join("hicolor").join(size).join(cat);
                    if let Ok(entries) = fs::read_dir(&dir) {
                        for entry in entries.flatten() {
                            let file = entry.path();
                            if !file.is_file() { continue; }
                            let fname = match entry.file_name().into_string() { Ok(s)=>s, Err(_)=>continue };
                            // name.ext
                            let (name, img_type) = match fname.rsplit_once('.') {
                                Some((name, ext)) if extensions.contains(&ext) => {
                                    let img_type = match size {
                                        "scalable" => ImageType::Scalable,
                                        "symbolic" => ImageType::Symbolic,
                                        _ => {
                                            if let Some((w,h)) = size.split_once('x') {
                                                if let (Ok(w), Ok(h)) = (w.parse::<u32>(), h.parse::<u32>()) {
                                                    if w==h { ImageType::SizedBitmap(w) } else { ImageType::Bitmap }
                                                } else { ImageType::Bitmap }
                                            } else { ImageType::Bitmap }
                                        }
                                    };
                                    (name.to_string(), img_type)
                                },
                                _ => continue,
                            };
                            // strip -symbolic suffix for mapping consistency
                            let key_name = if matches!(img_type, ImageType::Symbolic) {
                                name.strip_suffix("-symbolic").unwrap_or(&name).to_string()
                            } else { name };

                            let bucket = match icons.entry(key_name) {
                                Entry::Occupied(e) => e.into_mut(),
                                Entry::Vacant(e) => e.insert(HashMap::new()),
                            };
                            bucket.entry(img_type).or_insert(file);
                        }
                    }
                }
            }
        }

        // Pixmaps fallback
        if let Ok(entries) = fs::read_dir("/usr/share/pixmaps") {
            for entry in entries.flatten() {
                let file = entry.path();
                if !file.is_file() { continue; }
                let fname = match entry.file_name().into_string() { Ok(s)=>s, Err(_)=>continue };
                let (name, img_type) = match fname.rsplit_once('.') {
                    Some((n,"svg")) => (n.to_string(), ImageType::Scalable),
                    Some((n,"png")) => (n.to_string(), ImageType::Bitmap),
                    _ => continue,
                };
                icons.entry(name).or_default().entry(img_type).or_insert(file);
            }
        }

        Self { icons }
    }

    pub fn icon_path<'a>(&'a self, icon: &str) -> Option<&'a Path> {
        if icon.is_empty() { return None; }
        let map = self.icons.get(icon)?;
        // pick best
        let mut best: Option<(&ImageType,&PathBuf)> = None;
        for (t,p) in map {
            match best {
                None => best = Some((t,p)),
                Some((bt,_)) => if t > bt { best = Some((t,p)) },
            }
        }
        best.map(|(_,p)| p.as_path())
    }
}

fn parse_ini_app(content: &str, default_app_id: &str) -> Option<AppItem> {
    println!("DEBUG: parsing ini for {}", default_app_id);
    let mut in_app = false;
    let mut in_game = false;
    let mut name = String::new();
    let mut icon = String::new();
    let mut exec = String::new();
    let mut app_id = String::new();
    
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            if t.eq_ignore_ascii_case("[App]") {
                in_app = true;
                in_game = false;
                println!("DEBUG: Found [App] section");
            } else if t.eq_ignore_ascii_case("[Game]") {
                in_app = false;
                in_game = true;
                println!("DEBUG: Found [Game] section");
            } else {
                in_app = false;
                in_game = false;
            }
            continue;
        }
        
        if (!in_app && !in_game) || t.is_empty() { continue; }
        
        if let Some((k,v)) = t.split_once('=') {
            let key = k.trim();
            let val = v.trim();
            match key {
                "Name" => name = val.to_string(),
                "Icon" => icon = val.to_string(),
                "Exec" => {
                    if in_app {
                        exec = val.to_string();
                    }
                    // For [Game], we ignore the Exec line here as it points to the EXE
                    // We will construct the launcher command later
                },
                "AppId" => app_id = val.to_string(),
                _ => {}
            }
        }
    }

    let final_app_id = if app_id.is_empty() {
        default_app_id.to_string()
    } else {
        app_id
    };

    if in_game {
        // If it's a game, we use the game-launcher wrapper
        // exec = format!("game-launcher {} run", final_app_id); 
        // Wait, game-launcher takes [app_id] [command] currently.
        // If we change game-launcher to just take app_id, we can do:
        exec = format!("game-launcher {}", final_app_id);
    }

    if !name.is_empty() && !exec.is_empty() {
        Some(AppItem { name, icon, exec, app_id: final_app_id })
    } else {
        None 
    }
}

pub fn get_default_items(_icon_loader: &IconLoader) -> Vec<AppItem> {
    let mut apps = Vec::new();
    if let Some(home) = dirs::home_dir() {
        let app_dir = home.join(".jolly").join("app");
        if let Ok(entries) = fs::read_dir(app_dir) {
            for e in entries.flatten() {
                let path = e.path();
                if path.extension().map_or(false, |x| x == "ini") {
                    let default_app_id = path.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_string();
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Some(app) = parse_ini_app(&content, &default_app_id) {
                            apps.push(app);
                        }
                    }
                }
            }
        }
    }
    apps.sort_by(|a,b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}
