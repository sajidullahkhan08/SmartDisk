use serde::{Deserialize, Serialize};

/// User settings persisted in the local SQLite database.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// "dark" or "light".
    pub theme: String,
    pub low_resource: bool,
    /// 0 means automatic (based on available CPUs).
    pub workers: usize,
    pub follow_reparse_points: bool,
    pub exclusions: Vec<String>,
    /// Minimum size for duplicate candidate hashing, in bytes.
    pub min_duplicate_size: u64,
    pub use_recycle_bin: bool,
    pub confirm_destructive: bool,
    pub show_info_panel: bool,
    pub remember_last_location: bool,
    pub last_scan_path: String,
    pub window_width: i32,
    pub window_height: i32,
    pub max_history_entries: usize,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            low_resource: false,
            workers: 0,
            follow_reparse_points: false,
            exclusions: Vec::new(),
            min_duplicate_size: 1024 * 1024,
            use_recycle_bin: true,
            confirm_destructive: true,
            show_info_panel: true,
            remember_last_location: true,
            last_scan_path: String::new(),
            window_width: 1360,
            window_height: 860,
            max_history_entries: 200,
        }
    }
}

impl AppSettings {
    pub fn is_dark(&self) -> bool {
        self.theme != "light"
    }

    pub fn exclusions_text(&self) -> String {
        self.exclusions.join("\n")
    }

    pub fn set_exclusions_text(&mut self, text: &str) {
        self.exclusions = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_conservative() {
        let s = AppSettings::default();
        assert!(s.use_recycle_bin);
        assert!(s.confirm_destructive);
        assert!(!s.follow_reparse_points);
        assert!(s.is_dark());
        assert_eq!(s.min_duplicate_size, 1024 * 1024);
    }

    #[test]
    fn exclusions_round_trip() {
        let mut s = AppSettings::default();
        s.set_exclusions_text("  C:\\Temp  \n\nD:\\Cache\n");
        assert_eq!(s.exclusions, vec!["C:\\Temp", "D:\\Cache"]);
        assert_eq!(s.exclusions_text(), "C:\\Temp\nD:\\Cache");
    }

    #[test]
    fn settings_deserialize_with_missing_fields() {
        let parsed: AppSettings = serde_json::from_str(r#"{"theme":"light"}"#).unwrap();
        assert_eq!(parsed.theme, "light");
        assert!(parsed.use_recycle_bin);
    }
}
