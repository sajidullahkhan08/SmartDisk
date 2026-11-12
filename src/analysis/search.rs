use crate::core::models::{Category, NodeId, RiskLevel, ScanResult};
use crate::core::util::age_days;

#[derive(Clone, Debug)]
pub struct SearchFilters {
    pub query: String,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub older_than_days: Option<u64>,
    pub newer_than_days: Option<u64>,
    pub category: Option<Category>,
    pub risk: Option<RiskLevel>,
    pub files_only: bool,
    pub folders_only: bool,
    pub extension: Option<String>,
    pub limit: usize,
}

impl Default for SearchFilters {
    fn default() -> Self {
        Self {
            query: String::new(),
            min_size: None,
            max_size: None,
            older_than_days: None,
            newer_than_days: None,
            category: None,
            risk: None,
            files_only: false,
            folders_only: false,
            extension: None,
            limit: 2000,
        }
    }
}

/// Search the in-memory scan result. File contents are never read.
///
/// A query without path separators matches item names. A query containing `\`
/// or `/` is matched against reconstructed full paths, so pasted paths work.
pub fn search(result: &ScanResult, filters: &SearchFilters, now: i64) -> Vec<NodeId> {
    let query = filters.query.trim().to_ascii_lowercase();
    let path_query = query.contains('\\') || query.contains('/');
    let extension = filters
        .extension
        .as_ref()
        .map(|e| e.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|e| !e.is_empty());
    let limit = if filters.limit == 0 { 2000 } else { filters.limit };

    let mut matches: Vec<NodeId> = Vec::new();
    for id in 1..result.nodes.len() as NodeId {
        let node = &result.nodes[id as usize];
        if filters.files_only && node.is_dir {
            continue;
        }
        if filters.folders_only && !node.is_dir {
            continue;
        }
        if let Some(min) = filters.min_size {
            if node.size < min {
                continue;
            }
        }
        if let Some(max) = filters.max_size {
            if node.size > max {
                continue;
            }
        }
        if let Some(category) = filters.category {
            if node.category != category {
                continue;
            }
        }
        if let Some(risk) = filters.risk {
            if node.risk != risk {
                continue;
            }
        }
        if let Some(ext) = &extension {
            if node.is_dir || node.extension() != *ext {
                continue;
            }
        }
        if filters.older_than_days.is_some() || filters.newer_than_days.is_some() {
            let Some(modified) = node.modified else { continue };
            let age = age_days(Some(modified), now).unwrap_or(0);
            if let Some(days) = filters.older_than_days {
                if age < days {
                    continue;
                }
            }
            if let Some(days) = filters.newer_than_days {
                if age > days {
                    continue;
                }
            }
        }
        if !query.is_empty() {
            let matched = if path_query {
                let path = result.path_of(id).to_ascii_lowercase();
                path_query_matches(&path, &query)
            } else {
                contains_ci(&node.name, &query)
            };
            if !matched {
                continue;
            }
        }
        matches.push(id);
    }

    matches.sort_by(|a, b| {
        let na = &result.nodes[*a as usize];
        let nb = &result.nodes[*b as usize];
        nb.size
            .cmp(&na.size)
            .then_with(|| na.name.to_lowercase().cmp(&nb.name.to_lowercase()))
    });
    matches.truncate(limit);
    matches
}

fn path_query_matches(path_lower: &str, query_lower: &str) -> bool {
    // A trailing separator means "this folder and everything below it".
    if query_lower.ends_with('\\') || query_lower.ends_with('/') {
        let prefix = query_lower.trim_end_matches(['\\', '/']);
        if prefix.is_empty() {
            return true;
        }
        return path_lower == prefix
            || path_lower.starts_with(query_lower)
            || (prefix.len() < path_lower.len()
                && path_lower.starts_with(prefix)
                && matches!(path_lower.as_bytes()[prefix.len()], b'\\' | b'/'));
    }
    path_lower.contains(query_lower)
}

fn contains_ci(haystack: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return true;
    }
    if needle_lower.is_ascii() {
        let needle = needle_lower.as_bytes();
        haystack
            .as_bytes()
            .windows(needle.len())
            .any(|w| w.eq_ignore_ascii_case(needle))
    } else {
        haystack.to_lowercase().contains(needle_lower)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{run_scan, ScanControl, ScanOptions, ScanProgress};
    use std::fs;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn setup() -> (tempfile::TempDir, ScanResult) {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("Photos")).unwrap();
        fs::write(dir.path().join("Photos/cat.jpg"), vec![1u8; 100]).unwrap();
        fs::write(dir.path().join("Photos/dog.jpg"), vec![1u8; 200]).unwrap();
        fs::write(dir.path().join("report.pdf"), vec![1u8; 300]).unwrap();
        fs::write(dir.path().join("archive.zip"), vec![1u8; 400]).unwrap();
        let result = run_scan(
            ScanOptions::new(dir.path().to_path_buf()),
            Arc::new(ScanControl::new()),
            Arc::new(ScanProgress::new()),
        )
        .unwrap();
        (dir, result)
    }

    #[test]
    fn searches_names_case_insensitively() {
        let (_dir, result) = setup();
        let filters = SearchFilters {
            query: "CAT".into(),
            ..Default::default()
        };
        let hits = search(&result, &filters, crate::core::util::now_unix());
        assert_eq!(hits.len(), 1);
        assert_eq!(result.node(hits[0]).unwrap().name, "cat.jpg");
    }

    #[test]
    fn size_and_kind_filters() {
        let (_dir, result) = setup();
        let filters = SearchFilters {
            min_size: Some(250),
            files_only: true,
            ..Default::default()
        };
        let hits = search(&result, &filters, crate::core::util::now_unix());
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().all(|id| !result.node(*id).unwrap().is_dir));
    }

    #[test]
    fn extension_filter() {
        let (_dir, result) = setup();
        let filters = SearchFilters {
            extension: Some(".JPG".into()),
            ..Default::default()
        };
        let hits = search(&result, &filters, crate::core::util::now_unix());
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn path_query_matches_full_paths() {
        let (dir, result) = setup();
        let q = format!("{}\\Photos\\", dir.path().to_string_lossy());
        let filters = SearchFilters {
            query: q,
            ..Default::default()
        };
        let hits = search(&result, &filters, crate::core::util::now_unix());
        assert_eq!(hits.len(), 3); // Photos dir + two files
    }

    #[test]
    fn invalid_dates_are_excluded_when_filtering() {
        let (_dir, result) = setup();
        let filters = SearchFilters {
            older_than_days: Some(1),
            ..Default::default()
        };
        let hits = search(&result, &filters, crate::core::util::now_unix());
        assert!(hits.is_empty());
    }
}
