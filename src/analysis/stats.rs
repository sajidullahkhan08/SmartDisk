use crate::analysis::file_types::category_stats;
use crate::core::models::{NodeId, ScanResult, INVALID_NODE};
use crate::core::util::percent;

#[derive(Clone, Debug)]
pub struct OverviewStats {
    pub total_size: u64,
    pub total_allocated: u64,
    pub files: u64,
    pub dirs: u64,
    pub errors: u64,
    pub skipped_reparse: u64,
    pub elapsed_ms: u64,
    pub average_file_size: u64,
    pub largest_file: Option<(NodeId, String, u64)>,
    pub largest_folder: Option<(NodeId, String, u64)>,
    pub top_level: Vec<TopLevelEntry>,
    pub category_count: usize,
}

#[derive(Clone, Debug)]
pub struct TopLevelEntry {
    pub id: Option<NodeId>,
    pub name: String,
    pub size: u64,
    pub percent: f64,
    pub is_dir: bool,
}

pub fn overview(result: &ScanResult) -> OverviewStats {
    let root = &result.nodes[result.root as usize];
    let mut top_level = Vec::new();
    for id in result.child_ids(result.root) {
        let node = &result.nodes[id as usize];
        if node.is_dir {
            top_level.push(TopLevelEntry {
                id: Some(id),
                name: node.name.clone(),
                size: node.size,
                percent: percent(node.size, root.size),
                is_dir: true,
            });
        }
    }
    if root.direct_size > 0 {
        top_level.push(TopLevelEntry {
            id: None,
            name: "Files directly in this location".into(),
            size: root.direct_size,
            percent: percent(root.direct_size, root.size),
            is_dir: false,
        });
    }
    top_level.sort_by(|a, b| b.size.cmp(&a.size));

    let largest_file = if result.stats.largest_file.0 != INVALID_NODE {
        let id = result.stats.largest_file.0;
        let node = &result.nodes[id as usize];
        Some((
            id,
            result.path_of(id),
            node.size,
        ))
    } else {
        None
    };
    let largest_folder = if result.stats.largest_dir.0 != INVALID_NODE {
        let id = result.stats.largest_dir.0;
        let node = &result.nodes[id as usize];
        Some((
            id,
            result.path_of(id),
            node.size,
        ))
    } else {
        None
    };

    OverviewStats {
        total_size: result.stats.total_size,
        total_allocated: result.stats.total_allocated,
        files: result.stats.files,
        dirs: result.stats.dirs,
        errors: result.stats.errors,
        skipped_reparse: result.stats.skipped_reparse,
        elapsed_ms: result.stats.elapsed_ms,
        average_file_size: result.stats.average_file_size,
        largest_file,
        largest_folder,
        category_count: category_stats(result).len(),
        top_level,
    }
}

/// Verifies that recursive folder sizes are arithmetically consistent.
pub fn verify_aggregation(result: &ScanResult) -> Vec<(NodeId, u64, u64)> {
    let mut problems = Vec::new();
    for id in 1..result.nodes.len() as NodeId {
        let node = &result.nodes[id as usize];
        if !node.is_dir {
            continue;
        }
        let mut sum = node.direct_size;
        for child in result.child_ids(id) {
            let child_node = &result.nodes[child as usize];
            // `direct_size` already accounts for files directly in this folder;
            // only child folders add to the remainder.
            if child_node.is_dir {
                sum = sum.saturating_add(child_node.size);
            }
        }
        if sum != node.size {
            problems.push((id, node.size, sum));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::file_types::extension_stats;
    use crate::core::models::{
        Category, Confidence, FsNode, RiskLevel, ScanStats, ScanStatus,
    };
    use crate::scanner::{run_scan, ScanControl, ScanOptions, ScanProgress};
    use std::fs;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn synthetic(files: &[(&str, u64, Category)]) -> ScanResult {
        let total: u64 = files.iter().map(|f| f.1).sum();
        let root = FsNode {
            parent: INVALID_NODE,
            first_child: INVALID_NODE,
            next_sibling: INVALID_NODE,
            name: "C:\\synthetic".into(),
            is_dir: true,
            size: total,
            allocated: total,
            direct_size: total,
            file_count: files.len() as u64,
            dir_count: 0,
            modified: None,
            created: None,
            attributes: 0,
            category: Category::Other,
            confidence: Confidence::Low,
            risk: RiskLevel::Unknown,
        };
        let mut nodes = vec![root];
        let mut previous = INVALID_NODE;
        for (index, (name, size, category)) in files.iter().enumerate() {
            let id = (index + 1) as NodeId;
            if index == 0 {
                nodes[0].first_child = id;
            } else {
                nodes[previous as usize].next_sibling = id;
            }
            nodes.push(FsNode {
                parent: 0,
                first_child: INVALID_NODE,
                next_sibling: INVALID_NODE,
                name: (*name).into(),
                is_dir: false,
                size: *size,
                allocated: *size,
                direct_size: *size,
                file_count: 0,
                dir_count: 0,
                modified: None,
                created: None,
                attributes: 0,
                category: *category,
                confidence: Confidence::Low,
                risk: RiskLevel::Unknown,
            });
            previous = id;
        }
        ScanResult {
            nodes,
            root: 0,
            root_path: "C:\\synthetic".into(),
            status: ScanStatus::Complete,
            stats: ScanStats {
                total_size: total,
                files: files.len() as u64,
                ..Default::default()
            },
            errors: Vec::new(),
            warnings: Vec::new(),
            started_at: 0,
            finished_at: 0,
            volume: None,
        }
    }

    fn result_for(dir: &std::path::Path) -> ScanResult {
        run_scan(
            ScanOptions::new(dir.to_path_buf()),
            Arc::new(ScanControl::new()),
            Arc::new(ScanProgress::new()),
        )
        .unwrap()
    }

    #[test]
    fn overview_sums_and_consistency() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("media")).unwrap();
        fs::write(dir.path().join("a.txt"), vec![1u8; 100]).unwrap();
        fs::write(dir.path().join("media/b.png"), vec![1u8; 200]).unwrap();
        fs::write(dir.path().join("media/c.mp4"), vec![1u8; 300]).unwrap();
        let result = result_for(dir.path());
        let overview = overview(&result);
        assert_eq!(overview.total_size, 600);
        assert_eq!(overview.files, 3);
        assert_eq!(overview.largest_file.as_ref().unwrap().2, 300);
        assert!(overview.top_level.iter().any(|t| t.name == "media" && t.size == 500));
        assert!(
            verify_aggregation(&result).is_empty(),
            "aggregation problems: {:?}",
            verify_aggregation(&result)
        );
    }

    #[test]
    fn category_sizes_sum_to_total_file_size() {
        let result = synthetic(&[
            ("a.txt", 10, Category::UserDocuments),
            ("b.png", 20, Category::Images),
            ("c.png", 5, Category::Images),
        ]);
        let stats = category_stats(&result);
        let sum: u64 = stats.iter().map(|s| s.size).sum();
        assert_eq!(sum, 35);
        let images = stats
            .iter()
            .find(|s| s.category == Category::Images)
            .unwrap();
        assert_eq!(images.size, 25);
        assert_eq!(images.count, 2);
    }

    #[test]
    fn extension_stats_aggregate() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), vec![1u8; 10]).unwrap();
        fs::write(dir.path().join("b.TXT"), vec![1u8; 15]).unwrap();
        fs::write(dir.path().join("c"), vec![1u8; 5]).unwrap();
        let result = result_for(dir.path());
        let exts = extension_stats(&result, 10);
        let txt = exts.iter().find(|e| e.extension == "txt").unwrap();
        assert_eq!(txt.count, 2);
        assert_eq!(txt.size, 25);
        assert!(exts.iter().any(|e| e.extension == "(none)" && e.size == 5));
    }
}
