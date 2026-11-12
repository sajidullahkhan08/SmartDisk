//! Scanner integration tests against controlled filesystem fixtures.

mod common;

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use common::{find_child, find_path, fixture_root, remove_fixture, scan, scan_with, write_file};
use smartdisk::core::errors::ScanError;
use smartdisk::core::models::ScanStatus;
use smartdisk::scanner::{run_scan, ScanControl, ScanOptions, ScanProgress};

#[test]
fn nested_tree_aggregates_sizes_and_counts() {
    let root = fixture_root("nested");
    write_file(&root.join("a.txt"), 100);
    write_file(&root.join("zero.bin"), 0);
    write_file(&root.join("sub").join("b.bin"), 200);
    write_file(&root.join("sub").join("deep").join("c.dat"), 300);
    fs::create_dir_all(root.join("empty")).unwrap();

    let result = scan(&root);
    assert_eq!(result.stats.files, 4);
    assert_eq!(result.stats.dirs, 3, "sub, deep and empty");
    assert_eq!(result.stats.total_size, 600);
    assert_eq!(result.status, ScanStatus::Complete);

    let root_node = result.node(result.root).unwrap();
    assert_eq!(root_node.size, 600);
    assert_eq!(root_node.direct_size, 100);
    assert_eq!(root_node.file_count, 4, "counts include descendants");
    assert_eq!(root_node.dir_count, 3);

    let sub = find_child(&result, result.root, "sub").expect("sub folder");
    assert_eq!(result.node(sub).unwrap().size, 500);
    assert_eq!(result.node(sub).unwrap().file_count, 2);
    assert_eq!(result.node(sub).unwrap().dir_count, 1);

    let deep = find_path(&result, "sub/deep").expect("deep folder");
    assert_eq!(result.node(deep).unwrap().size, 300);

    let empty = find_child(&result, result.root, "empty").expect("empty folder");
    assert_eq!(result.node(empty).unwrap().size, 0);
    assert_eq!(result.node(empty).unwrap().file_count, 0);

    remove_fixture(&root);
}

#[test]
fn unicode_and_long_names_are_preserved() {
    let root = fixture_root("unicode");
    let japanese = "日本語のデータ.txt";
    let emoji = "snapshot-😀.bin";
    let long = format!("{}.dat", "a".repeat(180));
    write_file(&root.join(japanese), 10);
    write_file(&root.join(emoji), 20);
    write_file(&root.join(&long), 30);

    let result = scan(&root);
    assert_eq!(result.stats.files, 3);
    let names: Vec<&str> = result
        .child_ids(result.root)
        .into_iter()
        .map(|id| result.node(id).unwrap().name.as_str())
        .collect();
    assert!(names.contains(&japanese));
    assert!(names.contains(&emoji));
    assert!(names.contains(&long.as_str()));

    remove_fixture(&root);
}

#[test]
fn hidden_and_readonly_attributes_are_read() {
    let root = fixture_root("attributes");
    let path = root.join("protected.txt");
    write_file(&path, 12);
    let status = Command::new("attrib")
        .args(["+h", "+r"])
        .arg(&path)
        .status()
        .expect("run attrib");
    assert!(status.success(), "attrib should succeed");

    let result = scan(&root);
    let id = find_child(&result, result.root, "protected.txt").expect("file node");
    let node = result.node(id).unwrap();
    assert!(node.is_hidden(), "hidden attribute should be exposed");
    assert!(node.is_readonly(), "read-only attribute should be exposed");

    // Do not leave a read-only fixture behind.
    let _ = Command::new("attrib").args(["-h", "-r"]).arg(&path).status();
    remove_fixture(&root);
}

#[test]
fn many_small_files_are_counted_once() {
    let root = fixture_root("many");
    for folder in 0..5 {
        for file in 0..50 {
            write_file(&root.join(format!("dir{folder}")).join(format!("f{file}.bin")), 32);
        }
    }
    let result = scan(&root);
    assert_eq!(result.stats.files, 250);
    assert_eq!(result.stats.dirs, 5);
    assert_eq!(result.stats.total_size, 250 * 32);
    assert_eq!(result.status, ScanStatus::Complete);
    remove_fixture(&root);
}

#[test]
fn exclusions_are_component_aware_and_skip_subtrees() {
    let root = fixture_root("exclusions");
    write_file(&root.join("keep.txt"), 10);
    write_file(&root.join("skip").join("inside.txt"), 20);
    write_file(&root.join("skip-alike").join("kept.txt"), 30);

    let excluded = root.join("skip");
    let result = scan_with(&root, |options| {
        options.exclusions = vec![excluded];
    });
    assert!(find_child(&result, result.root, "skip").is_none());
    assert!(find_child(&result, result.root, "skip-alike").is_some());
    assert_eq!(result.stats.files, 2);
    assert_eq!(result.status, ScanStatus::Complete);

    remove_fixture(&root);
}

#[test]
fn duplicate_names_in_different_folders_get_distinct_paths() {
    let root = fixture_root("dupes");
    write_file(&root.join("one").join("same.txt"), 11);
    write_file(&root.join("two").join("same.txt"), 22);

    let result = scan(&root);
    let a = find_path(&result, "one/same.txt").expect("first");
    let b = find_path(&result, "two/same.txt").expect("second");
    assert_ne!(a, b);
    assert_ne!(result.path_of(a), result.path_of(b));
    assert_eq!(result.stats.files, 2);

    remove_fixture(&root);
}

#[test]
fn deep_nesting_is_scanned_without_recursion_overflow() {
    let root = fixture_root("deep");
    let mut current = root.clone();
    for level in 0..40 {
        current = current.join(format!("level{level:02}"));
        write_file(&current.join("leaf.bin"), 1);
    }
    let result = scan(&root);
    assert_eq!(result.stats.files, 40);
    assert_eq!(result.stats.dirs, 40);
    assert_eq!(result.stats.total_size, 40);
    let deepest = find_path(&result, &{
        let mut rel = String::new();
        for level in 0..40 {
            if level > 0 {
                rel.push('/');
            }
            rel.push_str(&format!("level{level:02}"));
        }
        rel
    })
    .expect("deepest level");
    assert_eq!(result.depth_of(deepest), 40);
    remove_fixture(&root);
}

#[test]
fn junction_is_reported_and_not_followed_by_default() {
    let root = fixture_root("junction");
    let outside = fixture_root("junction-target");
    write_file(&outside.join("secret.txt"), 64);
    write_file(&root.join("visible.txt"), 8);

    let link = root.join("link-to-outside");
    let output = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(&outside)
        .output()
        .expect("run mklink");
    if !output.status.success() {
        // Junction creation is available without elevation on NTFS, but the
        // filesystem may not support it (for example FAT/exFAT).
        eprintln!(
            "SKIP junction test: mklink failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        remove_fixture(&root);
        remove_fixture(&outside);
        return;
    }

    let result = scan(&root);
    assert_eq!(result.stats.skipped_reparse, 1);
    assert!(result
        .warnings
        .iter()
        .any(|warning| warning.contains("reparse point")));
    assert!(
        find_path(&result, "link-to-outside/secret.txt").is_none(),
        "junction targets must not be traversed by default"
    );
    assert_eq!(result.stats.files, 1, "only visible.txt is counted");

    let _ = fs::remove_dir(&link);
    remove_fixture(&root);
    remove_fixture(&outside);
}

#[test]
fn permission_denied_subtree_is_reported_without_aborting_scan() {
    let root = fixture_root("denied");
    write_file(&root.join("visible.txt"), 10);
    let denied = root.join("locked");
    fs::create_dir_all(&denied).unwrap();
    write_file(&denied.join("inside.txt"), 20);

    let user = std::env::var("USERNAME").unwrap_or_else(|_| "Everyone".into());
    let deny = Command::new("icacls")
        .arg(&denied)
        .arg("/deny")
        .arg(format!("{user}:(OI)(CI)(RX)"))
        .output()
        .expect("run icacls");
    if !deny.status.success() {
        eprintln!(
            "SKIP permission-denied test: icacls failed: {}",
            String::from_utf8_lossy(&deny.stderr)
        );
        remove_fixture(&root);
        return;
    }
    // Elevated test hosts may run with SeBackupPrivilege, which bypasses ACLs
    // for directory enumeration. A skip is reported instead of asserting a
    // failure the environment cannot produce.
    if fs::read_dir(&denied).is_ok() {
        eprintln!(
            "SKIP permission-denied test: this process can read ACL-denied directories (backup privilege)."
        );
        let _ = Command::new("icacls")
            .arg(&denied)
            .arg("/remove:d")
            .arg(&user)
            .output();
        remove_fixture(&root);
        return;
    }

    let result = scan(&root);
    // The scan must finish and keep everything it could read; the inaccessible
    // subtree is reported instead of silently disappearing or aborting.
    assert_eq!(result.status, ScanStatus::Partial);
    assert!(
        !result.errors.is_empty(),
        "an unreadable directory must be recorded as an error"
    );
    assert!(
        result.errors.iter().any(|issue| issue.path.contains("locked")),
        "the error should identify the unreadable directory: {:?}",
        result.errors
    );
    assert!(find_child(&result, result.root, "visible.txt").is_some());
    assert!(find_path(&result, "locked/inside.txt").is_none());

    let _ = Command::new("icacls")
        .arg(&denied)
        .arg("/remove:d")
        .arg(&user)
        .output();
    remove_fixture(&root);
}

#[test]
fn cancelled_scan_reports_cancelled_status_with_partial_data() {
    let root = fixture_root("cancel");
    write_file(&root.join("one.bin"), 10);
    let control = Arc::new(ScanControl::new());
    control.cancel();
    let result = run_scan(
        ScanOptions::new(root.clone()),
        control,
        Arc::new(ScanProgress::new()),
    )
    .expect("pre-cancelled scans still return the root");
    assert_eq!(result.status, ScanStatus::Cancelled);
    assert!(result
        .warnings
        .iter()
        .any(|warning| warning.contains("cancelled")));
    remove_fixture(&root);
}

#[test]
fn missing_root_is_an_actionable_error() {
    let root = fixture_root("missing");
    let gone = root.join("does-not-exist");
    let error = run_scan(
        ScanOptions::new(gone),
        Arc::new(ScanControl::new()),
        Arc::new(ScanProgress::new()),
    )
    .expect_err("missing roots must fail");
    assert!(matches!(error, ScanError::NotFound(_)));
    remove_fixture(&root);
}

#[test]
fn file_root_is_rejected() {
    let root = fixture_root("fileroot");
    let file = root.join("plain.txt");
    write_file(&file, 4);
    let error = run_scan(
        ScanOptions::new(file),
        Arc::new(ScanControl::new()),
        Arc::new(ScanProgress::new()),
    )
    .expect_err("files are not scan roots");
    assert!(matches!(error, ScanError::NotADirectory(_)));
    remove_fixture(&root);
}

#[test]
fn worker_count_does_not_change_results() {
    let root = fixture_root("workers");
    for folder in 0..4 {
        for file in 0..20 {
            write_file(&root.join(format!("d{folder}")).join(format!("f{file}")), 7);
        }
    }
    let sequential = scan_with(&root, |options| options.workers = 1);
    let parallel = scan_with(&root, |options| options.workers = 4);
    assert_eq!(sequential.stats.files, parallel.stats.files);
    assert_eq!(sequential.stats.total_size, parallel.stats.total_size);
    assert_eq!(sequential.stats.dirs, parallel.stats.dirs);
    remove_fixture(&root);
}

#[test]
fn allocated_size_uses_clusters() {
    let root = fixture_root("allocated");
    write_file(&root.join("tiny.bin"), 1);
    let result = scan_with(&root, |options| options.cluster_size = 4096);
    assert_eq!(result.stats.total_size, 1);
    assert_eq!(result.stats.total_allocated, 4096);
    let id = find_path(&result, "tiny.bin").expect("file");
    assert_eq!(result.node(id).unwrap().allocated, 4096);
    remove_fixture(&root);
}

#[test]
fn scan_paths_are_absolute_and_stable() {
    let root = fixture_root("paths");
    write_file(&root.join("x.txt"), 1);
    let result = scan(Path::new(&root));
    assert!(Path::new(&result.root_path).is_absolute());
    let id = find_child(&result, result.root, "x.txt").expect("file");
    assert!(result.path_of(id).ends_with("x.txt"));
    assert_eq!(result.path_of(result.root), result.root_path);
    remove_fixture(&root);
}
