//! Deletion-risk integration tests, including adversarial path shapes.

use std::path::Path;

use smartdisk::core::models::{
    Category, RiskLevel, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SYSTEM,
};
use smartdisk::intelligence::{assess, assess_detailed, is_protected_path, PathContext};

const NOW: i64 = 1_800_000_000;
const OLD: i64 = NOW - 90 * 86_400;

fn risk(path: &str, category: Category, modified: Option<i64>) -> RiskLevel {
    let ctx = PathContext::new(Path::new(path));
    assess(&ctx, false, 0, modified, category, NOW)
}

fn risk_with_attributes(path: &str, category: Category, attributes: u32) -> RiskLevel {
    let ctx = PathContext::new(Path::new(path));
    assess(&ctx, false, attributes, Some(OLD), category, NOW)
}

#[test]
fn drive_and_profile_roots_are_always_protected() {
    assert_eq!(risk(r"D:\", Category::Other, None), RiskLevel::Critical);
    assert_eq!(
        risk(r"C:\Users\Alice", Category::Other, None),
        RiskLevel::Critical
    );
}

#[test]
fn protected_locations_never_receive_cleanup_levels() {
    let protected = [
        r"C:\Windows\System32\kernel32.dll",
        r"C:\Windows\explorer.exe",
        r"C:\Program Files\Example\example.exe",
        r"C:\Program Files (x86)\Example\example.exe",
        r"C:\ProgramData\Microsoft\Windows\Start Menu\file.lnk",
    ];
    for path in protected {
        let level = risk(path, Category::Other, Some(OLD));
        assert_eq!(level, RiskLevel::Critical, "{path} should be protected");
        assert!(is_protected_path(Path::new(path)), "{path} should be protected");
        assert!(!level.cleanup_eligible());
    }
}

#[test]
fn narrow_windows_cleanup_zones_are_reviewable_not_critical() {
    let level = risk(
        r"C:\Windows\Temp\old-installer.tmp",
        Category::TemporaryFiles,
        Some(OLD),
    );
    assert_eq!(level, RiskLevel::Lower);
    assert!(!is_protected_path(Path::new(
        r"C:\Windows\Temp\old-installer.tmp"
    )));
    let update = risk(
        r"C:\Windows\SoftwareDistribution\Download\update.cab",
        Category::WindowsUpdate,
        Some(OLD),
    );
    assert!(matches!(update, RiskLevel::Review | RiskLevel::Lower));
}

#[test]
fn user_data_is_high_risk_but_not_protected() {
    for (path, category) in [
        (r"C:\Users\Alice\Pictures\holiday.jpg", Category::Images),
        (r"C:\Users\Alice\Videos\clip.mp4", Category::Videos),
        (r"C:\Users\Alice\Documents\tax-2025.pdf", Category::UserDocuments),
        (r"C:\Users\Alice\Downloads\album.zip", Category::Archives),
        (r"D:\Backups\weekly.bin", Category::BackupData),
        (r"E:\Cloud\notes.txt", Category::CloudData),
    ] {
        let level = risk(path, category, Some(OLD));
        assert_eq!(level, RiskLevel::High, "{path}");
        assert!(!is_protected_path(Path::new(path)), "{path}");
        assert!(!level.cleanup_eligible());
    }
}

#[test]
fn unknown_stays_unknown_and_is_not_cleanup_eligible() {
    let level = risk(r"D:\Misc\mystery.qwerty", Category::Unknown, None);
    assert_eq!(level, RiskLevel::Unknown);
    assert!(!level.cleanup_eligible());
}

#[test]
fn classification_alone_does_not_decide_safety() {
    // Same category, different locations: the location changes the verdict.
    let windows_temp = risk(
        r"C:\Windows\Temp\old.log",
        Category::Logs,
        Some(OLD),
    );
    let user_log = risk(r"D:\Projects\app\logs\old.log", Category::Logs, Some(OLD));
    assert_eq!(windows_temp, RiskLevel::Lower);
    assert_eq!(user_log, RiskLevel::Lower);
    // ... but a protected location always wins over a cleanup category.
    let protected = risk(
        r"C:\Windows\System32\old.log",
        Category::Logs,
        Some(OLD),
    );
    assert_eq!(protected, RiskLevel::Critical);
}

#[test]
fn similar_directory_names_are_not_confused_with_protected_roots() {
    let windows_apps = risk(
        r"C:\WindowsApps\Example\example.dll",
        Category::ApplicationInstallation,
        Some(OLD),
    );
    assert_ne!(windows_apps, RiskLevel::Critical);
    assert_eq!(windows_apps, RiskLevel::High);

    let user_windows = risk(
        r"C:\Users\Alice\Windows\notes.txt",
        Category::UserDocuments,
        Some(OLD),
    );
    assert_ne!(user_windows, RiskLevel::Critical);

    let microsoft_edge = risk(
        r"C:\ProgramData\MicrosoftEdge\data.bin",
        Category::Other,
        Some(OLD),
    );
    assert_ne!(microsoft_edge, RiskLevel::Critical);
}

#[test]
fn case_variations_are_normalised() {
    assert_eq!(
        risk(r"c:\wInDoWs\SyStEm32\file.dll", Category::WindowsSystem, Some(OLD)),
        RiskLevel::Critical
    );
    assert!(is_protected_path(Path::new(r"c:\PROGRAM FILES\App\app.exe")));
}

#[test]
fn parent_traversal_cannot_hide_a_protected_location() {
    assert!(is_protected_path(Path::new(
        r"C:\Users\Alice\..\..\Windows\System32\kernel32.dll"
    )));
    assert_eq!(
        risk(
            r"C:\Users\Alice\..\..\Windows\System32\kernel32.dll",
            Category::WindowsSystem,
            Some(OLD)
        ),
        RiskLevel::Critical
    );
    // Traversal out of Windows into its cleanup zone stays reviewable.
    let level = risk(
        r"C:\Windows\System32\..\Temp\old.tmp",
        Category::TemporaryFiles,
        Some(OLD),
    );
    assert!(!is_protected_path(Path::new(
        r"C:\Windows\System32\..\Temp\old.tmp"
    )));
    assert!(matches!(level, RiskLevel::Lower | RiskLevel::Review));
}

#[test]
fn reparse_points_are_never_lower_risk() {
    let level = risk_with_attributes(
        r"C:\Windows\Temp\old.tmp",
        Category::TemporaryFiles,
        FILE_ATTRIBUTE_REPARSE_POINT,
    );
    assert_eq!(level, RiskLevel::High);
}

#[test]
fn system_attribute_in_protected_area_is_critical() {
    let level = risk_with_attributes(
        r"C:\Windows\System32\data.tmp",
        Category::Other,
        FILE_ATTRIBUTE_SYSTEM,
    );
    assert_eq!(level, RiskLevel::Critical);
}

#[test]
fn unicode_profile_paths_behave_like_ascii_ones() {
    assert_eq!(
        risk(
            r"C:\Users\Ålice\Documents\skatt.pdf",
            Category::UserDocuments,
            Some(OLD)
        ),
        RiskLevel::High
    );
    assert!(is_protected_path(Path::new(r"C:\Users\Ålice")));
}

#[test]
fn files_directly_inside_protected_roots_are_protected() {
    // A file cannot be a protected location, but directly placing one in a
    // protected root (profile metadata, boot files) is just as sensitive.
    assert!(is_protected_path(Path::new(r"C:\Users\Alice\NTUSER.DAT")));
    assert!(is_protected_path(Path::new(r"C:\bootmgr")));
    // Ordinary user data remains reviewable, not blocked as a system area.
    assert!(!is_protected_path(Path::new(
        r"C:\Users\Alice\Documents\notes.txt"
    )));
    assert!(!is_protected_path(Path::new(
        r"C:\Users\Alice\Downloads\installer.exe"
    )));
}

#[test]
fn detailed_reports_explain_evidence_and_consequences() {
    let report = assess_detailed(
        Path::new(r"C:\Users\Alice\Pictures\holiday.jpg"),
        "holiday.jpg",
        false,
        0,
        4_000_000,
        Some(OLD),
        Category::Images,
        NOW,
    );
    assert_eq!(report.level, RiskLevel::High);
    assert!(!report.summary.is_empty());
    assert!(!report.consequences.is_empty());
    assert!(!report.recommendation.is_empty());
    assert!(report.evidence.iter().any(|line| line.contains("Classifier category")));
    assert!(!report.uncertain);
}

#[test]
fn unknown_reports_say_evidence_is_missing() {
    let report = assess_detailed(
        Path::new(r"D:\Misc\thing.qwerty"),
        "thing.qwerty",
        false,
        0,
        10,
        None,
        Category::Unknown,
        NOW,
    );
    assert_eq!(report.level, RiskLevel::Unknown);
    assert!(report.uncertain);
    assert!(report.missing_evidence.is_some());
    assert!(!report.blocked);
}
