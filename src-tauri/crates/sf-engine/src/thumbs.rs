//! Small picture previews for the browser's image tiles, made on this
//! computer. Photos come back as JPEG, pictures with transparency as PNG,
//! SVGs as themselves (an <img> never runs their scripts).

use image::{DynamicImage, ImageFormat, ImageReader};
use std::fs;
use std::io::{self, Cursor};
use std::path::Path;

/// Bigger files are skipped: decoding them would stall the listing.
const MAX_BYTES: u64 = 60 * 1024 * 1024;
const MAX_SVG_BYTES: u64 = 2 * 1024 * 1024;

/// A preview at most `max_px` on its longer side, as (mime type, bytes).
/// None for files that aren't a supported picture.
pub fn thumbnail(path: &Path, max_px: u32) -> io::Result<Option<(&'static str, Vec<u8>)>> {
    let ext = crate::classify::extension_of(&path.to_string_lossy());
    let size = fs::metadata(path)?.len();
    if ext == "svg" {
        if size > MAX_SVG_BYTES {
            return Ok(None);
        }
        return Ok(Some(("image/svg+xml", fs::read(path)?)));
    }
    if !matches!(
        ext.as_str(),
        "jpg" | "jpeg" | "jfif" | "png" | "gif" | "webp" | "bmp" | "ico" | "tif" | "tiff"
    ) || size > MAX_BYTES
    {
        return Ok(None);
    }
    let img = match ImageReader::open(path)?.with_guessed_format()?.decode() {
        Ok(img) => img,
        // A damaged or unusual picture just keeps its plain icon.
        Err(_) => return Ok(None),
    };
    let small = img.thumbnail(max_px, max_px);
    encode(&small).map(Some)
}

fn encode(img: &DynamicImage) -> io::Result<(&'static str, Vec<u8>)> {
    let mut out = Cursor::new(Vec::new());
    if img.color().has_alpha() {
        img.write_to(&mut out, ImageFormat::Png)
            .map_err(io::Error::other)?;
        Ok(("image/png", out.into_inner()))
    } else {
        img.to_rgb8()
            .write_to(&mut out, ImageFormat::Jpeg)
            .map_err(io::Error::other)?;
        Ok(("image/jpeg", out.into_inner()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage, Rgba, RgbaImage};

    #[test]
    fn photos_shrink_to_jpeg_and_transparent_pictures_stay_png() {
        let tmp = tempfile::tempdir().unwrap();
        let photo = tmp.path().join("photo.png");
        RgbImage::from_pixel(800, 400, Rgb([200, 30, 30]))
            .save(&photo)
            .unwrap();
        let (mime, bytes) = thumbnail(&photo, 160).unwrap().unwrap();
        assert_eq!(mime, "image/jpeg");
        let back = image::load_from_memory(&bytes).unwrap();
        assert_eq!((back.width(), back.height()), (160, 80));

        let logo = tmp.path().join("logo.png");
        RgbaImage::from_pixel(64, 64, Rgba([0, 0, 0, 0]))
            .save(&logo)
            .unwrap();
        assert_eq!(thumbnail(&logo, 160).unwrap().unwrap().0, "image/png");
    }

    #[test]
    fn non_pictures_and_broken_files_get_none() {
        let tmp = tempfile::tempdir().unwrap();
        let txt = tmp.path().join("notes.txt");
        fs::write(&txt, b"hello").unwrap();
        assert!(thumbnail(&txt, 160).unwrap().is_none());
        let broken = tmp.path().join("broken.jpg");
        fs::write(&broken, b"not a jpeg").unwrap();
        assert!(thumbnail(&broken, 160).unwrap().is_none());
        let svg = tmp.path().join("icon.svg");
        fs::write(&svg, b"<svg xmlns='http://www.w3.org/2000/svg'/>").unwrap();
        assert_eq!(thumbnail(&svg, 160).unwrap().unwrap().0, "image/svg+xml");
    }
}
