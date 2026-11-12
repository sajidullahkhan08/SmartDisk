//! Classification integration tests with known path examples.

use std::path::Path;

use smartdisk::core::models::{Category, Confidence};
use smartdisk::intelligence::{classify_detailed, classify_entry, PathContext};

fn classify(path: &str, name: &str, is_dir: bool) -> (Category, Confidence) {
    // The scanner builds one PathContext per directory and reuses it for every
    // child, so files are classified against their containing folder.
    let path = Path::new(path);
    let context_path = if is_dir {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    let ctx = PathContext::new(context_path);
    classify_entry(&ctx, name, is_dir, 0)
}

fn detailed(path: &str, is_dir: bool) -> smartdisk::intelligence::Classification {
    let name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    classify_detailed(Path::new(path), &name, is_dir, 0)
}

#[test]
fn every_documented_category_example_matches() {
    let cases: &[(&str, bool, Category)] = &[
        (r"C:\Windows\System32\kernel32.dll", false, Category::WindowsSystem),
        (r"C:\Windows\Temp\leftover.tmp", false, Category::TemporaryFiles),
        (
            r"C:\Windows\SoftwareDistribution\Download\update.cab",
            false,
            Category::WindowsUpdate,
        ),
        (r"C:\Program Files\Example\example.exe", false, Category::ApplicationInstallation),
        (r"C:\ProgramData\Example\settings.json", false, Category::ApplicationData),
        (r"C:\ProgramData\Example\Cache\data.bin", false, Category::ApplicationCache),
        (
            r"C:\Users\Alice\AppData\Local\Temp\scratch.tmp",
            false,
            Category::TemporaryFiles,
        ),
        (
            r"C:\Users\Alice\AppData\Local\Example\logs\app.log",
            false,
            Category::Logs,
        ),
        (
            r"C:\Users\Alice\AppData\Local\Example\data.db",
            false,
            Category::ApplicationData,
        ),
        (
            r"C:\Users\Alice\AppData\Local\Google\Chrome\User Data\Default\Cache\f_0001",
            false,
            Category::ApplicationCache,
        ),
        (
            r"C:\Users\Alice\AppData\Local\Google\Chrome\User Data\Default\Preferences",
            false,
            Category::BrowserData,
        ),
        (r"C:\Users\Alice\Documents\report.docx", false, Category::UserDocuments),
        (r"C:\Users\Alice\Pictures\photo.jpg", false, Category::Images),
        (r"C:\Users\Alice\Videos\clip.mp4", false, Category::Videos),
        (r"C:\Users\Alice\Music\song.flac", false, Category::Audio),
        (r"C:\Users\Alice\Downloads\setup.exe", false, Category::Installers),
        (r"C:\Users\Alice\Downloads\album.zip", false, Category::Archives),
        (r"C:\Users\Alice\Downloads\random-notes.zzz", false, Category::UserDocuments),
        (r"C:\Users\Alice\OneDrive\notes.txt", false, Category::CloudData),
        (r"D:\Games\Steam\steamapps\common\Game\data.bin", false, Category::GameData),
        (r"E:\Backups\weekly.bin", false, Category::BackupData),
        (r"D:\Virtual Machines\Windows 11.vhdx", false, Category::VirtualMachines),
        (r"D:\Work\project\src\main.rs", false, Category::DevelopmentProjects),
        (
            r"D:\Work\project\node_modules\lib\index.js",
            false,
            Category::DevelopmentDependencies,
        ),
        (r"D:\Work\media\mystery.zzz", false, Category::Unknown),
        (r"C:\$Recycle.Bin\S-1-5-21-1\$RABCDEF.txt", false, Category::RecycleBin),
        (r"C:\pagefile.sys", false, Category::WindowsSystem),
    ];

    for (path, is_dir, expected) in cases {
        let (category, _) = classify(path, Path::new(path).file_name().unwrap().to_str().unwrap(), *is_dir);
        assert_eq!(category, *expected, "unexpected category for {path}");
    }
}

#[test]
fn windows_locations_win_over_file_extensions() {
    // A photo inside System32 is still an operating-system file.
    assert_eq!(
        classify(r"C:\Windows\System32\photo.jpg", "photo.jpg", false).0,
        Category::WindowsSystem
    );
    // An installer inside Windows\Temp is temporary data.
    assert_eq!(
        classify(r"C:\Windows\Temp\setup.exe", "setup.exe", false).0,
        Category::TemporaryFiles
    );
}

#[test]
fn folder_context_is_respected() {
    assert_eq!(
        classify(r"C:\Users\Alice\Pictures\scan.docx", "scan.docx", false).0,
        Category::Images
    );
    assert_eq!(
        classify(r"C:\Users\Alice\Documents\notes.zzz", "notes.zzz", false).0,
        Category::UserDocuments
    );
    assert_eq!(
        classify(r"D:\Work\empty-cache", "empty-cache", true).0,
        Category::Unknown,
        "a neutral folder name without evidence stays unknown"
    );
    assert_eq!(
        classify(r"D:\Work\node_modules", "node_modules", true).0,
        Category::DevelopmentDependencies
    );
    assert_eq!(
        classify(r"D:\Work\temp", "temp", true).0,
        Category::TemporaryFiles
    );
    assert_eq!(
        classify(r"D:\Work\cache", "cache", true).0,
        Category::ApplicationCache
    );
}

#[test]
fn classification_is_case_insensitive() {
    assert_eq!(
        classify(r"c:\wInDoWs\sYsTeM32\file.dll", "file.dll", false).0,
        Category::WindowsSystem
    );
    assert_eq!(
        classify(r"C:\USERS\Alice\PICTURES\image.PNG", "image.PNG", false).0,
        Category::Images
    );
}

#[test]
fn unknown_falls_back_without_false_certainty() {
    let (category, confidence) = classify(r"D:\Misc\thing.qwerty", "thing.qwerty", false);
    assert_eq!(category, Category::Unknown);
    assert!(
        matches!(confidence, Confidence::Low | Confidence::Unknown),
        "unknown items must not claim high confidence, got {confidence:?}"
    );
}

#[test]
fn detailed_classification_explains_itself() {
    let classification = detailed(r"C:\Users\Alice\Pictures\holiday.jpg", false);
    assert_eq!(classification.category, Category::Images);
    assert!(!classification.rule.is_empty());
    assert!(!classification.explanation.is_empty());
    assert!(!classification.evidence.is_empty());
    assert!(classification
        .evidence
        .iter()
        .any(|line| line.starts_with("Matched rule:")));
}

#[test]
fn installers_are_recognised_by_extension_and_name() {
    for name in ["setup.exe", "app-installer.exe", "package.msi", "bundle.msix"] {
        let path = format!(r"C:\Users\Alice\Downloads\{name}");
        assert_eq!(
            classify(&path, name, false).0,
            Category::Installers,
            "expected installer classification for {name}"
        );
    }
}

#[test]
fn archives_and_database_files_have_distinct_categories() {
    assert_eq!(
        classify(r"D:\Data\backup.tar.gz", "backup.tar.gz", false).0,
        Category::Archives
    );
    assert_eq!(
        classify(r"D:\Data\index.sqlite", "index.sqlite", false).0,
        Category::DatabaseStorage
    );
    assert_eq!(
        classify(r"D:\Data\tool.exe", "tool.exe", false).0,
        Category::Executables
    );
}
