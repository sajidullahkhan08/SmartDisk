//! Aggregation, treemap, search, cleanup-plan and duplicate integration tests.

mod common;

use std::sync::Arc;

use common::{find_path, fixture_root, remove_fixture, scan, write_file};
use smartdisk::analysis::duplicates::{describe_group, find_duplicates, DuplicateProgress, DuplicateStage};
use smartdisk::analysis::file_types::{category_stats, extension_stats};
use smartdisk::analysis::search::{search, SearchFilters};
use smartdisk::analysis::stats::{overview, verify_aggregation};
use smartdisk::analysis::treemap::squarify;
use smartdisk::cleanup::{build_plan, estimate_recovery, recommendations, total_recommended};
use smartdisk::core::models::{Category, NodeId, RiskLevel, ScanResult};

fn mixed_fixture(name: &str) -> std::path::PathBuf {
    let root = fixture_root(name);
    write_file(&root.join("Images").join("photo.jpg"), 800);
    write_file(&root.join("Images").join("photo-copy.jpg"), 800);
    write_file(&root.join("Documents").join("report.docx"), 400);
    write_file(&root.join("Temp").join("cache.tmp"), 200);
    write_file(&root.join("Deep").join("a").join("b").join("c.bin"), 100);
    write_file(&root.join("root.txt"), 50);
    root
}

#[test]
fn overview_reports_totals_and_top_level_shares() {
    let root = mixed_fixture("overview");
    let result = scan(&root);
    let stats = overview(&result);
    assert_eq!(stats.files, 6);
    assert_eq!(stats.dirs, 6);
    assert_eq!(stats.total_size, 2350);
    assert_eq!(stats.average_file_size, 2350 / 6);
    assert_eq!(stats.top_level.first().unwrap().name, "Images");
    assert_eq!(stats.top_level.first().unwrap().size, 1600);
    assert!(stats.top_level.iter().any(|entry| entry.name == "Files directly in this location" && entry.size == 50));
    let share: f64 = stats.top_level.iter().map(|entry| entry.percent).sum();
    assert!((share - 100.0).abs() < 0.01, "shares should add up, got {share}");
    remove_fixture(&root);
}

#[test]
fn folder_sums_are_consistent_with_children() {
    let root = mixed_fixture("verify");
    let result = scan(&root);
    assert!(verify_aggregation(&result).is_empty());

    let images = find_path(&result, "Images").unwrap();
    let folder = result.node(images).unwrap();
    let child_sum: u64 = result
        .child_ids(images)
        .into_iter()
        .map(|id| result.node(id).unwrap().size)
        .sum();
    assert_eq!(folder.size, child_sum);
    assert_eq!(folder.file_count, 2);
    remove_fixture(&root);
}

#[test]
fn category_and_extension_totals_add_up() {
    let root = mixed_fixture("categories");
    let result = scan(&root);
    let categories = category_stats(&result);
    let total: u64 = categories.iter().map(|stat| stat.size).sum();
    assert_eq!(total, result.stats.total_size);
    let images = categories
        .iter()
        .find(|stat| stat.category == Category::Images)
        .expect("images category");
    assert_eq!(images.size, 1600);
    assert_eq!(images.count, 2);

    let extensions = extension_stats(&result, 10);
    let jpg = extensions.iter().find(|stat| stat.extension == "jpg").unwrap();
    assert_eq!(jpg.count, 2);
    assert_eq!(jpg.size, 1600);
    let missing = extensions.iter().find(|stat| stat.extension == "zzz");
    assert!(missing.is_none());
    remove_fixture(&root);
}

#[test]
fn treemap_areas_are_proportional_to_folder_sizes() {
    let root = mixed_fixture("treemap");
    let result = scan(&root);
    let children: Vec<NodeId> = result.child_ids(result.root);
    let sizes: Vec<f64> = children
        .iter()
        .map(|id| result.node(*id).unwrap().size as f64)
        .collect();
    let total: f64 = sizes.iter().sum();
    let rects = squarify(&sizes);
    assert_eq!(rects.len(), sizes.len());
    let area: f64 = rects.iter().map(|r| r.area()).sum();
    assert!((area - 1.0).abs() < 1e-6);
    for (rect, size) in rects.iter().zip(sizes.iter()) {
        assert!((rect.area() - size / total).abs() < 1e-6);
        assert!(rect.x >= -1e-9 && rect.y >= -1e-9);
        assert!(rect.right() <= 1.0 + 1e-9 && rect.bottom() <= 1.0 + 1e-9);
    }
    remove_fixture(&root);
}

#[test]
fn search_matches_names_extensions_categories_and_sizes() {
    let root = mixed_fixture("search");
    let result = scan(&root);
    let now = smartdisk::core::util::now_unix();

    let by_name = search(
        &result,
        &SearchFilters {
            query: "PHOTO".into(),
            ..Default::default()
        },
        now,
    );
    assert_eq!(by_name.len(), 2);

    let by_extension = search(
        &result,
        &SearchFilters {
            extension: Some(".jpg".into()),
            ..Default::default()
        },
        now,
    );
    assert_eq!(by_extension.len(), 2);
    assert!(by_extension
        .iter()
        .all(|id| result.node(*id).unwrap().name.ends_with(".jpg")));

    let by_category = search(
        &result,
        &SearchFilters {
            category: Some(Category::Images),
            ..Default::default()
        },
        now,
    );
    assert_eq!(by_category.len(), 2);

    let by_size = search(
        &result,
        &SearchFilters {
            min_size: Some(700),
            files_only: true,
            ..Default::default()
        },
        now,
    );
    assert_eq!(by_size.len(), 2);

    let folders = search(
        &result,
        &SearchFilters {
            folders_only: true,
            query: "deep".into(),
            ..Default::default()
        },
        now,
    );
    assert_eq!(folders.len(), 1);

    let limited = search(
        &result,
        &SearchFilters {
            limit: 1,
            files_only: true,
            ..Default::default()
        },
        now,
    );
    assert_eq!(limited.len(), 1);
    assert_eq!(result.node(limited[0]).unwrap().size, 800, "largest first");

    let none = search(
        &result,
        &SearchFilters {
            query: "no-such-item".into(),
            ..Default::default()
        },
        now,
    );
    assert!(none.is_empty());
    remove_fixture(&root);
}

#[test]
fn path_search_matches_subtrees_and_trailing_separator() {
    let root = mixed_fixture("pathsearch");
    let result = scan(&root);
    let now = smartdisk::core::util::now_unix();
    let folder_query = format!("{}\\Images", result.root_path);
    let hits = search(
        &result,
        &SearchFilters {
            query: folder_query,
            ..Default::default()
        },
        now,
    );
    // The folder itself plus the two files below it.
    assert_eq!(hits.len(), 3);
    let subtree_query = format!("{}\\Deep\\", result.root_path);
    let subtree = search(
        &result,
        &SearchFilters {
            query: subtree_query,
            ..Default::default()
        },
        now,
    );
    assert!(subtree.len() >= 3, "Deep, a, b and c.bin live in the subtree");
    remove_fixture(&root);
}

#[test]
fn cleanup_recommendations_and_plan_are_conservative() {
    let root = mixed_fixture("cleanup");
    let result = scan(&root);
    let groups = recommendations(&result);
    let (size, count) = total_recommended(&groups);
    assert!(size >= 200, "the .tmp file should be recommended");
    assert!(count >= 1);
    assert!(groups
        .iter()
        .all(|group| group.items.iter().all(|item| item.risk.cleanup_eligible())));
    // Photos are user data and must never be offered automatically.
    assert!(groups
        .iter()
        .flat_map(|group| group.items.iter())
        .all(|item| !item.name.ends_with(".jpg")));

    let candidates: Vec<NodeId> = groups
        .iter()
        .flat_map(|group| group.items.iter().map(|item| item.node))
        .collect();
    let plan = build_plan(&result, &candidates, true);
    assert!(!plan.items.is_empty());
    assert!(plan.blocked.is_empty());
    assert!(plan.estimate.warnings[0].contains("Recycle Bin"));
    assert!((plan.estimate.logical_bytes as i64 - plan.total_size as i64).abs() == 0);
    let estimate = estimate_recovery(&result, &candidates);
    assert_eq!(estimate.logical_bytes, plan.total_size);
    remove_fixture(&root);
}

#[test]
fn cleanup_plan_dedupes_selection_and_blocks_critical_nodes() {
    let root = mixed_fixture("plan");
    let mut result: ScanResult = scan(&root);
    let tmp = find_path(&result, "Temp/cache.tmp").unwrap();
    let photo = find_path(&result, "Images/photo.jpg").unwrap();

    result.nodes[tmp as usize].risk = RiskLevel::Critical;
    result.nodes[photo as usize].risk = RiskLevel::Unknown;
    let plan = build_plan(&result, &[tmp, tmp, photo, photo], true);
    assert_eq!(plan.blocked, vec![tmp]);
    assert_eq!(plan.unknown, vec![photo]);
    assert_eq!(plan.items.len(), 1);
    assert_eq!(plan.items[0].node, photo);
    assert_eq!(plan.total_size, 800);
    remove_fixture(&root);
}

#[test]
fn duplicate_detection_uses_content_not_names() {
    let root = fixture_root("dupes-content");
    let payload = vec![7u8; 2048];
    std::fs::create_dir_all(root.join("one")).unwrap();
    std::fs::create_dir_all(root.join("two")).unwrap();
    std::fs::write(root.join("one").join("alpha.bin"), &payload).unwrap();
    std::fs::write(root.join("two").join("beta.bin"), &payload).unwrap();
    std::fs::write(root.join("different.bin"), vec![9u8; 2048]).unwrap();
    std::fs::write(root.join("small.bin"), vec![7u8; 16]).unwrap();

    let result = scan(&root);
    let progress = Arc::new(DuplicateProgress::new());
    let groups = find_duplicates(&result, 1, progress.clone());
    assert_eq!(groups.len(), 1, "only alpha and beta share content");
    let group = &groups[0];
    assert_eq!(group.files.len(), 2);
    assert_eq!(group.size, 2048);
    assert_eq!(group.recoverable, 2048);
    assert!(describe_group(group).contains("2 files"));
    assert_eq!(progress.stage(), DuplicateStage::Complete);

    let filtered = find_duplicates(&result, 4096, Arc::new(DuplicateProgress::new()));
    assert!(filtered.is_empty());
    remove_fixture(&root);
}

#[test]
fn cancelled_duplicate_analysis_returns_nothing() {
    let root = mixed_fixture("dupes-cancel");
    let result = scan(&root);
    let progress = Arc::new(DuplicateProgress::new());
    progress.cancel();
    let groups = find_duplicates(&result, 1, progress.clone());
    assert!(groups.is_empty());
    assert_eq!(progress.stage(), DuplicateStage::Cancelled);
    remove_fixture(&root);
}

#[test]
fn percentages_never_divide_by_zero() {
    let root = fixture_root("percent");
    std::fs::create_dir_all(root.join("empty")).unwrap();
    std::fs::write(root.join("zero.bin"), []).unwrap();
    let result = scan(&root);
    let stats = overview(&result);
    for entry in &stats.top_level {
        assert!(entry.percent.is_finite());
        assert!(entry.percent >= 0.0);
    }
    remove_fixture(&root);
}
