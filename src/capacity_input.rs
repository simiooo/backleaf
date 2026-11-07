//! Tolerant capacity input for advanced-search size fields (e.g. `10MB`).
//!
//! File sizes use the same binary progression as the task/volume labels, so
//! `10 MB` means 10 × 1024² bytes. Plain digits mean bytes.
pub fn parse_capacity(text: &str) -> Result<u64, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(crate::i18n::tr_owned("请输入容量，如 10MB、500KB 或 1024"));
    }
    let normalized = trimmed.to_lowercase().replace('．', ".");
    let number_end = normalized
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(normalized.len());
    let (number, unit) = normalized.split_at(number_end);
    let number = number.trim_end_matches('.');
    if number.is_empty() {
        return Err(crate::i18n::tr_owned("请输入数字容量，如 10MB"));
    }
    let value = number
        .parse::<f64>()
        .map_err(|_| crate::i18n::tr_owned("请输入数字容量，如 10MB"))?;
    if !value.is_finite() || value < 0. {
        return Err(crate::i18n::tr_owned("容量必须是大于等于 0 的数字"));
    }
    let unit = unit.trim();
    let multiplier = match unit {
        "" | "b" | "byte" | "bytes" | "字节" => 1f64,
        "k" | "kb" | "kib" | "千字节" => 1024.,
        "m" | "mb" | "mib" | "兆" | "兆字节" => 1024. * 1024.,
        "g" | "gb" | "gib" | "吉" | "吉字节" => 1024. * 1024. * 1024.,
        "t" | "tb" | "tib" | "太" | "太字节" => 1024. * 1024. * 1024. * 1024.,
        unit => {
            return Err(format!(
                "无法识别的容量单位“{unit}”；可用 B、KB、MB、GB、TB"
            ));
        }
    };
    let bytes = value * multiplier;
    if !bytes.is_finite() || bytes > u64::MAX as f64 {
        return Err(crate::i18n::tr_owned("容量过大"));
    }
    Ok(bytes.round() as u64)
}

#[cfg(test)]
mod tests {
    use super::parse_capacity;
    #[test]
    fn accepts_plain_bytes_and_tolerant_units() {
        for (text, expected) in [
            ("0", 0),
            ("1024", 1024),
            ("10 KB", 10 * 1024),
            ("10kb", 10 * 1024),
            ("10 k", 10 * 1024),
            ("1.5 MB", 1024 * 1024 + 512 * 1024),
            ("2m", 2 * 1024 * 1024),
            ("1 GB", 1024 * 1024 * 1024),
            ("1.5gb", 1024 * 1024 * 1024 + 512 * 1024 * 1024),
            ("3 字节", 3),
            ("2 兆字节", 2 * 1024 * 1024),
        ] {
            assert_eq!(parse_capacity(text), Ok(expected), "{text}");
        }
    }
    #[test]
    fn rejects_ambiguous_or_unknown_units_with_actionable_errors() {
        for text in ["", "MB", "10XB", "-1MB", "1e3", "10 20MB", "1..5MB"] {
            assert!(parse_capacity(text).is_err(), "{text}");
        }
        assert!(parse_capacity("10XB").unwrap_err().contains("KB"));
        assert!(parse_capacity("").unwrap_err().contains("10MB"));
    }
}
