//! File-name presentation only. Never probes content, Shell, associations or the network.
//! Stable paths let GPUI reuse one raster image per type/theme across viewport pages.
use std::{ffi::OsStr, path::Path};

include!("file_icons_generated.rs");

impl FileIcon {
    pub fn for_name(name: &OsStr, directory: bool) -> Self {
        if directory {
            return Self::Folder;
        }
        // A few extensionless build/config names override their apparent suffix.
        if let Some(value) = name.to_str()
            && value.len() <= 32
            && let Some(icon) = Self::from_filename(value)
        {
            return icon;
        }
        let Some(ext) = Path::new(name).extension().and_then(OsStr::to_str) else {
            return Self::Generic;
        };
        // The largest supported extension fits here. No allocation or lossy name
        // conversion is needed; even a non-Unicode stem can have an ASCII suffix.
        let mut folded = [0u8; 32];
        if ext.len() > folded.len() || !ext.is_ascii() {
            return Self::Generic;
        }
        for (to, from) in folded.iter_mut().zip(ext.bytes()) {
            *to = from.to_ascii_lowercase();
        }
        Self::from_extension(std::str::from_utf8(&folded[..ext.len()]).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::FileIcon;
    use std::{ffi::OsStr, os::windows::ffi::OsStringExt};

    #[test]
    fn names_cover_requested_types_without_probing_or_misreading_directories() {
        let cases = [
            ("报告.DOCX", FileIcon::Word),
            ("预算.xlsm", FileIcon::Spreadsheet),
            ("提案.ppsx", FileIcon::Presentation),
            ("合同.pdf", FileIcon::Pdf),
            ("photo.HEIC", FileIcon::Image),
            ("clip.mkv", FileIcon::Video),
            ("song.FLAC", FileIcon::Audio),
            ("archive.tar.gz", FileIcon::Archive),
            ("app.exe", FileIcon::Application),
            ("setup.MSIXBUNDLE", FileIcon::Installer),
            ("app.apk", FileIcon::Installer),
            ("book.epub", FileIcon::Book),
            ("book.azw3", FileIcon::Book),
            ("思源宋体.otf", FileIcon::Font),
            ("font.woff2", FileIcon::Font),
            ("main.rs", FileIcon::Rust),
            ("main.py", FileIcon::Python),
            ("main.ts", FileIcon::Typescript),
            ("main.js", FileIcon::Javascript),
            ("App.tsx", FileIcon::React),
            ("main.cpp", FileIcon::Cpp),
            ("Program.cs", FileIcon::Csharp),
            ("main.go", FileIcon::Go),
            ("Main.swift", FileIcon::Swift),
            ("App.vue", FileIcon::Vue),
            ("README.md", FileIcon::Markdown),
            ("Cargo.toml", FileIcon::Toml),
            ("CMakeLists.txt", FileIcon::Makefile),
            ("Dockerfile", FileIcon::Docker),
            (".gitignore", FileIcon::Config),
            (".env", FileIcon::Config),
            ("report.pdf.exe", FileIcon::Application),
            ("program.exe.txt", FileIcon::Text),
            ("README", FileIcon::Generic),
            ("name.", FileIcon::Generic),
            ("name.custom", FileIcon::Generic),
            ("name.ｐｄｆ", FileIcon::Generic),
            (
                "name.abcdefghijklmnopqrstuvwxyz0123456789",
                FileIcon::Generic,
            ),
        ];
        for (name, expected) in cases {
            assert_eq!(
                FileIcon::for_name(OsStr::new(name), false),
                expected,
                "{name}"
            );
            assert_eq!(FileIcon::for_name(OsStr::new(name), true), FileIcon::Folder);
        }
    }

    #[test]
    fn unpaired_windows_stem_does_not_destroy_a_valid_extension() {
        let name = std::ffi::OsString::from_wide(&[0xD800, 46, 80, 68, 70]);
        assert_eq!(FileIcon::for_name(&name, false), FileIcon::Pdf);
        assert_eq!(FileIcon::for_name(&name, true), FileIcon::Folder);
    }

    #[test]
    fn embedded_artwork_has_clear_padding_and_full_sized_opaque_silhouettes() {
        let kinds = [
            FileIcon::Folder,
            FileIcon::Generic,
            FileIcon::Word,
            FileIcon::Spreadsheet,
            FileIcon::Presentation,
            FileIcon::Pdf,
            FileIcon::Application,
            FileIcon::Installer,
            FileIcon::Image,
            FileIcon::Video,
            FileIcon::Audio,
            FileIcon::Archive,
            FileIcon::Book,
            FileIcon::Font,
            FileIcon::Code,
        ];
        for kind in kinds {
            let bytes = super::asset_bytes(kind.asset(false)).unwrap();
            let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
                .expect("embedded PNG decodes")
                .into_rgba8();
            assert_eq!(image.dimensions(), (192, 192), "{kind:?}");
            let (mut left, mut top, mut right, mut bottom) = (192, 192, 0, 0);
            let mut maximum = 0;
            for (x, y, pixel) in image.enumerate_pixels() {
                if x == 0 || y == 0 || x == 191 || y == 191 {
                    assert_eq!(pixel[3], 0, "{kind:?} clipped edge");
                }
                maximum = maximum.max(pixel[3]);
                if pixel[3] >= 128 {
                    left = left.min(x);
                    top = top.min(y);
                    right = right.max(x + 1);
                    bottom = bottom.max(y + 1);
                }
            }
            assert_eq!(maximum, 255, "{kind:?} opaque body");
            assert!(
                left >= 5 && top >= 5 && right <= 187 && bottom <= 187,
                "{kind:?} breathing room"
            );
            assert!(
                (169..=177).contains(&(right - left).max(bottom - top)),
                "{kind:?} visible artwork shrank"
            );
        }
    }
}
