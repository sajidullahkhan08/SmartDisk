use std::path::Path;

use crate::core::util::split_components;

/// Lightweight, component-aware description of a directory path.
///
/// It is computed once per scanned directory and reused for every child item.
/// All matching is component-based and case-insensitive; no substring matching
/// is used for location decisions.
#[derive(Clone, Debug)]
pub struct PathContext {
    pub drive: String,
    pub components: Vec<String>,
    pub depth: usize,
    pub is_drive_root: bool,

    pub in_windows: bool,
    pub in_system32: bool,
    pub in_wow64: bool,
    pub in_winsxs: bool,
    pub in_servicing: bool,
    pub in_softwaredistribution: bool,
    pub in_windows_temp: bool,
    pub in_windows_cleanup_zone: bool,

    pub in_program_files: bool,
    pub in_programdata: bool,
    pub in_programdata_microsoft: bool,

    pub in_users: bool,
    pub user_name: Option<String>,
    pub is_user_root: bool,
    pub in_appdata_local: bool,
    pub in_appdata_roaming: bool,

    pub in_local_temp: bool,
    pub in_documents: bool,
    pub in_desktop: bool,
    pub in_downloads: bool,
    pub in_pictures: bool,
    pub in_videos: bool,
    pub in_music: bool,

    pub in_recycle_bin: bool,
    pub in_system_volume_information: bool,
    pub in_boot_area: bool,

    pub in_onedrive: bool,
    pub in_cloud: bool,
    pub browser: Option<String>,
    pub browser_cache: bool,
    pub in_steam: bool,
    pub in_games: bool,
    pub in_vm: bool,
    pub in_backup: bool,
    pub in_dev_project: bool,
    pub in_dev_dependencies: bool,
    pub in_cache_dir: bool,
    pub in_logs_dir: bool,
}

impl PathContext {
    pub fn new(path: &Path) -> Self {
        let (drive, mut c) = split_components(path);
        // Resolve lexical `..` components so adversarial paths such as
        // `C:\Users\Alice\..\Windows\System32` cannot hide a protected
        // location from classification and the risk engine.
        normalize_parent_components(path, &mut c);
        let depth = c.len();

        let starts = |seq: &[&str]| -> bool { starts_with_seq(&c, seq) };
        let in_users = depth >= 2 && c[0] == "users";
        let user_name = in_users.then(|| c[1].clone());
        let first = c.first().map(|s| s.as_str()).unwrap_or("");
        let last = c.last().map(|s| s.as_str()).unwrap_or("");

        let in_windows = first == "windows";
        let in_program_files =
            first == "program files" || first == "program files (x86)" || first == "programfiles";
        let in_programdata = first == "programdata";

        let in_appdata_local = starts(&["users", "*", "appdata", "local"]);
        let in_appdata_roaming = starts(&["users", "*", "appdata", "roaming"]);

        let in_windows_temp = in_windows && c.get(1).map(|s| s == "temp").unwrap_or(false);
        let in_windows_cleanup_zone = in_windows
            && (in_windows_temp
                || c.get(1).map(|s| s == "prefetch").unwrap_or(false)
                || starts(&["windows", "softwaredistribution", "download"])
                || c.get(1).map(|s| s == "logs").unwrap_or(false)
                || c.get(1).map(|s| s == "minidump").unwrap_or(false)
                || c.get(1).map(|s| s == "livekernelreports").unwrap_or(false));

        let in_onedrive = c
            .iter()
            .any(|s| s == "onedrive" || s.starts_with("onedrive -"));
        let cloud_names = [
            "dropbox",
            "google drive",
            "googledrive",
            "my drive",
            "icloud drive",
            "pcloud",
            "nextcloud",
        ];
        let in_cloud = in_onedrive || c.iter().any(|s| cloud_names.contains(&s.as_str()));

        let browser = browser_kind(&c);
        let browser_cache = browser.is_some()
            && (c.iter().any(|s| is_cache_component(s))
                || c.get(c.len().saturating_sub(1))
                    .map(|s| s == "cache2")
                    .unwrap_or(false));

        let in_steam = c.iter().any(|s| s == "steamapps" || s == "steam");
        let in_games = in_steam
            || c.iter()
                .any(|s| s == "epic games" || s == "gog games" || s == "riot games");

        let in_vm = c.iter().any(|s| {
            matches!(
                s.as_str(),
                "virtualbox vms" | "vmware" | "hyper-v" | "virtual machines" | "wsl" | "docker"
            )
        });

        let in_backup = c
            .iter()
            .any(|s| matches!(s.as_str(), "backup" | "backups" | "windowsimagebackup"));

        let in_dev_project = c
            .iter()
            .any(|s| matches!(s.as_str(), ".git" | ".hg" | ".svn"));

        let dependency_names = [
            "node_modules",
            "target",
            ".venv",
            "venv",
            "__pycache__",
            "site-packages",
            ".cargo",
            ".rustup",
            ".m2",
            ".gradle",
            ".nuget",
            ".conda",
            ".npm",
            ".pnpm-store",
            ".yarn",
            "bower_components",
        ];
        let in_dev_dependencies = c.iter().any(|s| dependency_names.contains(&s.as_str()));

        let in_cache_dir = is_cache_component(last) || in_windows_temp || in_local_temp(&c);
        let in_logs_dir = last == "logs" || last == "log";

        PathContext {
            drive,
            is_drive_root: depth == 0,
            in_windows,
            in_system32: in_windows && c.get(1).map(|s| s == "system32").unwrap_or(false),
            in_wow64: in_windows && c.get(1).map(|s| s == "syswow64").unwrap_or(false),
            in_winsxs: in_windows && c.get(1).map(|s| s == "winsxs").unwrap_or(false),
            in_servicing: in_windows && c.get(1).map(|s| s == "servicing").unwrap_or(false),
            in_softwaredistribution: in_windows
                && c.get(1)
                    .map(|s| s == "softwaredistribution")
                    .unwrap_or(false),
            in_windows_temp,
            in_windows_cleanup_zone,
            in_program_files,
            in_programdata,
            in_programdata_microsoft: in_programdata
                && c.get(1).map(|s| s == "microsoft").unwrap_or(false),
            in_users,
            user_name,
            is_user_root: in_users && depth == 2,
            in_appdata_local,
            in_appdata_roaming,
            in_local_temp: in_local_temp(&c),
            in_documents: starts(&["users", "*", "documents"])
                || starts(&["users", "*", "my documents"]),
            in_desktop: starts(&["users", "*", "desktop"]),
            in_downloads: starts(&["users", "*", "downloads"]),
            in_pictures: starts(&["users", "*", "pictures"]),
            in_videos: starts(&["users", "*", "videos"]),
            in_music: starts(&["users", "*", "music"]),
            in_recycle_bin: c.iter().any(|s| s == "$recycle.bin"),
            in_system_volume_information: c
                .iter()
                .any(|s| s == "system volume information"),
            in_boot_area: first == "boot" || first == "efi" || first == "recovery",
            in_onedrive,
            in_cloud,
            browser,
            browser_cache,
            in_steam,
            in_games,
            in_vm,
            in_backup,
            in_dev_project,
            in_dev_dependencies,
            in_cache_dir,
            in_logs_dir,
            components: c,
            depth,
        }
    }

    pub fn last_component(&self) -> &str {
        self.components
            .last()
            .map(|s| s.as_str())
            .unwrap_or_default()
    }

    pub fn parent_component(&self) -> &str {
        let n = self.components.len();
        if n >= 2 {
            &self.components[n - 2]
        } else {
            ""
        }
    }

    pub fn is_in(&self, name: &str) -> bool {
        self.components.iter().any(|c| c == name)
    }

    pub fn has_component(&self, name: &str) -> bool {
        self.is_in(&name.to_ascii_lowercase())
    }

    /// True for locations where deletion could break Windows or installed apps.
    pub fn is_critical_location(&self) -> bool {
        if self.is_drive_root || self.is_user_root {
            return true;
        }
        if self.in_system_volume_information || (self.in_boot_area && self.depth == 1) {
            return true;
        }
        if self.in_windows {
            // A few narrowly-scoped, well-known cleanup zones inside Windows are
            // not treated as part of the protected installation.
            return !self.in_windows_cleanup_zone;
        }
        if self.in_program_files {
            return true;
        }
        if self.in_programdata_microsoft {
            return true;
        }
        false
    }

    pub fn is_recognized_cleanup_location(&self) -> bool {
        self.in_windows_cleanup_zone || self.in_local_temp || self.in_recycle_bin || self.in_cache_dir
    }
}

fn normalize_parent_components(path: &Path, components: &mut Vec<String>) {
    if !path.is_absolute() {
        return;
    }
    let is_unc = matches!(
        path.components().next(),
        Some(std::path::Component::Prefix(prefix))
            if prefix.as_os_str().to_string_lossy().starts_with("\\\\")
    );
    let mut out = Vec::with_capacity(components.len());
    for part in components.drain(..) {
        if part == ".." {
            // The parent of a drive root is the root itself.
            if out.is_empty() {
                continue;
            }
            // Never pop the \\server\share component of a UNC path.
            if is_unc && out.len() == 1 {
                continue;
            }
            out.pop();
        } else {
            out.push(part);
        }
    }
    *components = out;
}

fn starts_with_seq(components: &[String], seq: &[&str]) -> bool {
    if seq.len() > components.len() {
        return false;
    }
    seq.iter()
        .zip(components.iter())
        .all(|(want, have)| *want == "*" || *want == have.as_str())
}

fn in_local_temp(c: &[String]) -> bool {
    starts_with_seq(c, &["users", "*", "appdata", "local", "temp"])
}

pub fn is_cache_component(s: &str) -> bool {
    matches!(
        s,
        "cache"
            | "caches"
            | "cache2"
            | "code cache"
            | "gpucache"
            | "graphitecache"
            | "dawncache"
            | "grshadercache"
            | "shadercache"
            | "cachestorage"
            | "service worker"
            | "browsercache"
            | "tmp"
            | "temp"
    )
}

fn browser_kind(c: &[String]) -> Option<String> {
    let has = |seq: &[&str]| starts_with_seq(c, seq);
    if has(&["users", "*", "appdata", "local", "google", "chrome", "user data"])
        || has(&["users", "*", "appdata", "roaming", "google", "chrome", "user data"])
    {
        return Some("Google Chrome".into());
    }
    if has(&["users", "*", "appdata", "local", "microsoft", "edge", "user data"])
        || has(&["users", "*", "appdata", "roaming", "microsoft", "edge", "user data"])
    {
        return Some("Microsoft Edge".into());
    }
    if has(&["users", "*", "appdata", "local", "brave-browser", "user data"]) {
        return Some("Brave".into());
    }
    if has(&["users", "*", "appdata", "roaming", "mozilla", "firefox"])
        || has(&["users", "*", "appdata", "local", "mozilla", "firefox"])
    {
        return Some("Mozilla Firefox".into());
    }
    if has(&["users", "*", "appdata", "roaming", "opera software"]) {
        return Some("Opera".into());
    }
    if has(&["users", "*", "appdata", "local", "vivaldi", "user data"]) {
        return Some("Vivaldi".into());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(p: &str) -> PathContext {
        PathContext::new(Path::new(p))
    }

    #[test]
    fn detects_windows_components_case_insensitively() {
        let c = ctx("C:\\WINDOWS\\System32\\drivers");
        assert!(c.in_windows);
        assert!(c.in_system32);
        assert!(c.is_critical_location());
    }

    #[test]
    fn does_not_match_substrings() {
        let c = ctx("D:\\Projects\\Windows-Tools\\notes");
        assert!(!c.in_windows);
        assert!(!c.is_critical_location());
    }

    #[test]
    fn windows_temp_is_cleanup_zone() {
        let c = ctx("C:\\Windows\\Temp");
        assert!(c.in_windows);
        assert!(c.in_windows_temp);
        assert!(c.in_windows_cleanup_zone);
        assert!(!c.is_critical_location());
    }

    #[test]
    fn detects_user_folders() {
        let c = ctx("C:\\Users\\Alice\\Documents\\Work");
        assert!(c.in_users);
        assert!(c.in_documents);
        assert_eq!(c.user_name.as_deref(), Some("alice"));
        assert!(!c.is_critical_location());
        assert!(ctx("C:\\Users\\Alice").is_critical_location());
        assert!(!ctx("C:\\Users\\Alice\\Documents").is_critical_location());
    }

    #[test]
    fn detects_recycle_bin_and_dev_dependencies() {
        assert!(ctx("C:\\$Recycle.Bin\\S-1-5-21").in_recycle_bin);
        let c = ctx("C:\\Users\\Alice\\dev\\project\\node_modules\\pkg");
        assert!(c.in_dev_dependencies);
        assert!(ctx("C:\\Users\\Alice\\dev\\project\\.git\\objects").in_dev_project);
    }

    #[test]
    fn recognizes_browser_cache() {
        let c = ctx("C:\\Users\\Alice\\AppData\\Local\\Google\\Chrome\\User Data\\Default\\Cache\\Cache_Data");
        assert_eq!(c.browser.as_deref(), Some("Google Chrome"));
        assert!(c.browser_cache);
    }

    #[test]
    fn recognizes_program_files_and_programdata() {
        assert!(ctx("C:\\Program Files\\App").is_critical_location());
        assert!(!ctx("C:\\ProgramData\\App\\Cache").is_critical_location());
        assert!(ctx("C:\\ProgramData\\Microsoft\\Windows").is_critical_location());
    }
}
