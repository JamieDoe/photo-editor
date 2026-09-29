//! Local logging and error references.
//!
//! Everything stays on the user's machine: log files rotate in the OS log directory
//! (macOS: ~/Library/Logs/<identifier>/). Nothing is uploaded (CLAUDE.md §25).

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use log::LevelFilter;
use tauri::Runtime;
use tauri::plugin::TauriPlugin;
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};

/// Log file size before rotation, and how many rotated files to keep (~25 MB total).
const MAX_LOG_FILE_BYTES: u128 = 5 * 1024 * 1024;
const KEPT_LOG_FILES: usize = 5;

/// The log plugin: rotating file in the OS log dir, plus stdout in development.
pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    let mut targets = vec![Target::new(TargetKind::LogDir {
        file_name: Some("photo-editor".into()),
    })];
    if cfg!(debug_assertions) {
        targets.push(Target::new(TargetKind::Stdout));
    }
    tauri_plugin_log::Builder::new()
        .clear_targets()
        .targets(targets)
        .level(if cfg!(debug_assertions) {
            LevelFilter::Debug
        } else {
            LevelFilter::Info
        })
        // Third-party crates are noisy at debug level.
        .level_for("tao", LevelFilter::Warn)
        .level_for("wry", LevelFilter::Warn)
        .rotation_strategy(RotationStrategy::KeepSome(KEPT_LOG_FILES))
        .max_file_size(MAX_LOG_FILE_BYTES)
        .timezone_strategy(TimezoneStrategy::UseLocal)
        .build()
}

/// Logs panics (with a backtrace) before the default hook runs. Engine jobs already
/// catch panics per job; this covers everything else.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        log::error!(target: "panic", "{info}\n{backtrace}");
        previous(info);
    }));
}

/// A short code shown to the user with an error and written to the log next to the
/// technical detail, so a report can be matched to the log line. Unique per process:
/// start time (seconds, base 36) + sequence number.
pub fn new_reference() -> String {
    static SEQ: AtomicU32 = AtomicU32::new(1);
    static START: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    let start = *START.get_or_init(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    });
    format!(
        "E-{}-{}",
        base36(start % 36u64.pow(5)),
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

fn base36(mut n: u64) -> String {
    const DIGITS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut out = Vec::new();
    loop {
        out.push(DIGITS[(n % 36) as usize]);
        n /= 36;
        if n == 0 {
            break;
        }
    }
    out.reverse();
    String::from_utf8(out).expect("ASCII digits")
}

/// Truncates untrusted text (e.g. from the webview) before it is logged.
pub fn clip(s: &str, max_chars: usize) -> String {
    let mut out: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        out.push_str(" …[truncated]");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_are_unique_and_short() {
        let a = new_reference();
        let b = new_reference();
        assert_ne!(a, b);
        assert!(a.starts_with("E-") && a.len() <= 12, "{a}");
    }

    #[test]
    fn base36_encodes() {
        assert_eq!(base36(0), "0");
        assert_eq!(base36(35), "Z");
        assert_eq!(base36(36), "10");
    }

    #[test]
    fn clip_truncates_on_char_boundaries() {
        assert_eq!(clip("héllo", 10), "héllo");
        assert_eq!(clip("héllo", 2), "hé …[truncated]");
    }
}
