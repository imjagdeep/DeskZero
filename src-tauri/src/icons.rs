//! Real system icons for search results (Windows and macOS): the same icon
//! Explorer or Finder shows, as a PNG data URL. Cached: one entry per file
//! type, and one per app, shortcut or folder (those have their own icons).
//! Linux keeps the generic icons (the icon library needs GTK 4 there).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

#[derive(Default)]
pub struct IconCache(Mutex<HashMap<String, Option<String>>>);

/// Pixel size asked for: crisp at 2x for the 22 px slot in the list.
#[cfg(any(windows, target_os = "macos"))]
const SIZE: u16 = 48;

/// Extensions whose icon belongs to the file itself, not its type.
const OWN_ICON: &[&str] = &["exe", "lnk", "url", "ico", "app", "appref-ms", "msc", "cpl", "scr"];

fn cache_key(path: &Path, kind: &str) -> String {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if kind == "file" && !ext.is_empty() && !OWN_ICON.contains(&ext.as_str()) {
        format!("ext:{ext}")
    } else {
        format!("path:{}", path.display())
    }
}

impl IconCache {
    /// Icons for (path, kind) pairs; paths without an icon are left out.
    pub fn get(&self, items: &[(String, String)]) -> HashMap<String, String> {
        let mut out = HashMap::new();
        let Ok(mut cache) = self.0.lock() else {
            return out;
        };
        for (path, kind) in items {
            let p = Path::new(path);
            let key = cache_key(p, kind);
            let icon = cache.entry(key).or_insert_with(|| load(p)).clone();
            if let Some(url) = icon {
                out.insert(path.clone(), url);
            }
        }
        // A long session can see many apps and folders; keep it bounded.
        if cache.len() > 4000 {
            cache.clear();
        }
        out
    }
}

#[cfg(any(windows, target_os = "macos"))]
fn load(path: &Path) -> Option<String> {
    use base64::Engine;
    let icon = match file_icon_provider::get_file_icon(path, SIZE) {
        Ok(i) => i,
        Err(e) => {
            tracing::debug!("no icon for {}: {e:?}", path.display());
            return None;
        }
    };
    let mut png_bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png_bytes, icon.width, icon.height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().ok()?;
        w.write_image_data(&icon.pixels).ok()?;
    }
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png_bytes)
    ))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn load(_path: &Path) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_share_an_icon_per_type_apps_do_not() {
        assert_eq!(
            cache_key(Path::new("C:/a/report.PDF"), "file"),
            cache_key(Path::new("D:/b/other.pdf"), "file")
        );
        assert_ne!(
            cache_key(Path::new("C:/a/Tool.exe"), "file"),
            cache_key(Path::new("C:/b/Other.exe"), "file")
        );
        assert_ne!(
            cache_key(Path::new("/Applications/A.app"), "app"),
            cache_key(Path::new("/Applications/B.app"), "app")
        );
    }
}
