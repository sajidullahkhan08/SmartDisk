//! End-to-end flow: scan, inspect, search, plan, operate, rescan.
//!
//! This mirrors the manual UI walkthrough without Slint: it exercises the same
//! core functions the window calls, on an isolated fixture tree.

mod common;

use std::fs;
use std::sync::Arc;

use common::{find_path, fixture_root, remove_fixture, scan, write_file};
use smartdisk::analysis::duplicates::find_duplicates;
use smartdisk::analysis::duplicates::DuplicateProgress;
use smartdisk::analysis::search::{search, SearchFilters};
use smartdisk::analysis::stats::{overview, verify_aggregation};
use smartdisk::analysis::treemap::squarify;
use smartdisk::cleanup::{build_plan, recommendations};
use smartdisk::core::models::{Category, RiskLevel, ScanStatus};
use smartdisk::file_operations::operations::{copy_items, delete_to_recycle_bin, rename_item};
use smartdisk::intelligence::{assess_detailed, classify_detailed};

fn build_project(name: &str) -> std::path::PathBuf {
    let root = fixture_root(name);
    write_file(&root.join("Documents").join("report.docx"), 4000);
    let photo_bytes = vec![0xABu8; 8000];
    fs::create_dir_all(root.join("Images")).unwrap();
    fs::write(root.join("Images").join("photo.jpg"), &photo_bytes).unwrap();
    fs::write(root.join("Images").join("photo-copy.jpg"), &photo_bytes).unwrap();
    write_file(&root.join("Temp").join("cache.tmp"), 2000);
    write_file(&root.join("Archive").join("data.zip"), 7000);
    write_file(&root.join("project").join("node_modules").join("lib").join("index.js"), 512);
    write_file(&root.join("project").join("src").join("main.rs"), 256);
    write_file(&root.join("readme.txt"), 64);
    root
}

#[test]
fn complete_scan_inspect_search_cleanup_operation_rescan_flow() {
    let root = build_project("flow");

    // 1-2. Select the fixture and scan it.
    let first = scan(&root);
    assert_eq!(first.status, ScanStatus::Complete);
    assert_eq!(first.stats.files, 8);
    assert_eq!(first.stats.total_size, 4000 + 8000 + 8000 + 2000 + 7000 + 512 + 256 + 64);
    assert!(verify_aggregation(&first).is_empty(), "folder sums must agree");

    // 3. View aggregated results.
    let stats = overview(&first);
    assert_eq!(stats.total_size, first.stats.total_size);
    assert!(stats.top_level.iter().any(|entry| entry.name == "Images" && entry.size == 16000));
    assert!(stats.largest_file.is_some());
    assert!(stats.largest_folder.is_some());
    assert!(stats.category_count >= 3);

    // 4-5. Select a folder and lay out its treemap.
    let images = find_path(&first, "Images").expect("Images folder");
    let children: Vec<f64> = first
        .child_ids(images)
        .into_iter()
        .map(|id| first.node(id).unwrap().size as f64)
        .collect();
    let rects = squarify(&children);
    assert_eq!(rects.len(), 2);
    let covered: f64 = rects.iter().map(|rect| rect.area()).sum();
    assert!((covered - 1.0).abs() < 1e-6);

    // 6-7. Inspect classification and delete-risk explanations.
    let photo = find_path(&first, "Images/photo.jpg").expect("photo");
    let node = first.node(photo).unwrap();
    assert_eq!(node.category, Category::Images);
    assert_eq!(node.risk, RiskLevel::High);
    let classification = classify_detailed(
        std::path::Path::new(&first.path_of(photo)),
        "photo.jpg",
        false,
        node.attributes,
    );
    assert_eq!(classification.category, Category::Images);
    let risk = assess_detailed(
        std::path::Path::new(&first.path_of(photo)),
        "photo.jpg",
        false,
        node.attributes,
        node.size,
        node.modified,
        node.category,
        smartdisk::core::util::now_unix(),
    );
    assert_eq!(risk.level, RiskLevel::High);
    assert!(!risk.recommendation.is_empty());
    assert!(risk.evidence.iter().any(|line| line.contains("Images")));

    // 8. Search the scanned tree.
    let now = smartdisk::core::util::now_unix();
    let found = search(
        &first,
        &SearchFilters {
            query: "photo".into(),
            ..Default::default()
        },
        now,
    );
    assert_eq!(found.len(), 2);

    // 9-11. Build a cleanup preview and cancel it without touching disk.
    let groups = recommendations(&first);
    let temporary = groups
        .iter()
        .find(|group| group.key == "temporary")
        .expect("temporary group");
    assert!(temporary
        .items
        .iter()
        .any(|item| item.name == "cache.tmp"));
    let candidates: Vec<_> = temporary.items.iter().map(|item| item.node).collect();
    let preview = build_plan(&first, &candidates, true);
    assert!(preview.estimate.warnings[0].contains("Recycle Bin"));
    assert!(preview.items.iter().all(|item| item.risk.cleanup_eligible()));
    // Cancelling a preview leaves the filesystem untouched.
    drop(preview);
    assert!(root.join("Temp").join("cache.tmp").is_file());

    // 12. Perform a safe, non-destructive operation.
    let destination = root.join("Documents");
    let copied = copy_items(&[std::path::PathBuf::from(&first.path_of(photo))], &destination)
        .expect("copy photo");
    assert!(copied.ok(), "{}", copied.summary());
    assert!(destination.join("photo.jpg").is_file());

    // 13. Rescan and confirm updated results.
    let second = scan(&root);
    assert_eq!(second.stats.files, 9);
    assert_eq!(second.stats.total_size, first.stats.total_size + 8000);
    let copied_id = find_path(&second, "Documents/photo.jpg").expect("copied file");
    assert_eq!(second.node(copied_id).unwrap().size, 8000);
    assert_eq!(
        second.node(copied_id).unwrap().category,
        Category::Images,
        "extension evidence classifies the copied photo"
    );

    // 14. Three identical photos are now reported as duplicates.
    let groups = find_duplicates(&second, 1024, Arc::new(DuplicateProgress::new()));
    let photo_group = groups
        .iter()
        .find(|group| group.size == 8000)
        .expect("photo duplicate group");
    assert_eq!(photo_group.files.len(), 3);
    assert_eq!(photo_group.recoverable, 16000);

    // 15. Rename the copy, rescan, then recycle it and rescan again.
    let renamed = rename_item(&destination.join("photo.jpg"), "photo-renamed.jpg").unwrap();
    let third = scan(&root);
    assert_eq!(third.stats.files, 9);
    assert!(find_path(&third, "Documents/photo-renamed.jpg").is_some());

    let recycled = delete_to_recycle_bin(&[renamed.clone()]).expect("recycle copy");
    assert!(recycled.ok(), "{}", recycled.summary());
    assert!(!renamed.exists());

    let fourth = scan(&root);
    assert_eq!(fourth.stats.files, 8);
    assert_eq!(fourth.stats.total_size, first.stats.total_size);
    assert!(find_path(&fourth, "Documents/photo-renamed.jpg").is_none());
    assert_eq!(fourth.status, ScanStatus::Complete);
    assert!(verify_aggregation(&fourth).is_empty());

    remove_fixture(&root);
}
