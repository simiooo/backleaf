//! Component validation for Shell-backed naming, not path parsing.
pub fn validate(name: &str) -> Result<(), &'static str> {
    if name.is_empty() || name == "." || name == ".." {
        return Err("请输入名称");
    }
    if name.ends_with(['.', ' ']) {
        return Err("名称不能以点或空格结尾");
    }
    if name
        .chars()
        .any(|c| c <= '\u{1f}' || "<>:\"/\\|?*".contains(c))
    {
        return Err("名称包含无效字符");
    }
    if name.encode_utf16().count() > 255 {
        return Err("名称过长");
    }
    let base = name
        .split('.')
        .next()
        .unwrap()
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    let device = matches!(
        base.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ["COM", "LPT"].iter().any(|p| {
        base.strip_prefix(p).is_some_and(|n| {
            matches!(
                n,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    });
    if device {
        return Err("此名称由 Windows 保留");
    }
    Ok(())
}
pub fn stem_range(name: &str, directory: bool) -> std::ops::Range<usize> {
    0..if directory {
        name.len()
    } else {
        name.rfind('.').filter(|p| *p > 0).unwrap_or(name.len())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_shell_aliases_and_keeps_unicode() {
        for name in [
            "",
            ".",
            "..",
            "CON.txt",
            "nul.tar.gz",
            "LPT¹.pdf",
            "COM²",
            "CONIN$",
            "filename.",
            "filename ",
            "a/b",
            "x:y",
            "x\0y",
        ] {
            assert!(validate(name).is_err(), "{name}");
        }
        for name in [
            ".gitignore",
            "城市_👩🏽‍💻_é.txt",
            "COM10.txt",
            "预算（最终）.xlsx",
        ] {
            assert!(validate(name).is_ok(), "{name}");
        }
        assert!(validate(&"😀".repeat(128)).is_err());
        assert!(validate(&"文".repeat(255)).is_ok());
    }
    #[test]
    fn rename_stem_uses_utf8_boundaries() {
        let name = "城市_👩🏽‍💻_é.tar.gz";
        assert_eq!(&name[stem_range(name, false)], "城市_👩🏽‍💻_é.tar");
        assert_eq!(&name[stem_range(name, true)], name);
        assert_eq!(stem_range(".gitignore", false), 0..10);
    }
}
