//! Drive enumeration and volume information via documented Windows APIs.

use std::path::{Path, PathBuf};

use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDiskFreeSpaceW, GetDriveTypeW, GetLogicalDrives,
    GetVolumeInformationW,
};

use crate::core::models::VolumeInfo;

// Documented GetDriveTypeW return values (Win32 API).
const DRIVE_UNKNOWN: u32 = 0;
const DRIVE_NO_ROOT_DIR: u32 = 1;
const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_FIXED: u32 = 3;
const DRIVE_REMOTE: u32 = 4;
const DRIVE_CDROM: u32 = 5;
const DRIVE_RAMDISK: u32 = 6;

#[derive(Clone, Debug)]
pub struct DriveInfo {
    pub letter: String,
    pub root: String,
    pub label: String,
    pub filesystem: String,
    pub drive_type: String,
    pub total: u64,
    pub free: u64,
    pub used: u64,
    pub cluster_size: u64,
    pub serial: u32,
    pub is_network: bool,
    pub is_removable: bool,
    pub is_fixed: bool,
}

impl DriveInfo {
    pub fn used_percent(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.used as f64 / self.total as f64 * 100.0
        }
    }

    /// Stable key used by the drive selector.
    pub fn key(&self) -> String {
        self.root.clone()
    }

    /// Compact label for the header drive dropdown (e.g. `C:`). The full
    /// `display()` name is used where space allows.
    pub fn short(&self) -> String {
        if self.letter.trim().is_empty() {
            self.root.trim_end_matches('\\').to_string()
        } else {
            format!("{}:", self.letter)
        }
    }

    pub fn display(&self) -> String {
        let label = if self.label.trim().is_empty() {
            self.drive_type.clone()
        } else {
            self.label.clone()
        };
        format!("{} ({}:)  {}", label, self.letter, self.drive_type)
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn drive_type_label(value: u32) -> &'static str {
    match value {
        DRIVE_FIXED => "Local disk",
        DRIVE_REMOVABLE => "Removable",
        DRIVE_REMOTE => "Network",
        DRIVE_CDROM => "Optical",
        DRIVE_RAMDISK => "RAM disk",
        DRIVE_NO_ROOT_DIR => "Unavailable",
        _ => "Unknown",
    }
}

fn read_volume(root: &str) -> Option<(String, String, u32, u64, u64, u64, u64)> {
    let root_w = wide(root);
    let mut label = [0u16; 261];
    let mut filesystem = [0u16; 64];
    let mut serial = 0u32;

    let volume_ok = unsafe {
        GetVolumeInformationW(
            PCWSTR(root_w.as_ptr()),
            Some(&mut label),
            Some(&mut serial),
            None,
            None,
            Some(&mut filesystem),
        )
        .is_ok()
    };

    let volume_name = if volume_ok {
        String::from_utf16_lossy(&label[..label.iter().position(|&c| c == 0).unwrap_or(0)])
    } else {
        String::new()
    };
    let fs_name = if volume_ok {
        String::from_utf16_lossy(
            &filesystem[..filesystem.iter().position(|&c| c == 0).unwrap_or(0)],
        )
    } else {
        String::new()
    };

    let mut available = 0u64;
    let mut total = 0u64;
    let mut free = 0u64;
    let size_ok = unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR(root_w.as_ptr()),
            Some(&mut available),
            Some(&mut total),
            Some(&mut free),
        )
        .is_ok()
    };
    if !size_ok {
        return None;
    }

    let mut sectors = 0u32;
    let mut bytes_per_sector = 0u32;
    let mut free_clusters = 0u32;
    let mut total_clusters = 0u32;
    let cluster = unsafe {
        if GetDiskFreeSpaceW(
            PCWSTR(root_w.as_ptr()),
            Some(&mut sectors),
            Some(&mut bytes_per_sector),
            Some(&mut free_clusters),
            Some(&mut total_clusters),
        )
        .is_ok()
        {
            (sectors as u64) * (bytes_per_sector as u64)
        } else {
            0
        }
    };

    Some((volume_name, fs_name, serial, total, free, available, cluster))
}

/// Enumerate accessible drive roots (A: .. Z:).
///
/// Network volumes are listed with their type but without capacity queries,
/// because a disconnected mapping can block for many seconds. Call
/// [`volume_info`] explicitly when capacity for a network path is needed.
pub fn list_drives() -> Vec<DriveInfo> {
    let mask = unsafe { GetLogicalDrives() };
    let mut drives = Vec::new();
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = (b'A' + i as u8) as char;
        let root = format!("{letter}:\\");
        let root_w = wide(&root);
        let type_code = unsafe { GetDriveTypeW(PCWSTR(root_w.as_ptr())) };
        if type_code == DRIVE_NO_ROOT_DIR || type_code == DRIVE_UNKNOWN || type_code == DRIVE_CDROM
        {
            continue;
        }
        let is_network = type_code == DRIVE_REMOTE;
        // Do not touch network shares during startup enumeration.
        let (label, filesystem, serial, total, free, _available, cluster) = if is_network {
            (String::new(), String::new(), 0, 0, 0, 0, 0)
        } else {
            match read_volume(&root) {
                Some(values) => values,
                None => (String::new(), String::new(), 0, 0, 0, 0, 0),
            }
        };
        drives.push(DriveInfo {
            letter: letter.to_string(),
            root,
            label,
            filesystem,
            drive_type: drive_type_label(type_code).to_string(),
            total,
            free,
            used: total.saturating_sub(free),
            cluster_size: cluster,
            serial,
            is_network,
            is_removable: type_code == DRIVE_REMOVABLE,
            is_fixed: type_code == DRIVE_FIXED,
        });
    }
    drives.sort_by(|a, b| a.letter.cmp(&b.letter));
    drives
}

/// Volume information for the root of `path`, when the path has a drive root.
pub fn volume_info(path: &Path) -> Option<VolumeInfo> {
    let root = volume_root(path)?;
    let (label, filesystem, serial, total, free, _available, cluster) = read_volume(&root)?;
    let root_w = wide(&root);
    let type_code = unsafe { GetDriveTypeW(PCWSTR(root_w.as_ptr())) };
    Some(VolumeInfo {
        root: root.clone(),
        label,
        filesystem,
        drive_type: drive_type_label(type_code).to_string(),
        total,
        free,
        used: total.saturating_sub(free),
        cluster_size: cluster,
        serial,
    })
}

fn volume_root(path: &Path) -> Option<String> {
    let text = path.to_string_lossy();
    if text.len() >= 2 && text.as_bytes()[1] == b':' {
        return Some(text[..3].to_string());
    }
    // UNC path: \\server\share
    if text.starts_with("\\\\") {
        let mut parts = text.trim_start_matches('\\').split('\\');
        let server = parts.next()?;
        let share = parts.next()?;
        if server.is_empty() || share.is_empty() {
            return None;
        }
        return Some(format!("\\\\{server}\\{share}"));
    }
    None
}

/// Convenience helper used by the scan-location picker.
pub fn normalize_scan_path(raw: &str) -> Option<PathBuf> {
    let trimmed = raw.trim().trim_matches('"');
    if trimmed.is_empty() {
        return None;
    }
    let path = PathBuf::from(trimmed);
    Some(dunce::simplified(&path).to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_at_least_one_drive() {
        let drives = list_drives();
        assert!(!drives.is_empty(), "expected at least one drive");
        for drive in &drives {
            assert_eq!(drive.root.len(), 3);
            assert!(drive.root.ends_with(":\\"));
        }
    }

    #[test]
    fn system_drive_has_capacity() {
        let drives = list_drives();
        let fixed = drives.iter().find(|d| d.is_fixed).expect("fixed drive");
        assert!(fixed.total > 0);
        assert!(fixed.used <= fixed.total);
        assert_eq!(fixed.total - fixed.free, fixed.used);
    }

    #[test]
    fn volume_root_parsing() {
        assert_eq!(
            volume_root(Path::new("C:\\Users\\test")),
            Some("C:\\".to_string())
        );
        assert_eq!(
            volume_root(Path::new("\\\\server\\share\\folder")),
            Some("\\\\server\\share".to_string())
        );
    }
}
