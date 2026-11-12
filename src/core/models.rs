use std::fmt;

pub type NodeId = u32;
pub const INVALID_NODE: NodeId = u32::MAX;

pub const FILE_ATTRIBUTE_READONLY: u32 = 0x0000_0001;
pub const FILE_ATTRIBUTE_HIDDEN: u32 = 0x0000_0002;
pub const FILE_ATTRIBUTE_SYSTEM: u32 = 0x0000_0004;
pub const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
pub const FILE_ATTRIBUTE_ARCHIVE: u32 = 0x0000_0020;
pub const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

/// Purpose-oriented categories produced by the rule-based classifier.
///
/// The category is *not* a deletion recommendation. The risk engine computes
/// deletion risk independently.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    WindowsSystem,
    WindowsUpdate,
    ApplicationInstallation,
    ApplicationData,
    ApplicationCache,
    TemporaryFiles,
    Logs,
    UserDocuments,
    Images,
    Videos,
    Audio,
    Archives,
    DevelopmentProjects,
    DevelopmentDependencies,
    VirtualMachines,
    DatabaseStorage,
    BrowserData,
    GameData,
    BackupData,
    CloudData,
    RecycleBin,
    Executables,
    Installers,
    Other,
    Unknown,
}

impl Category {
    pub const ALL: [Category; 25] = [
        Category::WindowsSystem,
        Category::WindowsUpdate,
        Category::ApplicationInstallation,
        Category::ApplicationData,
        Category::ApplicationCache,
        Category::TemporaryFiles,
        Category::Logs,
        Category::UserDocuments,
        Category::Images,
        Category::Videos,
        Category::Audio,
        Category::Archives,
        Category::DevelopmentProjects,
        Category::DevelopmentDependencies,
        Category::VirtualMachines,
        Category::DatabaseStorage,
        Category::BrowserData,
        Category::GameData,
        Category::BackupData,
        Category::CloudData,
        Category::RecycleBin,
        Category::Executables,
        Category::Installers,
        Category::Other,
        Category::Unknown,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::WindowsSystem => "Windows System",
            Category::WindowsUpdate => "Windows Update",
            Category::ApplicationInstallation => "Application Installation",
            Category::ApplicationData => "Application Data",
            Category::ApplicationCache => "Application Cache",
            Category::TemporaryFiles => "Temporary Files",
            Category::Logs => "Logs",
            Category::UserDocuments => "User Documents",
            Category::Images => "Images",
            Category::Videos => "Videos",
            Category::Audio => "Audio",
            Category::Archives => "Archives",
            Category::DevelopmentProjects => "Development Projects",
            Category::DevelopmentDependencies => "Development Dependencies",
            Category::VirtualMachines => "Virtual Machines",
            Category::DatabaseStorage => "Database Storage",
            Category::BrowserData => "Browser Data",
            Category::GameData => "Game Data",
            Category::BackupData => "Backup Data",
            Category::CloudData => "Cloud-Synchronized Data",
            Category::RecycleBin => "Recycle Bin",
            Category::Executables => "Executables",
            Category::Installers => "Installers",
            Category::Other => "Other",
            Category::Unknown => "Unknown / Needs Review",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Category::WindowsSystem => "windows_system",
            Category::WindowsUpdate => "windows_update",
            Category::ApplicationInstallation => "application_installation",
            Category::ApplicationData => "application_data",
            Category::ApplicationCache => "application_cache",
            Category::TemporaryFiles => "temporary_files",
            Category::Logs => "logs",
            Category::UserDocuments => "user_documents",
            Category::Images => "images",
            Category::Videos => "videos",
            Category::Audio => "audio",
            Category::Archives => "archives",
            Category::DevelopmentProjects => "development_projects",
            Category::DevelopmentDependencies => "development_dependencies",
            Category::VirtualMachines => "virtual_machines",
            Category::DatabaseStorage => "database_storage",
            Category::BrowserData => "browser_data",
            Category::GameData => "game_data",
            Category::BackupData => "backup_data",
            Category::CloudData => "cloud_data",
            Category::RecycleBin => "recycle_bin",
            Category::Executables => "executables",
            Category::Installers => "installers",
            Category::Other => "other",
            Category::Unknown => "unknown",
        }
    }

    pub fn from_key(key: &str) -> Category {
        Category::ALL
            .into_iter()
            .find(|c| c.key() == key)
            .unwrap_or(Category::Unknown)
    }

    /// RGB colour used consistently by the treemap, charts, and chips.
    pub fn color(self) -> (u8, u8, u8) {
        match self {
            Category::WindowsSystem => (100, 116, 139),
            Category::WindowsUpdate => (124, 58, 237),
            Category::ApplicationInstallation => (37, 99, 235),
            Category::ApplicationData => (8, 145, 178),
            Category::ApplicationCache => (6, 182, 212),
            Category::TemporaryFiles => (245, 158, 11),
            Category::Logs => (161, 98, 7),
            Category::UserDocuments => (22, 163, 74),
            Category::Images => (236, 72, 153),
            Category::Videos => (219, 39, 119),
            Category::Audio => (139, 92, 246),
            Category::Archives => (217, 119, 6),
            Category::DevelopmentProjects => (14, 165, 233),
            Category::DevelopmentDependencies => (2, 132, 199),
            Category::VirtualMachines => (147, 51, 234),
            Category::DatabaseStorage => (79, 70, 229),
            Category::BrowserData => (20, 184, 166),
            Category::GameData => (244, 63, 94),
            Category::BackupData => (101, 163, 13),
            Category::CloudData => (56, 189, 248),
            Category::RecycleBin => (156, 163, 175),
            Category::Executables => (71, 85, 105),
            Category::Installers => (249, 115, 22),
            Category::Other => (148, 163, 184),
            Category::Unknown => (107, 114, 128),
        }
    }

    pub fn cleanup_eligible(self) -> bool {
        matches!(
            self,
            Category::TemporaryFiles
                | Category::ApplicationCache
                | Category::Logs
                | Category::WindowsUpdate
                | Category::RecycleBin
        )
    }
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Confidence {
    High,
    Medium,
    Low,
    Unknown,
}

impl Confidence {
    pub fn label(self) -> &'static str {
        match self {
            Confidence::High => "High",
            Confidence::Medium => "Medium",
            Confidence::Low => "Low",
            Confidence::Unknown => "Unknown",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Confidence::High => "high",
            Confidence::Medium => "medium",
            Confidence::Low => "low",
            Confidence::Unknown => "unknown",
        }
    }

    pub fn from_key(key: &str) -> Confidence {
        match key {
            "high" => Confidence::High,
            "medium" => Confidence::Medium,
            "low" => Confidence::Low,
            _ => Confidence::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RiskLevel {
    Lower,
    Review,
    High,
    Critical,
    Unknown,
}

impl RiskLevel {
    pub fn label(self) -> &'static str {
        match self {
            RiskLevel::Lower => "Lower Risk",
            RiskLevel::Review => "Review Recommended",
            RiskLevel::High => "High Risk",
            RiskLevel::Critical => "Critical / Protected",
            RiskLevel::Unknown => "Unknown / Insufficient Evidence",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            RiskLevel::Lower => "Lower",
            RiskLevel::Review => "Review",
            RiskLevel::High => "High",
            RiskLevel::Critical => "Critical",
            RiskLevel::Unknown => "Unknown",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            RiskLevel::Lower => "lower",
            RiskLevel::Review => "review",
            RiskLevel::High => "high",
            RiskLevel::Critical => "critical",
            RiskLevel::Unknown => "unknown",
        }
    }

    pub fn from_key(key: &str) -> RiskLevel {
        match key {
            "lower" => RiskLevel::Lower,
            "review" => RiskLevel::Review,
            "high" => RiskLevel::High,
            "critical" => RiskLevel::Critical,
            _ => RiskLevel::Unknown,
        }
    }

    pub fn color(self) -> (u8, u8, u8) {
        match self {
            RiskLevel::Lower => (5, 150, 105),
            RiskLevel::Review => (217, 119, 6),
            RiskLevel::High => (220, 38, 38),
            RiskLevel::Critical => (153, 27, 27),
            RiskLevel::Unknown => (100, 116, 139),
        }
    }

    /// Whether the item may be offered in the ordinary cleanup assistant.
    pub fn cleanup_eligible(self) -> bool {
        matches!(self, RiskLevel::Lower | RiskLevel::Review)
    }
}

impl Default for RiskLevel {
    fn default() -> Self {
        RiskLevel::Unknown
    }
}

#[derive(Clone, Debug)]
pub struct FsNode {
    pub parent: NodeId,
    pub first_child: NodeId,
    pub next_sibling: NodeId,
    /// File or folder name. For the scan root this is the full display path.
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub allocated: u64,
    pub direct_size: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub modified: Option<i64>,
    pub created: Option<i64>,
    pub attributes: u32,
    pub category: Category,
    pub confidence: Confidence,
    pub risk: RiskLevel,
}

impl FsNode {
    pub fn is_hidden(&self) -> bool {
        self.attributes & FILE_ATTRIBUTE_HIDDEN != 0
    }
    pub fn is_system(&self) -> bool {
        self.attributes & FILE_ATTRIBUTE_SYSTEM != 0
    }
    pub fn is_readonly(&self) -> bool {
        self.attributes & FILE_ATTRIBUTE_READONLY != 0
    }
    pub fn is_reparse_point(&self) -> bool {
        self.attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    pub fn extension(&self) -> String {
        if self.is_dir {
            return String::new();
        }
        crate::core::util::extension_of(&self.name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanStatus {
    Complete,
    Partial,
    Cancelled,
    Failed,
}

impl ScanStatus {
    pub fn label(self) -> &'static str {
        match self {
            ScanStatus::Complete => "Complete",
            ScanStatus::Partial => "Completed with issues",
            ScanStatus::Cancelled => "Cancelled",
            ScanStatus::Failed => "Failed",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            ScanStatus::Complete => "complete",
            ScanStatus::Partial => "partial",
            ScanStatus::Cancelled => "cancelled",
            ScanStatus::Failed => "failed",
        }
    }
    pub fn from_key(key: &str) -> ScanStatus {
        match key {
            "complete" => ScanStatus::Complete,
            "partial" => ScanStatus::Partial,
            "cancelled" => ScanStatus::Cancelled,
            _ => ScanStatus::Failed,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ScanIssue {
    pub path: String,
    pub message: String,
}

#[derive(Clone, Debug, Default)]
pub struct VolumeInfo {
    pub root: String,
    pub label: String,
    pub filesystem: String,
    pub drive_type: String,
    pub total: u64,
    pub free: u64,
    pub used: u64,
    pub cluster_size: u64,
    pub serial: u32,
}

impl VolumeInfo {
    pub fn used_percent(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.used as f64 / self.total as f64 * 100.0
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ScanStats {
    pub total_size: u64,
    pub total_allocated: u64,
    pub files: u64,
    pub dirs: u64,
    pub errors: u64,
    pub skipped_reparse: u64,
    pub elapsed_ms: u64,
    pub largest_file: (NodeId, u64),
    pub largest_dir: (NodeId, u64),
    pub average_file_size: u64,
}

#[derive(Clone, Debug)]
pub struct ScanResult {
    pub nodes: Vec<FsNode>,
    pub root: NodeId,
    pub root_path: String,
    pub status: ScanStatus,
    pub stats: ScanStats,
    pub errors: Vec<ScanIssue>,
    pub warnings: Vec<String>,
    pub started_at: i64,
    pub finished_at: i64,
    pub volume: Option<VolumeInfo>,
}

impl ScanResult {
    pub fn node(&self, id: NodeId) -> Option<&FsNode> {
        self.nodes.get(id as usize)
    }

    pub fn parent_of(&self, id: NodeId) -> Option<NodeId> {
        let p = self.node(id)?.parent;
        (p != INVALID_NODE).then_some(p)
    }

    pub fn children(&self, id: NodeId) -> Children<'_> {
        Children {
            result: self,
            next: self
                .node(id)
                .map(|n| n.first_child)
                .unwrap_or(INVALID_NODE),
        }
    }

    /// Reconstruct the full display path without storing a path per node.
    pub fn path_of(&self, id: NodeId) -> String {
        if id == self.root {
            return self.root_path.clone();
        }
        let mut parts: Vec<&str> = Vec::new();
        let mut cur = id;
        let mut guard = 0usize;
        while cur != self.root && guard < 4096 {
            let Some(n) = self.node(cur) else { break };
            if n.parent == INVALID_NODE {
                break;
            }
            parts.push(n.name.as_str());
            cur = n.parent;
            guard += 1;
        }
        let mut path = std::path::PathBuf::from(&self.root_path);
        for part in parts.into_iter().rev() {
            path.push(part);
        }
        path.to_string_lossy().into_owned()
    }

    pub fn depth_of(&self, id: NodeId) -> usize {
        let mut depth = 0usize;
        let mut cur = id;
        while let Some(p) = self.parent_of(cur) {
            depth += 1;
            cur = p;
            if depth > 4096 {
                break;
            }
        }
        depth
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn child_ids(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut next = self
            .node(id)
            .map(|n| n.first_child)
            .unwrap_or(INVALID_NODE);
        while next != INVALID_NODE {
            let Some(n) = self.node(next) else { break };
            out.push(next);
            next = n.next_sibling;
        }
        out
    }

    /// Direct children of the root that are folders, largest first.
    pub fn root_folders(&self) -> Vec<(NodeId, u64)> {
        let mut out: Vec<(NodeId, u64)> = self
            .child_ids(self.root)
            .into_iter()
            .filter_map(|id| {
                let n = self.node(id)?;
                n.is_dir.then_some((id, n.size))
            })
            .collect();
        out.sort_by(|a, b| b.1.cmp(&a.1));
        out
    }
}

pub struct Children<'a> {
    result: &'a ScanResult,
    next: NodeId,
}

impl<'a> Iterator for Children<'a> {
    type Item = &'a FsNode;
    fn next(&mut self) -> Option<Self::Item> {
        if self.next == INVALID_NODE {
            return None;
        }
        let node = self.result.node(self.next)?;
        self.next = node.next_sibling;
        Some(node)
    }
}

#[derive(Clone, Debug, Default)]
pub struct ProgressSnapshot {
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
    pub errors: u64,
    pub skipped_reparse: u64,
    pub current_path: String,
    pub elapsed_ms: u64,
}
