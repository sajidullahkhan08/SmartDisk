//! Cleanup recommendations, preview and recovered-space estimation.
//!
//! Recommendations are based on recognized locations and the independent risk
//! engine. Nothing is ever deleted automatically.

use crate::core::models::{Category, NodeId, RiskLevel, ScanResult};
use crate::core::util::format_size;
use crate::intelligence::PathContext;

#[derive(Clone, Debug)]
pub struct CleanupCandidate {
    pub node: NodeId,
    pub name: String,
    pub path: String,
    pub size: u64,
    pub category: Category,
    pub risk: RiskLevel,
    pub modified: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct CleanupGroup {
    pub key: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub category: Category,
    pub size: u64,
    pub count: u64,
    pub items: Vec<CleanupCandidate>,
    pub truncated: bool,
}

impl CleanupGroup {
    pub fn color(&self) -> (u8, u8, u8) {
        self.category.color()
    }
}

#[derive(Clone, Debug, Default)]
pub struct RecoveryEstimate {
    pub logical_bytes: u64,
    pub summary: String,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct CleanupPlan {
    pub items: Vec<CleanupCandidate>,
    pub total_size: u64,
    pub estimate: RecoveryEstimate,
    pub use_recycle_bin: bool,
    pub blocked: Vec<NodeId>,
    pub unknown: Vec<NodeId>,
}

const MAX_ITEMS_PER_GROUP: usize = 1500;

/// Build cleanup groups from files in recognized temp/cache/log locations.
/// Directories are not offered as automatic candidates to avoid double-counting
/// their contents. Unknown and protected items are never included.
pub fn recommendations(result: &ScanResult) -> Vec<CleanupGroup> {
    let mut groups: Vec<CleanupGroup> = Vec::new();
    let definitions = [
        (
            "temporary",
            "Temporary files",
            "Files in recognized temporary directories. Some may be in use by running applications.",
            Category::TemporaryFiles,
        ),
        (
            "caches",
            "Application and browser caches",
            "Recognized cache data. Applications may recreate it or reload resources.",
            Category::ApplicationCache,
        ),
        (
            "logs",
            "Log files",
            "Log files in recognized locations. Recent logs can help diagnose problems.",
            Category::Logs,
        ),
        (
            "windows-update",
            "Windows Update downloads",
            "Update staging data. Windows may need to download the update again.",
            Category::WindowsUpdate,
        ),
        (
            "recycle-bin",
            "Recycle Bin contents",
            "Items deleted earlier. Removing them from the Recycle Bin makes restoration impossible.",
            Category::RecycleBin,
        ),
    ];

    for (key, title, description, category) in definitions {
        let mut items = Vec::new();
        let mut total_size = 0u64;
        let mut count = 0u64;
        for id in 1..result.nodes.len() as NodeId {
            let node = &result.nodes[id as usize];
            if node.is_dir
                || node.category != category
                || !node.risk.cleanup_eligible()
                || node.size == 0
            {
                continue;
            }
            let path = result.path_of(id);
            // Candidates are files, so protect them when their containing
            // folder is a protected location (e.g. C:\Windows).
            let candidate_path = std::path::Path::new(&path);
            let context_path = candidate_path.parent().unwrap_or(candidate_path);
            if PathContext::new(context_path).is_critical_location() {
                continue;
            }
            total_size = total_size.saturating_add(node.size);
            count += 1;
            if items.len() < MAX_ITEMS_PER_GROUP {
                items.push(CleanupCandidate {
                    node: id,
                    name: node.name.clone(),
                    path,
                    size: node.size,
                    category: node.category,
                    risk: node.risk,
                    modified: node.modified,
                });
            }
        }
        if count > 0 {
            items.sort_by(|a, b| b.size.cmp(&a.size));
            groups.push(CleanupGroup {
                key,
                title,
                description,
                category,
                size: total_size,
                count,
                items,
                truncated: count > MAX_ITEMS_PER_GROUP as u64,
            });
        }
    }
    groups.sort_by(|a, b| b.size.cmp(&a.size));
    groups
}

pub fn total_recommended(groups: &[CleanupGroup]) -> (u64, u64) {
    groups.iter().fold((0u64, 0u64), |(size, count), g| {
        (size.saturating_add(g.size), count + g.count)
    })
}

/// Build a preview/plan for explicitly selected nodes. The caller always has to
/// confirm before anything is moved.
pub fn build_plan(result: &ScanResult, selected: &[NodeId], use_recycle_bin: bool) -> CleanupPlan {
    let mut seen = std::collections::HashSet::new();
    let mut items = Vec::new();
    let mut blocked = Vec::new();
    let mut unknown = Vec::new();
    let mut total = 0u64;
    for &id in selected {
        if !seen.insert(id) {
            continue;
        }
        let Some(node) = result.node(id) else { continue };
        match node.risk {
            RiskLevel::Critical => {
                blocked.push(id);
                continue;
            }
            RiskLevel::Unknown => {
                unknown.push(id);
            }
            _ => {}
        }
        if node.is_dir {
            // Selected directories are counted once; descendants are not added.
            total = total.saturating_add(node.size);
        } else {
            total = total.saturating_add(node.size);
        }
        items.push(CleanupCandidate {
            node: id,
            name: node.name.clone(),
            path: result.path_of(id),
            size: node.size,
            category: node.category,
            risk: node.risk,
            modified: node.modified,
        });
    }
    items.sort_by(|a, b| b.size.cmp(&a.size));

    let mut warnings = Vec::new();
    if use_recycle_bin {
        warnings.push(
            "Items will be moved to the Recycle Bin. Disk space is not freed until the Recycle Bin is emptied."
                .to_string(),
        );
    } else {
        warnings.push(
            "Permanent deletion cannot be undone and does not use the Recycle Bin.".to_string(),
        );
    }
    warnings.push(
        "Recovered space can differ from the logical size because of hard links, compression, sparse files, allocation units, and concurrent changes."
            .to_string(),
    );
    if !blocked.is_empty() {
        warnings.push(format!(
            "{} protected item(s) were excluded from this plan.",
            blocked.len()
        ));
    }
    if !unknown.is_empty() {
        warnings.push(format!(
            "{} item(s) have insufficient evidence. Explicit confirmation is required.",
            unknown.len()
        ));
    }

    let summary = format!(
        "Logical size of the selection: {}. This is an estimate, not a guarantee of recovered space.",
        format_size(total)
    );
    CleanupPlan {
        items,
        total_size: total,
        estimate: RecoveryEstimate {
            logical_bytes: total,
            summary,
            warnings,
        },
        use_recycle_bin,
        blocked,
        unknown,
    }
}

pub fn estimate_recovery(result: &ScanResult, selected: &[NodeId]) -> RecoveryEstimate {
    build_plan(result, selected, true).estimate
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{run_scan, ScanControl, ScanOptions, ScanProgress};
    use std::fs;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn scan(dir: &std::path::Path) -> ScanResult {
        run_scan(
            ScanOptions::new(dir.to_path_buf()),
            Arc::new(ScanControl::new()),
            Arc::new(ScanProgress::new()),
        )
        .unwrap()
    }

    #[test]
    fn temp_directory_files_are_candidates() {
        // tempfile creates its directory under %LOCALAPPDATA%\Temp, a recognized
        // temporary location for the classifier.
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("leftover.tmp"), vec![1u8; 1024]).unwrap();
        let result = scan(dir.path());
        let groups = recommendations(&result);
        let total: u64 = groups.iter().map(|g| g.size).sum();
        assert!(total >= 1024, "groups: {groups:?}");
        assert!(groups.iter().any(|g| g.category == Category::TemporaryFiles));
    }

    #[test]
    fn critical_items_are_never_recommended() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("x.tmp"), vec![1u8; 10]).unwrap();
        let result = scan(dir.path());
        let groups = recommendations(&result);
        assert!(groups.iter().all(|g| g.items.iter().all(|i| i.risk.cleanup_eligible())));
    }

    #[test]
    fn plan_sums_selection_and_warns_about_recycle_bin() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.tmp"), vec![1u8; 100]).unwrap();
        fs::write(dir.path().join("b.tmp"), vec![1u8; 200]).unwrap();
        let result = scan(dir.path());
        let ids: Vec<NodeId> = result
            .nodes
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, n)| !n.is_dir)
            .map(|(i, _)| i as NodeId)
            .collect();
        let plan = build_plan(&result, &ids, true);
        assert_eq!(plan.total_size, 300);
        assert!(plan
            .estimate
            .warnings
            .iter()
            .any(|w| w.contains("Recycle Bin")));
    }

    #[test]
    fn protected_nodes_are_blocked_from_plans() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("safe.tmp"), vec![1u8; 10]).unwrap();
        let mut result = scan(dir.path());
        let id = 1u32;
        if result.nodes.len() > 1 {
            result.nodes[id as usize].risk = RiskLevel::Critical;
        }
        let plan = build_plan(&result, &[id], true);
        assert_eq!(plan.blocked, vec![id]);
        assert_eq!(plan.total_size, 0);
    }

    #[test]
    fn estimate_is_honest_about_limits() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.tmp"), vec![1u8; 50]).unwrap();
        let result = scan(dir.path());
        let est = estimate_recovery(&result, &[1]);
        assert!(est.summary.contains("estimate"));
        assert!(est.warnings.iter().any(|w| w.contains("hard links")));
    }
}
