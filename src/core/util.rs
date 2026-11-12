use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    if bytes == 0 {
        return "0 B".to_string();
    }
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} B", bytes)
    } else if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else if value >= 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

pub fn format_size_exact(bytes: u64) -> String {
    format!("{} bytes ({})", format_count(bytes), format_size(bytes))
}

pub fn format_count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

pub fn format_timestamp(unix: i64) -> String {
    use chrono::{Local, TimeZone};
    match Local.timestamp_opt(unix, 0) {
        chrono::LocalResult::Single(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
        _ => "-".to_string(),
    }
}

pub fn format_date(unix: i64) -> String {
    use chrono::{Local, TimeZone};
    match Local.timestamp_opt(unix, 0) {
        chrono::LocalResult::Single(dt) => dt.format("%Y-%m-%d").to_string(),
        _ => "-".to_string(),
    }
}

pub fn format_duration_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms} ms")
    } else if ms < 60_000 {
        format!("{:.1} s", ms as f64 / 1000.0)
    } else if ms < 3_600_000 {
        let minutes = ms / 60_000;
        let seconds = (ms % 60_000) / 1000;
        format!("{minutes} min {seconds} s")
    } else {
        let hours = ms / 3_600_000;
        let minutes = (ms % 3_600_000) / 60_000;
        format!("{hours} h {minutes} min")
    }
}

pub fn extension_of(name: &str) -> String {
    match name.rfind('.') {
        Some(0) | None => String::new(),
        Some(pos) => name[pos + 1..].to_ascii_lowercase(),
    }
}

pub fn file_stem(name: &str) -> &str {
    match name.rfind('.') {
        Some(0) | None => name,
        Some(pos) => &name[..pos],
    }
}

/// Remove the Win32 verbatim/UNC prefix for display purposes.
pub fn strip_verbatim(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{rest}")
    } else if let Some(rest) = path.strip_prefix("\\\\?\\") {
        rest.to_string()
    } else if let Some(rest) = path.strip_prefix("\\\\.\\") {
        rest.to_string()
    } else {
        path.to_string()
    }
}

/// Convert an absolute path into a Win32 extended-length path. This is used for
/// filesystem calls so paths longer than MAX_PATH are handled consistently.
pub fn extended_path(path: &Path) -> PathBuf {
    let s = path.as_os_str().to_string_lossy();
    if s.starts_with("\\\\?\\") || s.starts_with("\\\\.\\") {
        return path.to_path_buf();
    }
    if s.starts_with("\\\\") {
        return PathBuf::from(format!("\\\\?\\UNC\\{}", &s[2..]));
    }
    if s.len() >= 2 && s.as_bytes()[1] == b':' {
        return PathBuf::from(format!("\\\\?\\{s}"));
    }
    path.to_path_buf()
}

/// Component-aware, case-insensitive path representation. The drive letter is
/// returned separately; UNC share prefixes remain the first component.
pub fn split_components(path: &Path) -> (String, Vec<String>) {
    let mut drive = String::new();
    let mut out = Vec::new();
    for comp in path.components() {
        match comp {
            Component::Prefix(p) => {
                let s = p.as_os_str().to_string_lossy().to_ascii_lowercase();
                if s.starts_with("\\\\") {
                    out.push(s);
                } else {
                    drive = s;
                }
            }
            Component::RootDir | Component::CurDir => {}
            Component::ParentDir => out.push("..".to_string()),
            Component::Normal(n) => out.push(n.to_string_lossy().to_ascii_lowercase()),
        }
    }
    (drive, out)
}

/// Compare using path components, never substring matching.
pub fn is_same_or_subpath(child: &Path, parent: &Path) -> bool {
    let (_, c) = split_components(child);
    let (_, p) = split_components(parent);
    if p.is_empty() {
        return true;
    }
    c.len() >= p.len() && c[..p.len()] == p[..]
}

pub fn path_equals_ci(a: &Path, b: &Path) -> bool {
    let (ad, ac) = split_components(a);
    let (bd, bc) = split_components(b);
    ad == bd && ac == bc
}

/// A stable key used by SQLite (history excludes / cache), not for security
/// decisions.
pub fn path_key(path: &Path) -> String {
    let s = path.to_string_lossy().replace('/', "\\");
    s.trim_end_matches('\\').to_ascii_lowercase()
}

pub fn validate_file_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("The name cannot be empty.".into());
    }
    if name == "." || name == ".." {
        return Err("The name cannot be '.' or '..'.".into());
    }
    if name.chars().any(|c| matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')) {
        return Err("The name contains a character that is not allowed on Windows.".into());
    }
    if name.chars().any(|c| (c as u32) < 32) {
        return Err("The name contains a control character.".into());
    }
    if name.ends_with(' ') || name.ends_with('.') {
        return Err("Windows does not allow names ending with a space or a dot.".into());
    }
    if name.chars().count() > 255 {
        return Err("The name is longer than 255 characters.".into());
    }
    let base = file_stem(name).to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"]
        .contains(&base.as_str())
        || ((base.len() == 4)
            && (base.starts_with("COM") || base.starts_with("LPT"))
            && base[3..].parse::<u8>().map(|n| (1..=9).contains(&n)).unwrap_or(false));
    if reserved {
        return Err("The name is reserved by Windows.".into());
    }
    Ok(())
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn system_time_to_unix(t: SystemTime) -> Option<i64> {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => Some(d.as_secs() as i64),
        Err(e) => Some(-(e.duration().as_secs() as i64)),
    }
}

pub fn age_days(modified: Option<i64>, now: i64) -> Option<u64> {
    let m = modified?;
    if now <= m {
        Some(0)
    } else {
        Some(((now - m) as u64) / 86_400)
    }
}

pub fn attributes_text(attributes: u32) -> String {
    let mut parts = Vec::new();
    if attributes & crate::core::models::FILE_ATTRIBUTE_READONLY != 0 {
        parts.push("Read-only");
    }
    if attributes & crate::core::models::FILE_ATTRIBUTE_HIDDEN != 0 {
        parts.push("Hidden");
    }
    if attributes & crate::core::models::FILE_ATTRIBUTE_SYSTEM != 0 {
        parts.push("System");
    }
    if attributes & crate::core::models::FILE_ATTRIBUTE_ARCHIVE != 0 {
        parts.push("Archive");
    }
    if attributes & crate::core::models::FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        parts.push("Reparse point");
    }
    if parts.is_empty() {
        "Normal".to_string()
    } else {
        parts.join(", ")
    }
}

/// Safe percentage that never divides by zero and is clamped to 0..=100.
pub fn percent(part: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        ((part as f64 / total as f64) * 100.0).clamp(0.0, 100.0)
    }
}

/// Find a non-existing destination name by appending " (copy N)".
pub fn available_destination(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let (stem, ext) = match file_name.rfind('.') {
        Some(pos) if pos > 0 => (&file_name[..pos], &file_name[pos..]),
        _ => (file_name, ""),
    };
    for n in 1..1000 {
        let candidate = dir.join(format!("{stem} (copy {n}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stem} (copy){ext}"))
}

pub fn normalize_display_root(path: &Path) -> String {
    let s = path.to_string_lossy().replace('/', "\\");
    let s = strip_verbatim(&s);
    if s.len() == 2 && s.ends_with(':') {
        format!("{s}\\")
    } else if s.len() == 3 && s.ends_with(":\\") {
        s
    } else {
        s.trim_end_matches('\\').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_sizes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1536), "1.50 KB");
        assert_eq!(format_size(10 * 1024 * 1024), "10.0 MB");
        assert_eq!(format_size(200 * 1024 * 1024), "200 MB");
        assert_eq!(format_size(3 * 1024u64.pow(3)), "3.00 GB");
    }

    #[test]
    fn formats_counts() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1000), "1,000");
        assert_eq!(format_count(1234567), "1,234,567");
    }

    #[test]
    fn extensions() {
        assert_eq!(extension_of("archive.tar.gz"), "gz");
        assert_eq!(extension_of(".gitignore"), "");
        assert_eq!(extension_of("README"), "");
        assert_eq!(extension_of("A.TXT"), "txt");
    }

    #[test]
    fn verbatim_stripping() {
        assert_eq!(strip_verbatim("\\\\?\\C:\\Windows"), "C:\\Windows");
        assert_eq!(strip_verbatim("\\\\?\\UNC\\server\\share"), "\\\\server\\share");
        assert_eq!(strip_verbatim("C:\\Windows"), "C:\\Windows");
    }

    #[test]
    fn component_subpath_detection() {
        let child = Path::new("C:\\Users\\Alice\\Documents\\a.txt");
        let parent = Path::new("c:\\users\\alice\\documents");
        assert!(is_same_or_subpath(child, parent));
        assert!(!is_same_or_subpath(
            Path::new("C:\\Users\\Alice\\DocumentsOld\\a.txt"),
            parent
        ));
        assert!(!is_same_or_subpath(Path::new("C:\\Windows\\System32"), Path::new("C:\\Win")));
    }

    #[test]
    fn validates_file_names() {
        assert!(validate_file_name("report.txt").is_ok());
        assert!(validate_file_name("CON").is_err());
        assert!(validate_file_name("com1.txt").is_err());
        assert!(validate_file_name("bad/name").is_err());
        assert!(validate_file_name(" trail.").is_err());
        assert!(validate_file_name("").is_err());
    }

    #[test]
    fn percentage_clamps() {
        assert_eq!(percent(50, 100), 50.0);
        assert_eq!(percent(1, 0), 0.0);
        assert_eq!(percent(200, 100), 100.0);
    }
}
