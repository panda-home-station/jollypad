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
    pub category: String,
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
    println!("DEBUG: parsing ini for {} (VERSION 2)", default_app_id);
    let mut in_app = false;
    let mut in_game = false;
    let mut is_game_type = false;
    let mut name = String::new();
    let mut icon = String::new();
    let mut exec = String::new();
    let mut category = String::new();
    let app_id = String::new();
    
    for line in content.lines() {
        let trimmed_line = line.trim().trim_matches('\u{FEFF}');
        println!("DEBUG: parsing line='{}' in_app={} in_game={}", trimmed_line, in_app, in_game);
        if trimmed_line.starts_with('[') && trimmed_line.ends_with(']') {
            let section = trimmed_line[1..trimmed_line.len()-1].trim();
            if section.eq_ignore_ascii_case("App") {
                in_app = true;
                in_game = false;
                println!("DEBUG: Found [App] section");
            } else if section.eq_ignore_ascii_case("Game") {
                in_app = false;
                in_game = true;
                is_game_type = true;
                println!("DEBUG: Found [Game] section");
            } else if section.eq_ignore_ascii_case("Env") {
                in_app = false;
                in_game = false;
                println!("DEBUG: Found [Env] section");
            } else {
                in_app = false;
                in_game = false;
            }
            continue;
        }
        
        if (!in_app && !in_game) || trimmed_line.is_empty() { continue; }
        
        if let Some((k,v)) = trimmed_line.split_once('=') {
            let key = k.trim().trim_matches('\u{FEFF}');
            let val = v.trim().trim_matches('\u{FEFF}');
            println!("DEBUG: Key='{}', Val='{}' (in_app={}, in_game={})", key, val, in_app, in_game);
            match key {
                "Name" => name = val.to_string(),
                "Icon" => icon = val.to_string(),
                "Exec" => exec = val.to_string(),
                "Category" => category = val.to_string(),
                "Type" => {
                    if val.eq_ignore_ascii_case("Game") {
                        is_game_type = true;
                    }
                }
                _ => {}
            }
        }
    }

    let final_app_id = if app_id.is_empty() {
        default_app_id.to_string()
    } else {
        app_id
    };

    if is_game_type {
        // exec = format!("game-launcher {} run", final_app_id);
        exec = format!("game-launcher {}", final_app_id);
        if category.is_empty() {
            category = "Game".to_string();
        }
    }
    
    if category.is_empty() {
        category = "App".to_string();
    }
    
    println!("DEBUG: Finished parsing '{}'. Name='{}', Exec='{}', is_game={}, category='{}'", final_app_id, name, exec, is_game_type, category);

    if !name.is_empty() && !exec.is_empty() {
        Some(AppItem { name, icon, exec, app_id: final_app_id, category })
    } else {
        println!("DEBUG: App rejected due to empty name or exec");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_loose_ini() {
        let content = r#"
        [App]
        Name=Test App
        Icon=test.png
        Exec=test_exec
        "#;
        let app = parse_ini_app(content, "test.app").unwrap();
        assert_eq!(app.name, "Test App");
        assert_eq!(app.icon, "test.png");
        assert_eq!(app.exec, "test_exec");
    }

    #[test]
    fn test_bom_header() {
        // Simulate BOM at start of [Game]
        let content = "\u{FEFF}[Game]\nName=Test Game\nType=Game\nIcon=test.png\nExec=test";
        let app = parse_ini_app(content, "com.test").unwrap();
        assert_eq!(app.name, "Test Game");
    }
}
