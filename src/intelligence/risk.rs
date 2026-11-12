use std::path::Path;

use crate::core::models::{
    Category, RiskLevel, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SYSTEM,
};
use crate::core::util::age_days;
use crate::intelligence::features::PathContext;

#[derive(Clone, Debug)]
pub struct RiskReport {
    pub level: RiskLevel,
    pub summary: String,
    pub consequences: String,
    pub evidence: Vec<String>,
    pub recommendation: String,
    pub missing_evidence: Option<String>,
    pub uncertain: bool,
    pub blocked: bool,
}

/// Compute the risk level. The classifier's category is used as evidence, but
/// never decides safety by itself. Protected-location policy is enforced last.
pub fn assess(
    ctx: &PathContext,
    is_dir: bool,
    attributes: u32,
    modified: Option<i64>,
    category: Category,
    now: i64,
) -> RiskLevel {
    let age = age_days(modified, now).unwrap_or(0);
    let is_reparse = attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    let critical = ctx.is_critical_location();

    let mut level = if critical {
        match category {
            // Narrowly scoped cleanup zones are still evaluated as reviewable.
            Category::TemporaryFiles if ctx.in_windows_cleanup_zone => RiskLevel::Review,
            Category::WindowsUpdate if ctx.in_softwaredistribution => RiskLevel::Review,
            Category::Logs if ctx.in_windows_cleanup_zone => RiskLevel::Review,
            Category::ApplicationCache if ctx.in_windows_cleanup_zone => RiskLevel::Review,
            _ => RiskLevel::Critical,
        }
    } else {
        match category {
            Category::WindowsSystem => RiskLevel::High,
            Category::WindowsUpdate => RiskLevel::Review,
            Category::ApplicationInstallation | Category::Executables => RiskLevel::High,
            Category::ApplicationData => RiskLevel::High,
            Category::ApplicationCache => {
                if !is_dir && age >= 30 && ctx.in_cache_dir {
                    RiskLevel::Lower
                } else {
                    RiskLevel::Review
                }
            }
            Category::TemporaryFiles => {
                if !is_dir && age >= 7 && (ctx.in_cache_dir || ctx.in_local_temp || ctx.in_windows_temp)
                {
                    RiskLevel::Lower
                } else if !is_dir && ctx.in_recycle_bin {
                    RiskLevel::Review
                } else {
                    RiskLevel::Review
                }
            }
            Category::Logs => {
                if !is_dir && age >= 30 {
                    RiskLevel::Lower
                } else {
                    RiskLevel::Review
                }
            }
            Category::BrowserData => {
                if ctx.browser_cache {
                    if !is_dir && age >= 14 {
                        RiskLevel::Lower
                    } else {
                        RiskLevel::Review
                    }
                } else {
                    RiskLevel::High
                }
            }
            Category::DevelopmentDependencies => RiskLevel::Review,
            Category::RecycleBin => RiskLevel::Review,
            Category::UserDocuments
            | Category::Images
            | Category::Videos
            | Category::Audio
            | Category::Archives
            | Category::Installers
            | Category::DevelopmentProjects
            | Category::VirtualMachines
            | Category::DatabaseStorage
            | Category::GameData
            | Category::BackupData
            | Category::CloudData => RiskLevel::High,
            Category::Other | Category::Unknown => RiskLevel::Unknown,
        }
    };

    // A reparse point hides its target. Never call it lower-risk.
    if is_reparse
        && !matches!(
            level,
            RiskLevel::Critical | RiskLevel::High
        )
    {
        level = RiskLevel::High;
    }
    // A system-flagged item in a protected area is protected regardless.
    if attributes & FILE_ATTRIBUTE_SYSTEM != 0 && critical {
        level = RiskLevel::Critical;
    }
    level
}

pub fn assess_detailed(
    path: &Path,
    _name: &str,
    is_dir: bool,
    attributes: u32,
    size: u64,
    modified: Option<i64>,
    category: Category,
    now: i64,
) -> RiskReport {
    // For files, location evidence comes from the containing folder. Folder
    // names such as "Cache" or "Temp" describe the file's context, and a file
    // stored directly in a protected root is as sensitive as the root itself.
    let context_path = if is_dir {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    let ctx = PathContext::new(context_path);
    let level = assess(&ctx, is_dir, attributes, modified, category, now);
    let age = age_days(modified, now);

    let mut evidence = Vec::new();
    evidence.push(format!("Classifier category: {}", category.label()));
    if ctx.is_drive_root {
        evidence.push("Item is a drive root.".into());
    }
    if ctx.is_user_root {
        evidence.push("Item is a user profile root.".into());
    }
    if ctx.in_windows {
        evidence.push("Path is inside the Windows directory (component match).".into());
    }
    if ctx.in_program_files {
        evidence.push("Path is inside a Program Files installation directory.".into());
    }
    if ctx.in_programdata_microsoft {
        evidence.push("Path is inside ProgramData\\Microsoft.".into());
    }
    if ctx.in_programdata {
        evidence.push("Path is inside shared ProgramData.".into());
    }
    if ctx.in_appdata_local || ctx.in_appdata_roaming {
        evidence.push("Path is inside per-user AppData.".into());
    }
    if ctx.in_local_temp || ctx.in_windows_temp {
        evidence.push("Path is a recognized temporary directory.".into());
    }
    if ctx.in_cache_dir {
        evidence.push("Path or folder name is a recognized cache location.".into());
    }
    if ctx.in_recycle_bin {
        evidence.push("Path is inside the Recycle Bin.".into());
    }
    if ctx.in_dev_project || ctx.in_dev_dependencies {
        evidence.push("Path is associated with a development project or dependency tree.".into());
    }
    if ctx.in_dev_dependencies {
        evidence.push("Dependencies and build output are often reproducible, but rebuilding costs time.".into());
    }
    if ctx.in_onedrive || ctx.in_cloud {
        evidence.push("Path appears to be cloud-synchronized.".into());
    }
    if is_dir {
        evidence.push("Item is a directory; deleting it may affect all contained items.".into());
    }
    if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        evidence.push("Item is a reparse point (junction/symbolic link); its target is not followed by the scanner.".into());
    }
    if attributes & FILE_ATTRIBUTE_SYSTEM != 0 {
        evidence.push("Item has the Windows 'System' attribute.".into());
    }
    if let Some(days) = age {
        if !is_dir {
            evidence.push(format!("Last modified about {days} day(s) ago."));
        }
    }
    if size == 0 {
        evidence.push("Item reports a logical size of zero bytes.".into());
    }

    let (summary, consequences, recommendation, missing, uncertain) = match level {
        RiskLevel::Critical => (
            "Critical / protected location.".to_string(),
            "Removing this item can break Windows, an installed application, or your user profile. Some changes may be difficult or impossible to undo.".to_string(),
            "Do not delete this item. SmartDisk will not include it in ordinary cleanup.".to_string(),
            None,
            false,
        ),
        RiskLevel::High => (
            high_summary(category),
            high_consequences(category),
            "Review the contents and confirm you have a backup. SmartDisk does not recommend routine cleanup here; if you delete, use the Recycle Bin.".to_string(),
            None,
            false,
        ),
        RiskLevel::Review => (
            review_summary(category),
            review_consequences(category),
            "Close related applications and inspect the contents before deleting. Use the Recycle Bin so the action can be undone.".to_string(),
            None,
            true,
        ),
        RiskLevel::Lower => (
            "Lower risk: strong evidence of temporary or cache data.".to_string(),
            "Removing this item usually causes it to be recreated or re-downloaded. Specific applications may behave differently.".to_string(),
            "Likely disposable, but confirm that the creating application is not using it. Prefer the Recycle Bin.".to_string(),
            Some("No content inspection was performed; the assessment is based on metadata and location only.".to_string()),
            true,
        ),
        RiskLevel::Unknown => (
            "Insufficient evidence.".to_string(),
            "The effect of deleting this item is unknown. It may be important application or user data.".to_string(),
            "No cleanup recommendation is made. Inspect the item and verify what created it before making changes.".to_string(),
            Some("The path, name, and attributes do not match a sufficiently reliable rule.".to_string()),
            true,
        ),
    };

    let mut missing = missing;
    if level == RiskLevel::High && ctx.is_critical_location() {
        missing = Some("Protected-location policy overrides the classifier category.".into());
    }
    if is_dir && level == RiskLevel::Lower {
        missing = Some("Directory-level lower risk is never assigned automatically.".into());
    }

    RiskReport {
        level,
        summary,
        consequences,
        evidence,
        recommendation,
        missing_evidence: missing,
        uncertain,
        blocked: level == RiskLevel::Critical,
    }
}

fn high_summary(category: Category) -> String {
    match category {
        Category::UserDocuments
        | Category::Images
        | Category::Videos
        | Category::Audio
        | Category::Archives
        | Category::Installers => "Likely important user data.".to_string(),
        Category::ApplicationData => "Application data that may contain settings or local state.".to_string(),
        Category::DevelopmentProjects => "Development project data that may contain source code or uncommitted work.".to_string(),
        Category::VirtualMachines => "Virtual machine data.".to_string(),
        Category::DatabaseStorage => "Database storage.".to_string(),
        Category::BackupData => "Backup data.".to_string(),
        Category::CloudData => "Cloud-synchronized user data.".to_string(),
        Category::GameData => "Game installation or save data.".to_string(),
        Category::BrowserData => "Browser profile data.".to_string(),
        Category::WindowsSystem => "System component outside a protected root.".to_string(),
        _ => "Potentially important data.".to_string(),
    }
}

fn high_consequences(category: Category) -> String {
    match category {
        Category::UserDocuments | Category::Images | Category::Videos | Category::Audio => {
            "Deleting may cause permanent loss of personal files.".to_string()
        }
        Category::ApplicationData => {
            "Deleting may reset application preferences, sign you out, or remove locally stored information.".to_string()
        }
        Category::DevelopmentProjects => {
            "Deleting may destroy source code, local configuration, or uncommitted changes.".to_string()
        }
        Category::VirtualMachines => {
            "A VM disk may contain an entire guest operating system and its files.".to_string()
        }
        Category::DatabaseStorage => {
            "Deleting database files can destroy records required by an application.".to_string()
        }
        Category::BackupData => {
            "This may be the only copy of important files.".to_string()
        }
        Category::CloudData => {
            "Deletion may propagate to other devices through cloud synchronization.".to_string()
        }
        Category::GameData => {
            "Saves, settings, and downloaded game content may be lost.".to_string()
        }
        Category::BrowserData => {
            "Bookmarks, passwords, history, extensions, and open sessions may be lost.".to_string()
        }
        Category::Archives | Category::Installers => {
            "The original content or installer may need to be downloaded again, or may be unavailable offline.".to_string()
        }
        _ => "The item may be required by an application or user.".to_string(),
    }
}

fn review_summary(category: Category) -> String {
    match category {
        Category::TemporaryFiles => "Temporary data in a recognized location.".to_string(),
        Category::ApplicationCache | Category::BrowserData => "Cache data in a recognized location.".to_string(),
        Category::Logs => "Log files in a recognized location.".to_string(),
        Category::WindowsUpdate => "Windows Update download data.".to_string(),
        Category::RecycleBin => "Deleted items retained in the Recycle Bin.".to_string(),
        Category::DevelopmentDependencies => "Development dependencies or build output.".to_string(),
        _ => "Reviewable data with incomplete context.".to_string(),
    }
}

fn review_consequences(category: Category) -> String {
    match category {
        Category::TemporaryFiles | Category::ApplicationCache => {
            "Applications may recreate the data or re-download resources. A running application may be disrupted."
                .to_string()
        }
        Category::Logs => "Recent diagnostic information may be lost.".to_string(),
        Category::WindowsUpdate => "Windows may need to download the update again.".to_string(),
        Category::RecycleBin => {
            "Deleting Recycle Bin content permanently removes the ability to restore those items.".to_string()
        }
        Category::DevelopmentDependencies => {
            "A rebuild may be required, which can take time and network access.".to_string()
        }
        _ => "Some application or user data may be affected.".to_string(),
    }
}

/// Conservative check used before destructive file operations.
///
/// A file cannot itself be a protected *location*, but files stored directly in
/// one (for example `C:\Users\Alice\NTUSER.DAT` or `C:\bootmgr`) are just as
/// sensitive as the folder. Directory checks stay path-only so ordinary user
/// folders such as `C:\Users\Alice\Documents` are not treated as system areas.
pub fn is_protected_path(path: &Path) -> bool {
    if PathContext::new(path).is_critical_location() {
        return true;
    }
    let is_directory = std::fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_dir())
        .unwrap_or(false);
    if is_directory {
        return false;
    }
    path.parent()
        .map(|parent| PathContext::new(parent).is_critical_location())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_REPARSE_POINT};

    fn risk(path: &str, category: Category) -> RiskLevel {
        let ctx = PathContext::new(Path::new(path));
        assess(&ctx, false, 0, Some(1_600_000_000), category, 1_700_000_000)
    }

    #[test]
    fn protected_locations_are_critical() {
        assert_eq!(
            risk("C:\\Windows\\System32\\kernel32.dll", Category::WindowsSystem),
            RiskLevel::Critical
        );
        assert_eq!(
            risk("C:\\Program Files\\App\\app.exe", Category::ApplicationInstallation),
            RiskLevel::Critical
        );
        assert_eq!(
            risk("C:\\Users\\Alice\\Documents", Category::UserDocuments),
            RiskLevel::High
        );
        assert_eq!(
            risk("C:\\Users\\Alice", Category::Other),
            RiskLevel::Critical
        );
        assert_eq!(
            risk("D:\\Windows\\notes.txt", Category::Unknown),
            RiskLevel::Critical
        );
        assert_eq!(
            risk("D:\\Notes\\notes.txt", Category::Unknown),
            RiskLevel::Unknown
        );
    }

    #[test]
    fn temp_and_cache_rules_are_conservative() {
        assert_eq!(
            risk("C:\\Users\\Alice\\AppData\\Local\\Temp\\old.tmp", Category::TemporaryFiles),
            RiskLevel::Lower
        );
        assert_eq!(
            risk("C:\\Windows\\Temp\\old.tmp", Category::TemporaryFiles),
            RiskLevel::Lower
        );
        let ctx = PathContext::new(Path::new("C:\\Windows\\Temp\\folder"));
        assert_eq!(
            assess(&ctx, true, 0, Some(1_600_000_000), Category::TemporaryFiles, 1_700_000_000),
            RiskLevel::Review
        );
        assert_eq!(
            risk("C:\\Windows\\System32\\driver.sys", Category::TemporaryFiles),
            RiskLevel::Critical
        );
    }

    #[test]
    fn unknown_never_becomes_lower() {
        assert_eq!(
            risk("D:\\Mystery\\blob.bin", Category::Unknown),
            RiskLevel::Unknown
        );
    }

    #[test]
    fn reparse_points_are_never_lower_risk() {
        let ctx = PathContext::new(Path::new("C:\\Users\\Alice\\AppData\\Local\\Temp\\link"));
        assert_eq!(
            assess(
                &ctx,
                true,
                FILE_ATTRIBUTE_REPARSE_POINT,
                None,
                Category::TemporaryFiles,
                1_700_000_000
            ),
            RiskLevel::High
        );
    }

    #[test]
    fn system_attribute_in_protected_area_is_critical() {
        let ctx = PathContext::new(Path::new("C:\\Windows\\System32\\config"));
        assert_eq!(
            assess(
                &ctx,
                true,
                FILE_ATTRIBUTE_SYSTEM | FILE_ATTRIBUTE_HIDDEN,
                None,
                Category::ApplicationData,
                1_700_000_000
            ),
            RiskLevel::Critical
        );
    }

    #[test]
    fn detailed_reports_have_recommendations() {
        let report = assess_detailed(
            Path::new("C:\\Users\\Alice\\AppData\\Local\\Google\\Chrome\\User Data\\Default\\Cache\\x"),
            "x",
            false,
            0,
            1234,
            Some(1_600_000_000),
            Category::ApplicationCache,
            1_700_000_000,
        );
        assert!(!report.recommendation.is_empty());
        assert!(!report.summary.is_empty());
        assert!(!report.consequences.is_empty());
        assert!(report.uncertain);
    }
}
