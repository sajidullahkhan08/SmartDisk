use std::collections::HashMap;

use crate::core::models::{Category, ScanResult};
use crate::core::util::extension_of;

#[derive(Clone, Debug)]
pub struct CategoryStat {
    pub category: Category,
    pub count: u64,
    pub size: u64,
}

#[derive(Clone, Debug)]
pub struct ExtensionStat {
    pub extension: String,
    pub count: u64,
    pub size: u64,
}

/// File-type distribution by likely purpose. Sizes are summed over files only
/// so category totals add up to the scanned file size without double counting.
pub fn category_stats(result: &ScanResult) -> Vec<CategoryStat> {
    let mut counts = [0u64; Category::ALL.len()];
    let mut sizes = [0u64; Category::ALL.len()];
    for node in result.nodes.iter().skip(1) {
        if node.is_dir {
            continue;
        }
        let idx = Category::ALL
            .iter()
            .position(|c| *c == node.category)
            .unwrap_or(Category::ALL.len() - 1);
        counts[idx] += 1;
        sizes[idx] = sizes[idx].saturating_add(node.size);
    }
    let mut out: Vec<CategoryStat> = Category::ALL
        .iter()
        .enumerate()
        .filter(|(i, _)| counts[*i] > 0)
        .map(|(i, c)| CategoryStat {
            category: *c,
            count: counts[i],
            size: sizes[i],
        })
        .collect();
    out.sort_by(|a, b| b.size.cmp(&a.size));
    out
}

pub fn extension_stats(result: &ScanResult, limit: usize) -> Vec<ExtensionStat> {
    let mut map: HashMap<String, (u64, u64)> = HashMap::new();
    for node in result.nodes.iter().skip(1) {
        if node.is_dir {
            continue;
        }
        let ext = extension_of(&node.name);
        let key = if ext.is_empty() { "(none)".to_string() } else { ext };
        let entry = map.entry(key).or_insert((0, 0));
        entry.0 += 1;
        entry.1 = entry.1.saturating_add(node.size);
    }
    let mut out: Vec<ExtensionStat> = map
        .into_iter()
        .map(|(extension, (count, size))| ExtensionStat {
            extension,
            count,
            size,
        })
        .collect();
    out.sort_by(|a, b| b.size.cmp(&a.size).then(a.extension.cmp(&b.extension)));
    out.truncate(limit);
    out
}
