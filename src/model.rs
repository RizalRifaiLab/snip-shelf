//! History items and saving/loading them.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Text,
    Image,
    Video,
    File,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Item {
    pub id: u64,
    pub kind: Kind,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub path: Option<PathBuf>,
    /// Where it came from: "Copied", "Screenshot", "Snip", "Recording", "Copied file".
    pub source: String,
    pub created: u64,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default)]
    pub hash: u64,
}

impl Item {
    pub fn title(&self) -> String {
        match self.kind {
            Kind::Text => {
                let t = self.text.clone().unwrap_or_default();
                let first = t.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
                first.chars().take(80).collect()
            }
            _ => self
                .path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| self.source.clone()),
        }
    }

    pub fn matches(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let q = query.to_lowercase();
        if let Some(t) = &self.text {
            if t.to_lowercase().contains(&q) {
                return true;
            }
        }
        self.title().to_lowercase().contains(&q) || self.source.to_lowercase().contains(&q)
    }

    pub fn file_missing(&self) -> bool {
        match (&self.kind, &self.path) {
            (Kind::Text, _) => false,
            (_, Some(p)) => !p.exists(),
            _ => true,
        }
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn new_id() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1)
}

pub fn time_ago(created: u64) -> String {
    let now = now_secs();
    let d = now.saturating_sub(created);
    if d < 10 {
        "just now".into()
    } else if d < 60 {
        format!("{d} sec ago")
    } else if d < 3600 {
        format!("{} min ago", d / 60)
    } else if d < 86400 {
        format!("{} h ago", d / 3600)
    } else {
        format!("{} days ago", d / 86400)
    }
}

/// The folder where history, snips and thumbnails live.
pub fn data_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(std::env::temp_dir);
    let dir = base.join("Snip Shelf");
    let _ = std::fs::create_dir_all(dir.join("captures"));
    let _ = std::fs::create_dir_all(dir.join("thumbs"));
    dir
}

pub fn captures_dir() -> PathBuf {
    data_dir().join("captures")
}

pub fn thumbs_dir() -> PathBuf {
    data_dir().join("thumbs")
}

pub fn is_inside(path: &Path, dir: &Path) -> bool {
    path.starts_with(dir)
}

pub fn load_history() -> Vec<Item> {
    let file = data_dir().join("history.json");
    std::fs::read_to_string(file)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_history(items: &[Item]) {
    let dir = data_dir();
    let tmp = dir.join("history.json.tmp");
    if let Ok(s) = serde_json::to_string_pretty(items) {
        if std::fs::write(&tmp, s).is_ok() {
            let _ = std::fs::rename(tmp, dir.join("history.json"));
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub hide_after_copy: bool,
    pub auto_copy_new_captures: bool,
    pub max_items: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hide_after_copy: true,
            auto_copy_new_captures: false,
            max_items: 300,
        }
    }
}

pub fn load_settings() -> Settings {
    std::fs::read_to_string(data_dir().join("settings.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_settings(s: &Settings) {
    if let Ok(j) = serde_json::to_string_pretty(s) {
        let _ = std::fs::write(data_dir().join("settings.json"), j);
    }
}

pub fn ext_kind(path: &Path) -> Kind {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "tiff" | "tif" | "heic" | "bmp" | "webp" => Kind::Image,
        "mov" | "mp4" | "m4v" => Kind::Video,
        _ => Kind::File,
    }
}

/// Sample items used only for the `--screenshot` check picture, so no private history is shown.
pub fn sample_items() -> Vec<Item> {
    let now = now_secs();
    let mk = |kind, text: Option<&str>, path: Option<&str>, source: &str, ago: u64, pinned| Item {
        id: new_id() + ago,
        kind,
        text: text.map(|s| s.to_string()),
        path: path.map(PathBuf::from),
        source: source.into(),
        created: now - ago,
        pinned,
        width: 0,
        height: 0,
        hash: 0,
    };
    vec![
        mk(Kind::Text, Some("Meeting moved to Thursday 3 pm, room 4B.\nBring the printed slides."), None, "Copied", 5, false),
        mk(Kind::Image, None, Some("sample://screenshot"), "Screenshot", 95, false),
        mk(Kind::Video, None, Some("sample://recording"), "Recording", 640, false),
        mk(Kind::Text, Some("https://example.com/docs/getting-started"), None, "Copied", 1500, true),
        mk(Kind::Text, Some("npm run build && npm test"), None, "Copied", 4000, false),
    ]
}
