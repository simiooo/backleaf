//! Where the app is running from determines whether Windows default-app
//! registration is even meaningful. The portable ZIP and development builds
//! never write system defaults; the installed build opens the Windows Settings
//! confirmation page instead of silently changing registry associations.
use std::path::Path;
use windows::{
    Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    core::w,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Support {
    Installed,
    Portable,
    Development,
}

pub fn support() -> Support {
    std::env::current_exe()
        .ok()
        .as_deref()
        .map(classify)
        .unwrap_or(Support::Portable)
}

pub fn classify(path: &Path) -> Support {
    let lower = path.to_string_lossy().to_ascii_lowercase();
    let installed = lower.contains("\\programs\\backleaf\\")
        || lower.contains("/programs/backleaf/")
        || lower.contains("\\local\\programs\\backleaf\\");
    if installed {
        Support::Installed
    } else if lower.contains("\\target\\") || lower.contains("/target/") {
        Support::Development
    } else {
        Support::Portable
    }
}

pub fn supported() -> bool {
    support() == Support::Installed
}

/// Windows has no supported API for a third-party process to become the
/// default folder handler; the user must confirm it in Settings. This only
/// opens that page.
pub fn open_settings() -> Result<(), String> {
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            w!("ms-settings:defaultapps"),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        Err(crate::i18n::tr_owned("无法打开 Windows 默认应用设置"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Support, classify};
    #[test]
    fn installation_kind_is_derived_from_the_executable_path() {
        assert_eq!(
            classify(std::path::Path::new(
                r"C:\Users\qq651\AppData\Local\Programs\Backleaf\current\Backleaf.exe"
            )),
            Support::Installed
        );
        assert_eq!(
            classify(std::path::Path::new(
                r"C:\Users\qq651\Downloads\Backleaf-0.1.0\Backleaf.exe"
            )),
            Support::Portable
        );
        assert_eq!(
            classify(std::path::Path::new(
                r"D:\work\backleaf\target\release\backleaf.exe"
            )),
            Support::Development
        );
    }
}
