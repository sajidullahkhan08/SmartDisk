use std::path::Path;

use crate::core::models::Category;
use crate::core::models::Confidence;
use crate::intelligence::features::PathContext;

#[derive(Clone, Debug)]
pub struct Classification {
    pub category: Category,
    pub confidence: Confidence,
    pub rule: &'static str,
    pub evidence: Vec<String>,
    pub explanation: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rule {
    RecycleBin,
    KnownSystemFile,
    WindowsTemp,
    WindowsUpdate,
    WindowsLogs,
    Prefetch,
    WindowsSystem,
    ProgramFiles,
    ProgramDataCache,
    ProgramDataLogs,
    ProgramDataTemp,
    ProgramData,
    BrowserCache,
    BrowserProfile,
    AppTemp,
    AppCache,
    AppLogs,
    AppData,
    DevDependencies,
    DevProject,
    VirtualMachine,
    VirtualMachineDisk,
    CloudData,
    GameData,
    BackupData,
    Downloads,
    Documents,
    Pictures,
    Videos,
    Audio,
    Desktop,
    UserProfileOther,
    TempFile,
    LogFile,
    DatabaseFile,
    InstallerFile,
    ArchiveFile,
    ImageFile,
    VideoFile,
    AudioFile,
    DocumentFile,
    SourceFile,
    LibraryFile,
    SystemFile,
    ExecutableFile,
    CacheFolderName,
    TempFolderName,
    LogsFolderName,
    BackupFolderName,
    DependencyFolderName,
    Unknown,
}

impl Rule {
    fn category(self) -> Category {
        match self {
            Rule::RecycleBin => Category::RecycleBin,
            Rule::KnownSystemFile
            | Rule::WindowsSystem
            | Rule::SystemFile
            | Rule::Prefetch => Category::WindowsSystem,
            Rule::WindowsTemp | Rule::AppTemp | Rule::ProgramDataTemp | Rule::TempFile
            | Rule::TempFolderName => Category::TemporaryFiles,
            Rule::WindowsUpdate => Category::WindowsUpdate,
            Rule::WindowsLogs | Rule::ProgramDataLogs | Rule::AppLogs | Rule::LogFile
            | Rule::LogsFolderName => Category::Logs,
            Rule::ProgramFiles => Category::ApplicationInstallation,
            Rule::ProgramDataCache | Rule::BrowserCache | Rule::AppCache | Rule::CacheFolderName => {
                Category::ApplicationCache
            }
            Rule::ProgramData | Rule::AppData => Category::ApplicationData,
            Rule::BrowserProfile => Category::BrowserData,
            Rule::DevDependencies | Rule::DependencyFolderName => {
                Category::DevelopmentDependencies
            }
            Rule::DevProject => Category::DevelopmentProjects,
            Rule::VirtualMachine | Rule::VirtualMachineDisk => Category::VirtualMachines,
            Rule::CloudData => Category::CloudData,
            Rule::GameData => Category::GameData,
            Rule::BackupData | Rule::BackupFolderName => Category::BackupData,
            Rule::Downloads | Rule::Documents | Rule::Desktop | Rule::DocumentFile => {
                Category::UserDocuments
            }
            Rule::Pictures | Rule::ImageFile => Category::Images,
            Rule::Videos | Rule::VideoFile => Category::Videos,
            Rule::Audio | Rule::AudioFile => Category::Audio,
            Rule::UserProfileOther => Category::Other,
            Rule::DatabaseFile => Category::DatabaseStorage,
            Rule::InstallerFile => Category::Installers,
            Rule::ArchiveFile => Category::Archives,
            Rule::SourceFile => Category::DevelopmentProjects,
            Rule::LibraryFile => Category::ApplicationInstallation,
            Rule::ExecutableFile => Category::Executables,
            Rule::Unknown => Category::Unknown,
        }
    }

    fn confidence(self, _ctx: &PathContext) -> Confidence {
        match self {
            Rule::RecycleBin
            | Rule::WindowsTemp
            | Rule::WindowsUpdate
            | Rule::WindowsSystem
            | Rule::ProgramFiles
            | Rule::AppTemp
            | Rule::AppData
            | Rule::BrowserCache
            | Rule::BrowserProfile
            | Rule::DevDependencies
            | Rule::DevProject
            | Rule::VirtualMachine
            | Rule::CloudData
            | Rule::GameData
            | Rule::BackupData
            | Rule::Pictures
            | Rule::Videos
            | Rule::Audio
            | Rule::Documents => Confidence::High,
            Rule::ProgramData
            | Rule::ProgramDataCache
            | Rule::ProgramDataLogs
            | Rule::ProgramDataTemp
            | Rule::AppCache
            | Rule::AppLogs
            | Rule::WindowsLogs
            | Rule::Prefetch
            | Rule::Downloads
            | Rule::Desktop
            | Rule::TempFile
            | Rule::LogFile
            | Rule::DatabaseFile
            | Rule::InstallerFile
            | Rule::ArchiveFile
            | Rule::ImageFile
            | Rule::VideoFile
            | Rule::AudioFile
            | Rule::DocumentFile
            | Rule::VirtualMachineDisk
            | Rule::BackupFolderName
            | Rule::KnownSystemFile => Confidence::Medium,
            Rule::SourceFile
            | Rule::LibraryFile
            | Rule::SystemFile
            | Rule::ExecutableFile
            | Rule::CacheFolderName
            | Rule::TempFolderName
            | Rule::LogsFolderName
            | Rule::DependencyFolderName
            | Rule::UserProfileOther
            => Confidence::Low,
            Rule::Unknown => Confidence::Unknown,
        }
    }

    fn rule_name(self) -> &'static str {
        match self {
            Rule::RecycleBin => "recycle-bin-location",
            Rule::KnownSystemFile => "known-system-file",
            Rule::WindowsTemp => "windows-temp",
            Rule::WindowsUpdate => "windows-update-download",
            Rule::WindowsLogs => "windows-logs",
            Rule::Prefetch => "windows-prefetch",
            Rule::WindowsSystem => "windows-directory",
            Rule::ProgramFiles => "program-files",
            Rule::ProgramDataCache => "programdata-cache",
            Rule::ProgramDataLogs => "programdata-logs",
            Rule::ProgramDataTemp => "programdata-temp",
            Rule::ProgramData => "programdata",
            Rule::BrowserCache => "browser-cache",
            Rule::BrowserProfile => "browser-profile",
            Rule::AppTemp => "appdata-temp",
            Rule::AppCache => "appdata-cache",
            Rule::AppLogs => "appdata-logs",
            Rule::AppData => "appdata",
            Rule::DevDependencies => "dev-dependency-path",
            Rule::DevProject => "dev-project-path",
            Rule::VirtualMachine => "virtual-machine-path",
            Rule::VirtualMachineDisk => "virtual-machine-disk",
            Rule::CloudData => "cloud-sync-path",
            Rule::GameData => "game-path",
            Rule::BackupData => "backup-path",
            Rule::Downloads => "downloads-folder",
            Rule::Documents => "documents-folder",
            Rule::Pictures => "pictures-folder",
            Rule::Videos => "videos-folder",
            Rule::Audio => "music-folder",
            Rule::Desktop => "desktop-folder",
            Rule::UserProfileOther => "user-profile",
            Rule::TempFile => "temp-extension",
            Rule::LogFile => "log-extension",
            Rule::DatabaseFile => "database-extension",
            Rule::InstallerFile => "installer-extension",
            Rule::ArchiveFile => "archive-extension",
            Rule::ImageFile => "image-extension",
            Rule::VideoFile => "video-extension",
            Rule::AudioFile => "audio-extension",
            Rule::DocumentFile => "document-extension",
            Rule::SourceFile => "source-code-extension",
            Rule::LibraryFile => "library-extension",
            Rule::SystemFile => "system-extension",
            Rule::ExecutableFile => "executable-extension",
            Rule::CacheFolderName => "cache-folder-name",
            Rule::TempFolderName => "temp-folder-name",
            Rule::LogsFolderName => "logs-folder-name",
            Rule::BackupFolderName => "backup-folder-name",
            Rule::DependencyFolderName => "dependency-folder-name",
            Rule::Unknown => "unknown",
        }
    }
}

/// Fast path used inside the scanner: category and qualitative confidence only.
pub fn classify_entry(
    ctx: &PathContext,
    name: &str,
    is_dir: bool,
    attributes: u32,
) -> (Category, Confidence) {
    let rule = classify_rule(ctx, name, is_dir, attributes);
    (rule.category(), rule.confidence(ctx))
}

/// Detailed classification for the information panel, including evidence.
pub fn classify_detailed(
    path: &Path,
    name: &str,
    is_dir: bool,
    attributes: u32,
) -> Classification {
    // Directory-scoped evidence (cache/log folders, profile roots) is derived
    // from the containing folder for files, matching how the scanner builds
    // its context once per directory.
    let context_path = if is_dir {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    let ctx = PathContext::new(context_path);
    let rule = classify_rule(&ctx, name, is_dir, attributes);
    let mut evidence = evidence_for(rule, &ctx, name);
    evidence.push(format!("Matched rule: {}", rule.rule_name()));
    Classification {
        category: rule.category(),
        confidence: rule.confidence(&ctx),
        rule: rule.rule_name(),
        evidence,
        explanation: explanation_for(rule, &ctx, name),
    }
}

fn classify_rule(ctx: &PathContext, name: &str, is_dir: bool, attributes: u32) -> Rule {
    // Lowercase into stack buffers. Windows names are at most 255 UTF-16 units,
    // so this avoids two heap allocations per scanned entry in the common case.
    // ASCII lowercasing preserves the valid-UTF-8 invariant of the input.
    let mut lower_buf = [0u8; 260];
    let lower_heap;
    let lower: &str = if name.len() <= lower_buf.len() {
        for (dst, src) in lower_buf.iter_mut().zip(name.bytes()) {
            *dst = src.to_ascii_lowercase();
        }
        std::str::from_utf8(&lower_buf[..name.len()]).unwrap_or(name)
    } else {
        lower_heap = name.to_ascii_lowercase();
        &lower_heap
    };

    let mut ext_buf = [0u8; 32];
    let ext_heap;
    let ext: &str = if is_dir {
        ""
    } else {
        match extension_bounds(name) {
            None => "",
            Some((start, end)) if end - start <= ext_buf.len() => {
                for (i, b) in name[start..end].bytes().enumerate() {
                    ext_buf[i] = b.to_ascii_lowercase();
                }
                std::str::from_utf8(&ext_buf[..end - start]).unwrap_or("")
            }
            Some((start, end)) => {
                ext_heap = name[start..end].to_ascii_lowercase();
                &ext_heap
            }
        }
    };

    let _ = attributes;

    if ctx.in_recycle_bin {
        return Rule::RecycleBin;
    }
    if is_known_system_file(lower) {
        return Rule::KnownSystemFile;
    }
    if ctx.in_windows {
        if ctx.in_windows_temp {
            return Rule::WindowsTemp;
        }
        if ctx.in_softwaredistribution {
            return Rule::WindowsUpdate;
        }
        if ctx
            .components
            .get(1)
            .map(|c| c == "logs" || c == "minidump" || c == "livekernelreports")
            .unwrap_or(false)
        {
            return Rule::WindowsLogs;
        }
        if ctx.components.get(1).map(|c| c == "prefetch").unwrap_or(false) {
            return Rule::Prefetch;
        }
        return Rule::WindowsSystem;
    }
    if ctx.in_program_files {
        return Rule::ProgramFiles;
    }
    if ctx.in_programdata {
        if ctx.in_cache_dir {
            return Rule::ProgramDataCache;
        }
        if ctx.in_logs_dir {
            return Rule::ProgramDataLogs;
        }
        if ctx.last_component() == "temp" || ctx.last_component() == "tmp" {
            return Rule::ProgramDataTemp;
        }
        return Rule::ProgramData;
    }
    if ctx.browser.is_some() {
        return if ctx.browser_cache {
            Rule::BrowserCache
        } else {
            Rule::BrowserProfile
        };
    }
    if ctx.in_appdata_local || ctx.in_appdata_roaming {
        if ctx.in_local_temp {
            return Rule::AppTemp;
        }
        if ctx.in_cache_dir {
            return Rule::AppCache;
        }
        if ctx.in_logs_dir {
            return Rule::AppLogs;
        }
        return Rule::AppData;
    }
    if ctx.in_dev_dependencies {
        return Rule::DevDependencies;
    }
    if ctx.in_dev_project {
        return Rule::DevProject;
    }
    if ctx.in_vm {
        return Rule::VirtualMachine;
    }
    if !is_dir && is_vm_disk(ext) {
        return Rule::VirtualMachineDisk;
    }
    if ctx.in_cloud {
        return Rule::CloudData;
    }
    if ctx.in_games {
        return Rule::GameData;
    }
    if ctx.in_backup {
        return Rule::BackupData;
    }
    if ctx.in_softwaredistribution {
        return Rule::WindowsUpdate;
    }

    if ctx.in_pictures {
        return Rule::Pictures;
    }
    if ctx.in_videos {
        return Rule::Videos;
    }
    if ctx.in_music {
        return Rule::Audio;
    }
    if ctx.in_downloads {
        return extension_rule(ext, lower, is_dir).unwrap_or(Rule::Downloads);
    }
    if ctx.in_desktop {
        return extension_rule(ext, lower, is_dir).unwrap_or(Rule::Desktop);
    }
    if ctx.in_documents {
        if let Some(rule) = extension_rule(ext, lower, is_dir) {
            if !matches!(rule, Rule::Unknown) {
                return rule;
            }
        }
        return Rule::Documents;
    }
    if ctx.in_users {
        if let Some(rule) = extension_rule(ext, lower, is_dir) {
            return rule;
        }
        return Rule::UserProfileOther;
    }

    if let Some(rule) = extension_rule(ext, lower, is_dir) {
        return rule;
    }

    if is_dir {
        match lower {
            "cache" | "caches" | "cache2" | "code cache" | "gpucache" | "cachestorage" => {
                return Rule::CacheFolderName
            }
            "temp" | "tmp" | "temporary" | "temporary files" => return Rule::TempFolderName,
            "log" | "logs" => return Rule::LogsFolderName,
            "backup" | "backups" | "windowsimagebackup" => return Rule::BackupFolderName,
            "node_modules" | "target" | "__pycache__" | "site-packages" => {
                return Rule::DependencyFolderName
            }
            _ => {}
        }
    }
    if lower == "desktop.ini" {
        return Rule::SystemFile;
    }
    Rule::Unknown
}

/// Byte range of the extension (after the last dot), excluding the dot.
fn extension_bounds(name: &str) -> Option<(usize, usize)> {
    match name.rfind('.') {
        Some(0) | None => None,
        Some(pos) => Some((pos + 1, name.len())),
    }
}

fn extension_rule(ext: &str, lower_name: &str, is_dir: bool) -> Option<Rule> {
    if is_dir {
        return None;
    }
    if ext == "tmp"
        || ext == "temp"
        || ext == "etl"
        || ext == "dmp"
        || ext == "mdmp"
        || lower_name.starts_with("~$")
        || lower_name.ends_with(".tmp")
    {
        return Some(Rule::TempFile);
    }
    if ext == "log" || ext == "log1" || ext == "log2" || ext == "evtx" {
        return Some(Rule::LogFile);
    }
    if matches!(
        ext,
        "vmdk" | "vdi" | "vhd" | "vhdx" | "avhd" | "avhdx" | "qcow2" | "ova" | "ovf"
    ) {
        return Some(Rule::VirtualMachineDisk);
    }
    if matches!(
        ext,
        "db" | "sqlite" | "sqlite3" | "mdf" | "ndf" | "ldf" | "accdb" | "mdb" | "dbf" | "frm"
    ) {
        return Some(Rule::DatabaseFile);
    }
    if matches!(
        ext,
        "msi" | "msix" | "msixbundle" | "appx" | "appxbundle" | "msp"
    ) || (ext == "exe" && is_installer_name(lower_name))
    {
        return Some(Rule::InstallerFile);
    }
    if matches!(
        ext,
        "zip" | "rar" | "7z" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "cab" | "iso"
    ) {
        return Some(Rule::ArchiveFile);
    }
    if matches!(
        ext,
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "tif" | "tiff" | "heic" | "svg"
    ) {
        return Some(Rule::ImageFile);
    }
    if matches!(
        ext,
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "mpg" | "mpeg"
    ) {
        return Some(Rule::VideoFile);
    }
    if matches!(
        ext,
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "wma" | "opus" | "aiff"
    ) {
        return Some(Rule::AudioFile);
    }
    if matches!(
        ext,
        "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "pdf" | "txt" | "rtf" | "odt"
            | "ods" | "odp" | "md" | "csv" | "epub"
    ) {
        return Some(Rule::DocumentFile);
    }
    if matches!(
        ext,
        "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "c" | "cc" | "cpp" | "h" | "hpp" | "cs"
            | "go" | "java" | "kt" | "swift" | "rb" | "php" | "lua" | "sh" | "ps1" | "sql"
            | "toml" | "json" | "yaml" | "yml" | "xml" | "gradle" | "cmake" | "makefile"
    ) {
        return Some(Rule::SourceFile);
    }
    if matches!(ext, "sys" | "drv" | "efi" | "inf" | "cat" | "mui") {
        return Some(Rule::SystemFile);
    }
    if matches!(ext, "dll" | "ocx" | "lib" | "pdb") {
        return Some(Rule::LibraryFile);
    }
    if matches!(ext, "exe" | "com" | "bat" | "cmd" | "ps1" | "scr") {
        return Some(Rule::ExecutableFile);
    }
    if ext.is_empty() {
        return None;
    }
    None
}

fn is_installer_name(lower: &str) -> bool {
    lower.contains("setup")
        || lower.contains("install")
        || lower.contains("unins")
        || lower.starts_with("smartdisk-")
}

fn is_vm_disk(ext: &str) -> bool {
    matches!(
        ext,
        "vmdk" | "vdi" | "vhd" | "vhdx" | "avhd" | "avhdx" | "qcow2" | "ova" | "ovf"
    )
}

fn is_known_system_file(lower: &str) -> bool {
    matches!(
        lower,
        "pagefile.sys"
            | "hiberfil.sys"
            | "swapfile.sys"
            | "ntuser.dat"
            | "ntuser.ini"
            | "bootmgr"
            | "bootnxt"
            | "bootsect.bak"
            | "winload.exe"
            | "winload.efi"
            | "bcd"
            | "desktop.ini"
    )
}

fn evidence_for(rule: Rule, ctx: &PathContext, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let loc = |what: &str| {
        let mut v = vec![format!("Location matches {what} using path-component comparison.")];
        if !ctx.components.is_empty() {
            v.push(format!(
                "Path components: {}",
                ctx.components
                    .iter()
                    .take(6)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" > ")
            ));
        }
        v
    };
    match rule {
        Rule::RecycleBin => out.extend(loc("a $Recycle.Bin directory")),
        Rule::KnownSystemFile => out.push(format!("Filename '{name}' is a known Windows system file.")),
        Rule::WindowsTemp => out.extend(loc("the Windows temporary directory")),
        Rule::WindowsUpdate => out.extend(loc("Windows Update staging/download data")),
        Rule::WindowsLogs => out.extend(loc("Windows diagnostics log directories")),
        Rule::Prefetch => out.extend(loc("the Windows Prefetch directory")),
        Rule::WindowsSystem => out.extend(loc("the Windows installation directory")),
        Rule::ProgramFiles => out.extend(loc("a Program Files installation directory")),
        Rule::ProgramDataCache => out.extend(loc("a ProgramData cache directory")),
        Rule::ProgramDataLogs => out.extend(loc("a ProgramData log directory")),
        Rule::ProgramDataTemp => out.extend(loc("a ProgramData temporary directory")),
        Rule::ProgramData => out.extend(loc("the shared ProgramData directory")),
        Rule::BrowserCache => {
            if let Some(b) = &ctx.browser {
                out.push(format!("Path belongs to a {b} profile."));
            }
            out.push("A cache-related path component was found.".into());
        }
        Rule::BrowserProfile => {
            if let Some(b) = &ctx.browser {
                out.push(format!("Path belongs to a {b} profile."));
            }
        }
        Rule::AppTemp => out.extend(loc("a per-user temporary directory")),
        Rule::AppCache => out.extend(loc("a per-user application cache directory")),
        Rule::AppLogs => out.extend(loc("a per-user application log directory")),
        Rule::AppData => out.extend(loc("per-user AppData")),
        Rule::DevDependencies => {
            if let Some(c) = ctx
                .components
                .iter()
                .find(|c| {
                    ["node_modules", "target", ".venv", "venv", "site-packages", ".cargo"]
                        .contains(&c.as_str())
                })
            {
                out.push(format!("Path contains the dependency/build component '{c}'."));
            }
        }
        Rule::DevProject => out.push("Path contains a version-control metadata component (.git/.hg/.svn).".into()),
        Rule::VirtualMachine => out.extend(loc("a virtual-machine storage path")),
        Rule::VirtualMachineDisk => out.push("File extension belongs to virtual-machine disk images.".into()),
        Rule::CloudData => {
            if let Some(c) = ctx
                .components
                .iter()
                .find(|c| c.contains("onedrive") || c.contains("dropbox") || c.contains("google drive"))
            {
                out.push(format!("Path contains the cloud-sync component '{c}'."));
            } else {
                out.push("Path resembles a cloud-sync location.".into());
            }
        }
        Rule::GameData => out.extend(loc("a game library path")),
        Rule::BackupData | Rule::BackupFolderName => out.push("Path or name indicates backup data.".into()),
        Rule::Downloads => out.push("Path is a user Downloads folder.".into()),
        Rule::Documents => out.push("Path is a user Documents folder.".into()),
        Rule::Pictures => out.push("Path is a user Pictures folder.".into()),
        Rule::Videos => out.push("Path is a user Videos folder.".into()),
        Rule::Audio => out.push("Path is a user Music folder.".into()),
        Rule::Desktop => out.push("Path is a user Desktop folder.".into()),
        Rule::UserProfileOther => out.push("Path is inside a user profile but no specific folder rule matched.".into()),
        Rule::TempFile => out.push("Filename or extension indicates a temporary file.".into()),
        Rule::LogFile => out.push("Extension indicates a log file.".into()),
        Rule::DatabaseFile => out.push("Extension indicates a database file.".into()),
        Rule::InstallerFile => out.push("Extension or name indicates an application installer.".into()),
        Rule::ArchiveFile => out.push("Extension indicates an archive.".into()),
        Rule::ImageFile => out.push("Extension indicates an image.".into()),
        Rule::VideoFile => out.push("Extension indicates a video.".into()),
        Rule::AudioFile => out.push("Extension indicates audio.".into()),
        Rule::DocumentFile => out.push("Extension indicates a document.".into()),
        Rule::SourceFile => out.push("Extension suggests source code; location context is weak.".into()),
        Rule::LibraryFile => out.push("Extension indicates a compiled library or debug component.".into()),
        Rule::SystemFile => out.push("Extension or name suggests a system component.".into()),
        Rule::ExecutableFile => out.push("Extension indicates an executable program.".into()),
        Rule::CacheFolderName => out.push("Folder name suggests a cache, but the location is not a recognized cache directory.".into()),
        Rule::TempFolderName => out.push("Folder name suggests temporary data, but the location is not a recognized temp directory.".into()),
        Rule::LogsFolderName => out.push("Folder name suggests logs, but the location is not a recognized log directory.".into()),
        Rule::DependencyFolderName => out.push("Folder name is a common development dependency/build directory.".into()),
        Rule::Unknown => {}
    }
    out
}

fn explanation_for(rule: Rule, ctx: &PathContext, name: &str) -> String {
    let browser = ctx
        .browser
        .as_deref()
        .unwrap_or("a web browser");
    match rule {
        Rule::RecycleBin => "Likely Recycle Bin content. These items were deleted earlier and are retained until the Recycle Bin is emptied. Review them before removing anything that might still be needed.".into(),
        Rule::KnownSystemFile => format!("'{name}' is a Windows system or profile metadata file. Removing it can affect startup, paging, hibernation, or user sign-in."),
        Rule::WindowsTemp => "Windows temporary files. Windows and applications use this folder for transient data. Some files may be in use; old leftovers are usually safe to review.".into(),
        Rule::WindowsUpdate => "Windows Update download or staging data. Windows uses these files to install updates. Removing them may force Windows to download the update again.".into(),
        Rule::WindowsLogs => "Windows diagnostics or log data. It can help troubleshoot problems and is usually recreated, but recent logs may still be useful.".into(),
        Rule::Prefetch => "Windows Prefetch data. Windows uses it to speed up application startup and will recreate it; removing it is usually harmless but may slow first launches.".into(),
        Rule::WindowsSystem => "Windows system files. This location is part of the Windows installation. Deleting content here can break Windows; SmartDisk does not recommend cleanup here.".into(),
        Rule::ProgramFiles => "Installed application files. Programs depend on these files to run. Removing them can break or partially uninstall the application.".into(),
        Rule::ProgramDataCache => "Shared application cache. The application may recreate these files, but confirm that it is not running before deleting anything.".into(),
        Rule::ProgramDataLogs => "Shared application logs. They are usually recreated, but may contain information needed for support.".into(),
        Rule::ProgramDataTemp => "Shared temporary data under ProgramData. Some files may be in use by services or applications.".into(),
        Rule::ProgramData => "Shared application data. Applications may store configuration, caches, or resources here. Removing it may reset or break an application.".into(),
        Rule::BrowserCache => format!("Likely {browser} cache. Removing cache contents makes the browser reload resources; confirm the browser is not actively using these files."),
        Rule::BrowserProfile => format!("{browser} profile data. It can include history, bookmarks, passwords, extensions, and session state. Deleting it may sign you out or lose browser data."),
        Rule::AppTemp => "Per-user temporary files. Applications use this location for transient data. Some files may currently be in use.".into(),
        Rule::AppCache => "Per-user application cache. The application may recreate or re-download these files. Confirm the application is not running.".into(),
        Rule::AppLogs => "Per-user application log files. They are usually recreated, but may help diagnose problems.".into(),
        Rule::AppData => "Per-user application data. This folder may contain settings, profiles, or local application state. Removing it can reset preferences or lose local information.".into(),
        Rule::DevDependencies => "Development dependencies or build output. These are often reproducible from project manifests, but rebuilding may take time and network access.".into(),
        Rule::DevProject => "Development project data. It may contain source code, uncommitted changes, or local configuration. Treat it as important user data.".into(),
        Rule::VirtualMachine => "Virtual machine data. A VM disk can contain an entire operating system and its files. Verify backups before removing anything.".into(),
        Rule::VirtualMachineDisk => "Virtual machine disk image. It may contain an entire guest system and its data. Do not delete unless you are certain it is obsolete.".into(),
        Rule::CloudData => "Cloud-synchronized data. Changes here may sync to other devices and to the cloud; deletion can affect other copies.".into(),
        Rule::GameData => "Game installation or game data. Saves, settings, and downloaded content may be stored here.".into(),
        Rule::BackupData | Rule::BackupFolderName => "Backup data. It may be the only copy of important files. Verify other backups exist before deleting.".into(),
        Rule::Downloads => "Files in a download folder. They may be re-downloadable, but could include important documents or installers needed offline.".into(),
        Rule::Documents => "User documents. This is likely personal data. Review the contents before deleting.".into(),
        Rule::Pictures => "User pictures. This is likely personal media data.".into(),
        Rule::Videos => "User videos. This is likely personal media data.".into(),
        Rule::Audio => "User audio. This is likely personal media data.".into(),
        Rule::Desktop => "Files on the user's Desktop. They may be working files or shortcuts the user relies on.".into(),
        Rule::UserProfileOther => "This path is inside a user profile, but its purpose is not clear from the location alone. Extension-based evidence is weak.".into(),
        Rule::TempFile => "Filename or extension suggests a temporary file. Temporary files are often disposable, but verify the creating application is not using it.".into(),
        Rule::LogFile => "Log file. Logs are usually recreated, but recent logs may be needed for diagnostics.".into(),
        Rule::DatabaseFile => "Database file. Deleting it may destroy records used by an application. Close the application and verify backups first.".into(),
        Rule::InstallerFile => "Application installer. It can usually be downloaded again, but may be needed for offline or licensed installation.".into(),
        Rule::ArchiveFile => "Archive file. It may contain the only copy of the archived content. Inspect it before deleting.".into(),
        Rule::ImageFile => "Image file. Likely personal or application media; verify it is not needed.".into(),
        Rule::VideoFile => "Video file. Likely personal or application media; verify it is not needed.".into(),
        Rule::AudioFile => "Audio file. Likely personal or application media; verify it is not needed.".into(),
        Rule::DocumentFile => "Document file. It may contain important user content.".into(),
        Rule::SourceFile => "The extension suggests source code, but the surrounding location is not a recognized development area. No strong conclusion is drawn from the extension alone.".into(),
        Rule::LibraryFile => "A compiled library or debug-symbol file. It may be required by an application or a development toolchain.".into(),
        Rule::SystemFile => "The extension or filename suggests a system component, but the location is not a recognized Windows directory.".into(),
        Rule::ExecutableFile => "Executable file. It may be an application, tool, or installer component.".into(),
        Rule::CacheFolderName => "The folder name suggests a cache, but it is not inside a recognized application cache location. Treat this classification as weak evidence.".into(),
        Rule::TempFolderName => "The folder name suggests temporary data, but it is not inside a recognized temp location. Treat this classification as weak evidence.".into(),
        Rule::LogsFolderName => "The folder name suggests logs, but it is not inside a recognized log location. Treat this classification as weak evidence.".into(),
        Rule::DependencyFolderName => "Folder name matches a common development dependency or build-output directory. It may be reproducible, but confirm the project does not rely on it.".into(),
        Rule::Unknown => "SmartDisk does not have enough evidence to determine this item's purpose. No cleanup recommendation is made.".into(),
    }
}

impl Ord for Confidence {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn rank(c: Confidence) -> u8 {
            match c {
                Confidence::Unknown => 0,
                Confidence::Low => 1,
                Confidence::Medium => 2,
                Confidence::High => 3,
            }
        }
        rank(*self).cmp(&rank(*other))
    }
}

impl PartialOrd for Confidence {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
