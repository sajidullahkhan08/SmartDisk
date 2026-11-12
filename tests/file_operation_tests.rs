//! File-operation integration tests on isolated fixtures only.
//!
//! Copy, move and rename use the Windows shell on an STA thread (the same code
//! path as the UI) so these tests also exercise COM initialization. Destructive
//! tests use disposable files under `.testdata/` and never touch system paths.

mod common;

use std::fs;
use std::os::windows::fs::OpenOptionsExt;
use std::path::PathBuf;

use common::{fixture_root, remove_fixture, write_file};
use smartdisk::core::errors::FileOpError;
use smartdisk::file_operations::operations::{
    copy_items, delete_permanent, delete_to_recycle_bin, move_items, rename_item, unique_target,
    OperationOutcome,
};

#[test]
fn copy_file_and_folder_trees() {
    let root = fixture_root("copy");
    let source = root.join("source.bin");
    write_file(&source, 128);
    let folder = root.join("tree").join("nested");
    write_file(&folder.join("deep.bin"), 64);
    let destination = root.join("destination");
    fs::create_dir_all(&destination).unwrap();

    let outcome = copy_items(&[source.clone()], &destination).expect("copy file");
    assert!(outcome.ok(), "{}", outcome.summary());
    assert!(destination.join("source.bin").is_file());
    assert_eq!(
        fs::read(destination.join("source.bin")).unwrap(),
        fs::read(&source).unwrap()
    );

    let tree = root.join("tree");
    let tree_outcome = copy_items(&[tree], &destination).expect("copy tree");
    assert!(tree_outcome.ok(), "{}", tree_outcome.summary());
    assert!(destination.join("tree").join("nested").join("deep.bin").is_file());

    remove_fixture(&root);
}

#[test]
fn multi_selection_copy_reports_each_item() {
    let root = fixture_root("copy-multi");
    let a = root.join("a.txt");
    let b = root.join("b.txt");
    write_file(&a, 10);
    write_file(&b, 20);
    let destination = root.join("out");
    fs::create_dir_all(&destination).unwrap();

    let outcome = copy_items(&[a.clone(), b.clone()], &destination).unwrap();
    assert!(outcome.ok());
    assert_eq!(outcome.succeeded.len(), 2);
    assert!(destination.join("a.txt").exists());
    assert!(destination.join("b.txt").exists());
    assert_eq!(outcome.summary(), "2 items processed successfully.");
    remove_fixture(&root);
}

#[test]
fn move_removes_the_source() {
    let root = fixture_root("move");
    let source = root.join("move-me.bin");
    write_file(&source, 32);
    let destination = root.join("target");
    fs::create_dir_all(&destination).unwrap();

    let outcome = move_items(&[source.clone()], &destination).unwrap();
    assert!(outcome.ok(), "{}", outcome.summary());
    assert!(!source.exists());
    assert!(destination.join("move-me.bin").is_file());
    remove_fixture(&root);
}

#[test]
fn move_rejects_missing_source_and_non_folder_destination() {
    let root = fixture_root("move-errors");
    let missing = root.join("missing.bin");
    let destination = root.join("out");
    fs::create_dir_all(&destination).unwrap();
    let error = move_items(&[missing], &destination).expect_err("missing source");
    assert!(matches!(error, FileOpError::Invalid(_)));

    let file = root.join("file.bin");
    write_file(&file, 4);
    let not_folder = root.join("plain.txt");
    write_file(&not_folder, 4);
    let error = copy_items(&[file], &not_folder).expect_err("file destination");
    assert!(matches!(error, FileOpError::Invalid(_)));
    remove_fixture(&root);
}

#[test]
fn a_folder_cannot_be_moved_inside_itself() {
    let root = fixture_root("move-self");
    let folder = root.join("outer");
    write_file(&folder.join("inner").join("file.bin"), 8);
    let error = move_items(&[folder.clone()], &folder.join("inner")).expect_err("self move");
    assert!(matches!(error, FileOpError::Invalid(_)));
    assert!(folder.join("inner").join("file.bin").exists());
    remove_fixture(&root);
}

#[test]
fn rename_updates_the_path() {
    let root = fixture_root("rename");
    let source = root.join("old-name.txt");
    write_file(&source, 15);
    let renamed = rename_item(&source, "new-name.txt").unwrap();
    assert_eq!(renamed, root.join("new-name.txt"));
    assert!(!source.exists());
    assert!(renamed.is_file());
    remove_fixture(&root);
}

#[test]
fn rename_rejects_invalid_names_and_collisions() {
    let root = fixture_root("rename-errors");
    let source = root.join("keep.txt");
    write_file(&source, 5);

    for invalid in ["", ".", "..", "a/b", "bad<name>", "trailing.", "CON"] {
        let error = rename_item(&source, invalid).expect_err("invalid name");
        assert!(matches!(error, FileOpError::Invalid(_)), "name {invalid:?}");
    }

    write_file(&root.join("taken.txt"), 2);
    let error = rename_item(&source, "taken.txt").expect_err("collision");
    assert!(matches!(error, FileOpError::AlreadyExists(_)));
    assert!(source.exists(), "failed renames must not move the source");
    remove_fixture(&root);
}

#[test]
fn recycle_bin_move_is_reversible_in_principle_and_removes_source() {
    let root = fixture_root("recycle");
    let disposable = root.join("disposable-recycle.txt");
    write_file(&disposable, 42);

    let outcome = delete_to_recycle_bin(&[disposable.clone()]).expect("recycle");
    assert!(outcome.ok(), "{}", outcome.summary());
    assert_eq!(outcome.succeeded, vec![disposable.clone()]);
    assert!(!disposable.exists());
    remove_fixture(&root);
}

#[test]
fn recycle_bin_rejects_missing_items_before_touching_anything() {
    let root = fixture_root("recycle-missing");
    let missing = root.join("not-here.txt");
    let error = delete_to_recycle_bin(&[missing]).expect_err("missing item");
    assert!(matches!(error, FileOpError::Invalid(_)));
    remove_fixture(&root);
}

#[test]
fn partial_failure_is_reported_per_item() {
    let root = fixture_root("partial");
    let free = root.join("free.txt");
    let locked = root.join("locked.txt");
    write_file(&free, 8);
    write_file(&locked, 8);
    // Open the second file without sharing so the shell cannot remove it.
    let handle = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&locked)
        .expect("lock file");

    let outcome = delete_to_recycle_bin(&[free.clone(), locked.clone()]).expect("batch");
    assert!(!outcome.ok(), "locked file must be reported as failed");
    assert!(outcome.succeeded.contains(&free), "{:?}", outcome.succeeded);
    assert!(
        outcome.failed.iter().any(|(path, _)| path == &locked),
        "locked file missing from failures: {:?}",
        outcome.failed
    );
    assert!(!free.exists());
    assert!(locked.exists(), "the locked file must stay in place");

    drop(handle);
    remove_fixture(&root);
}

#[test]
fn permanent_delete_is_separate_and_removes_a_dedicated_tree() {
    let root = fixture_root("permanent");
    let tree = root.join("trash-me");
    write_file(&tree.join("child").join("file.bin"), 3);
    let outcome = delete_permanent(&[tree.clone()]).unwrap();
    assert!(outcome.ok(), "{}", outcome.summary());
    assert!(!tree.exists());
    remove_fixture(&root);
}

#[test]
fn unique_target_never_overwrites() {
    let root = fixture_root("unique");
    write_file(&root.join("report.txt"), 1);
    let first = unique_target(&root, "report.txt");
    assert_eq!(first.file_name().unwrap(), "report (copy 1).txt");
    write_file(&first, 1);
    let second = unique_target(&root, "report.txt");
    assert_eq!(second.file_name().unwrap(), "report (copy 2).txt");
    remove_fixture(&root);
}

#[test]
fn operation_outcome_summaries_are_grammatical() {
    let mut ok = OperationOutcome::default();
    ok.succeeded.push(PathBuf::from("a"));
    assert_eq!(ok.summary(), "1 item processed successfully.");
    ok.succeeded.push(PathBuf::from("b"));
    assert_eq!(ok.summary(), "2 items processed successfully.");

    let mut mixed = OperationOutcome::default();
    mixed.succeeded.push(PathBuf::from("a"));
    mixed.failed.push((PathBuf::from("b"), "denied".into()));
    mixed.failed.push((PathBuf::from("c"), "denied".into()));
    assert_eq!(mixed.summary(), "1 item succeeded, 2 items failed.");
    assert!(!mixed.ok());

    let mut cancelled = OperationOutcome::default();
    cancelled.cancelled = true;
    assert_eq!(cancelled.summary(), "The operation was cancelled.");
}
