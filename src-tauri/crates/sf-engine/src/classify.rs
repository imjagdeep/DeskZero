//! Deterministic file classification: extension → Category.
//!
//! A static match table — no magic, no guessing. Unknown extensions and
//! extensionless files land in `Other`. `dmg` is treated as a Disk Image
//! (on Windows/Linux it is never an installer).

use crate::types::Category;

/// Classify a file name or path by its extension. Case-insensitive; the
/// leading dot is optional and absent in practice (`file_stem`/`extension`
/// already strip it).
pub fn classify(file_name: &str) -> Category {
    let ext = extension_of(file_name);
    category_for_ext(&ext)
}

/// Lowercased extension without the dot, or "" when there is none.
pub fn extension_of(file_name: &str) -> String {
    let name = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    // A leading dot is not an extension: ".gitignore", ".env" have none.
    let stem_stripped = name.strip_prefix('.').unwrap_or(name);
    match stem_stripped.rfind('.') {
        Some(i) if i + 1 < stem_stripped.len() => stem_stripped[i + 1..].to_lowercase(),
        _ => String::new(),
    }
}

fn category_for_ext(ext: &str) -> Category {
    match ext {
        // --- Images ---
        "jpg" | "jpeg" | "jfif" | "png" | "gif" | "bmp" | "webp" | "svg" | "ico" | "tif"
        | "tiff" | "heic" | "heif" | "avif" | "raw" | "cr2" | "nef" | "arw" | "dng"
        | "psd" | "ai" | "eps" | "indd" => Category::Images,

        // --- Videos ---
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "mpg" | "mpeg"
        | "3gp" | "ts" | "vob" | "rm" | "m2ts" | "mts" | "asf" | "divx" => Category::Videos,

        // --- Audio ---
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "oga" | "m4a" | "wma" | "opus" | "aiff"
        | "aif" | "amr" | "mid" | "midi" | "ac3" | "aax" | "dsf" | "weba" => Category::Audio,

        // --- Documents ---
        "pdf" | "doc" | "docx" | "txt" | "md" | "rtf" | "odt" | "pages" | "epub" | "mobi"
        | "azw" | "azw3" | "tex" | "log" | "rst" | "djvu" | "xps" | "oxps" | "chm" => {
            Category::Documents
        }

        // --- Spreadsheets ---
        "xls" | "xlsx" | "xlsm" | "csv" | "tsv" | "ods" | "numbers" => Category::Spreadsheets,

        // --- Presentations ---
        "ppt" | "pptx" | "pps" | "ppsx" | "key" | "odp" => Category::Presentations,

        // --- Archives ---
        "zip" | "zipx" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar" | "zst" | "lz4"
        | "cab" | "z" | "lz" | "lzma" | "ace" | "cpio" => Category::Archives,

        // --- Applications (scripts / shortcuts / portable binaries, not installers) ---
        "bat" | "cmd" | "ps1" | "sh" | "bash" | "zsh" | "py" | "rb" | "pl" | "lua" | "jar"
        | "appimage" | "desktop" | "com" | "scr" | "lnk" | "url" | "webloc" => {
            Category::Applications
        }

        // --- Installers / packages ---
        "exe" | "msi" | "msix" | "msixbundle" | "appx" | "appxbundle" | "msu" | "msp"
        | "pkg" | "deb" | "rpm" | "apk" | "ipa" | "run" => Category::Installers,

        // --- Disk images ---
        "iso" | "img" | "dmg" | "vhd" | "vhdx" | "vdi" | "vmdk" | "wim" | "qcow2" | "ova"
        | "ovf" => Category::DiskImages,

        // --- Code / config ---
        "rs" | "go" | "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hh" | "cs" | "java" | "kt"
        | "kts" | "swift" | "m" | "mm" | "tsx" | "jsx" | "js" | "mjs" | "cjs" | "vue"
        | "svelte" | "html" | "htm" | "css" | "scss" | "sass" | "less" | "json" | "jsonc"
        | "toml" | "yaml" | "yml" | "xml" | "ini" | "cfg" | "conf" | "env" | "sql" | "lock" => {
            Category::Code
        }

        _ => Category::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_files_map_to_expected_categories() {
        assert_eq!(classify("photo.JPG"), Category::Images);
        assert_eq!(classify("movie.mp4"), Category::Videos);
        assert_eq!(classify("song.flac"), Category::Audio);
        assert_eq!(classify("invoice.pdf"), Category::Documents);
        assert_eq!(classify("budget.xlsx"), Category::Spreadsheets);
        assert_eq!(classify("deck.pptx"), Category::Presentations);
        assert_eq!(classify("backup.tar.gz"), Category::Archives);
        assert_eq!(classify("installer.msi"), Category::Installers);
        assert_eq!(classify("setup.exe"), Category::Installers);
        assert_eq!(classify("ubuntu.iso"), Category::DiskImages);
        assert_eq!(classify("mac.dmg"), Category::DiskImages);
        assert_eq!(classify("main.rs"), Category::Code);
        assert_eq!(classify("app.json"), Category::Code);
    }

    #[test]
    fn less_common_downloads_are_covered() {
        assert_eq!(classify("photo.jfif"), Category::Images);
        assert_eq!(classify("clip.mts"), Category::Videos);
        assert_eq!(classify("audiobook.aax"), Category::Audio);
        assert_eq!(classify("manual.chm"), Category::Documents);
        assert_eq!(classify("backup.zipx"), Category::Archives);
        assert_eq!(classify("shortcut.lnk"), Category::Applications);
        assert_eq!(classify("bookmark.url"), Category::Applications);
        assert_eq!(classify("driver.msu"), Category::Installers);
        assert_eq!(classify("linux-installer.run"), Category::Installers);
        assert_eq!(classify("vm.qcow2"), Category::DiskImages);
    }

    #[test]
    fn unknown_and_extensionless_are_other() {
        assert_eq!(classify("mystery.xyzabc"), Category::Other);
        assert_eq!(classify("README"), Category::Other);
        assert_eq!(classify("noext."), Category::Other);
    }

    #[test]
    fn dotfiles_have_no_extension() {
        // Dotfiles are classified as Other (no extension); only names like
        // "notes.gitignore" would hit the table — which we don't ship.
        assert_eq!(classify(".gitignore"), Category::Other);
        assert_eq!(classify(".env"), Category::Other);
        assert_eq!(classify(".hidden"), Category::Other);
        assert_eq!(extension_of(".bashrc"), "");
    }

    #[test]
    fn case_insensitive_and_path_tolerant() {
        assert_eq!(
            classify("C:\\\\Photos\\\\Holiday\\\\PIC.PDF"),
            Category::Documents
        );
        assert_eq!(classify("archive.ZIP"), Category::Archives);
    }
}
