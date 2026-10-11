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
const OWN_ICON: &[&str] = &[
    "exe",
    "lnk",
    "url",
    "ico",
    "app",
    "appref-ms",
    "msc",
    "cpl",
    "scr",
];

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
    #[cfg(windows)]
    if path
        .to_string_lossy()
        .starts_with(crate::finder::SHELL_APPS)
    {
        let (w, h, pixels) = shell_item_icon(&path.to_string_lossy(), i32::from(SIZE))?;
        return png_data_url(w, h, &pixels);
    }
    let icon = match file_icon_provider::get_file_icon(path, SIZE) {
        Ok(i) => i,
        Err(e) => {
            tracing::debug!("no icon for {}: {e:?}", path.display());
            return None;
        }
    };
    png_data_url(icon.width, icon.height, &icon.pixels)
}

/// RGBA pixels as a PNG data URL.
#[cfg(any(windows, target_os = "macos"))]
fn png_data_url(width: u32, height: u32, rgba: &[u8]) -> Option<String> {
    use base64::Engine;
    let mut png_bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png_bytes, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().ok()?;
        w.write_image_data(rgba).ok()?;
    }
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png_bytes)
    ))
}

/// Icon of a shell item by parsing name (e.g. `shell:AppsFolder\<id>` for
/// a Store app, which has no file to read an icon from), as RGBA.
#[cfg(windows)]
fn shell_item_icon(name: &str, size: i32) -> Option<(u32, u32, Vec<u8>)> {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, DIB_RGB_COLORS, HGDIOBJ,
    };
    use windows::Win32::System::Com::IBindCtx;
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
    };
    // SAFETY: COM is initialised on the main thread (where this command
    // runs); every handle made here is released before returning.
    unsafe {
        let factory: IShellItemImageFactory =
            SHCreateItemFromParsingName(&HSTRING::from(name), None::<&IBindCtx>).ok()?;
        let bmp = factory
            .GetImage(
                SIZE { cx: size, cy: size },
                SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK,
            )
            .ok()?;
        let obj = HGDIOBJ(bmp.0);
        let mut info = BITMAP::default();
        let got = GetObjectW(
            obj,
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut info as *mut BITMAP as *mut core::ffi::c_void),
        );
        let (w, h) = (info.bmWidth, info.bmHeight.abs());
        if got == 0 || w <= 0 || h <= 0 || w > 512 || h > 512 {
            let _ = DeleteObject(obj);
            return None;
        }
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                // Negative height: rows top to bottom.
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut px = vec![0u8; (w * h * 4) as usize];
        let dc = CreateCompatibleDC(None);
        let lines = GetDIBits(
            dc,
            bmp,
            0,
            h as u32,
            Some(px.as_mut_ptr() as *mut core::ffi::c_void),
            &mut bmi,
            DIB_RGB_COLORS,
        );
        let _ = DeleteDC(dc);
        let _ = DeleteObject(obj);
        if lines == 0 {
            return None;
        }
        // BGRA with premultiplied alpha -> straight RGBA.
        for p in px.chunks_exact_mut(4) {
            p.swap(0, 2);
            let a = u32::from(p[3]);
            if a > 0 && a < 255 {
                for c in &mut p[..3] {
                    *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
                }
            }
        }
        Some((w as u32, h as u32, px))
    }
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
