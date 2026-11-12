//! File operations.
//!
//! Copy, move and rename are performed through `IFileOperation` on a dedicated
//! STA COM context so Windows provides its native conflict and progress UI.
//! Recycle-bin deletion uses the well-tested `trash` crate, which invokes the
//! native shell operation. Permanent deletion is a separate, explicitly
//! confirmed advanced action and is protected by the same location policy as
//! the rest of the application.

use std::path::{Path, PathBuf};

use windows::core::HSTRING;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{
    FileOperation, IFileOperation, IFileOperationProgressSink, IShellItem,
    SHCreateItemFromParsingName, FOF_NOCONFIRMMKDIR,
};

use crate::core::errors::{FileOpError, FileOpResult};
use crate::core::util::{available_destination, is_same_or_subpath, validate_file_name};

#[derive(Clone, Debug, Default)]
pub struct OperationOutcome {
    pub succeeded: Vec<PathBuf>,
    pub failed: Vec<(PathBuf, String)>,
    pub cancelled: bool,
}

impl OperationOutcome {
    pub fn ok(&self) -> bool {
        self.failed.is_empty() && !self.cancelled
    }

    pub fn summary(&self) -> String {
        if self.cancelled {
            return "The operation was cancelled.".into();
        }
        if self.failed.is_empty() {
            let count = self.succeeded.len();
            if count == 1 {
                "1 item processed successfully.".to_string()
            } else {
                format!("{count} items processed successfully.")
            }
        } else {
            format!(
                "{} succeeded, {} failed.",
                plural_count(self.succeeded.len()),
                plural_count(self.failed.len())
            )
        }
    }
}

fn plural_count(count: usize) -> String {
    if count == 1 {
        "1 item".to_string()
    } else {
        format!("{count} items")
    }
}

fn with_sta<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let initialized = hr.is_ok();
    let result = f();
    if initialized {
        unsafe { CoUninitialize() };
    }
    result
}

fn shell_item(path: &Path) -> windows::core::Result<IShellItem> {
    let text = path.to_string_lossy().into_owned();
    unsafe { SHCreateItemFromParsingName(&HSTRING::from(text), None) }
}

/// Copy one or more items into `destination` using the Windows shell.
pub fn copy_items(paths: &[PathBuf], destination: &Path) -> FileOpResult<OperationOutcome> {
    validate_batch(paths)?;
    if !destination.is_dir() {
        return Err(FileOpError::Invalid(format!(
            "Destination is not a folder: {}",
            destination.display()
        )));
    }
    let mut outcome = OperationOutcome::default();
    with_sta(|| unsafe {
        let operation: IFileOperation =
            CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(|e| e.to_string())?;
        operation
            .SetOperationFlags(FOF_NOCONFIRMMKDIR)
            .map_err(|e| e.to_string())?;
        let dest_item = shell_item(destination).map_err(|e| e.to_string())?;
        let mut added = 0usize;
        for path in paths {
            match shell_item(path) {
                Ok(item) => match operation.CopyItem(&item, &dest_item, None, None) {
                    Ok(()) => {
                        added += 1;
                        outcome.succeeded.push(path.clone());
                    }
                    Err(error) => outcome.failed.push((path.clone(), error.to_string())),
                },
                Err(error) => outcome.failed.push((path.clone(), error.to_string())),
            }
        }
        if added > 0 {
            if let Err(error) = operation.PerformOperations() {
                // The shell reports a single failure for the batch. Move items
                // that were queued into the failed list so the UI does not
                // claim success.
                let message = error.to_string();
                outcome.succeeded.clear();
                for path in paths {
                    outcome.failed.push((path.clone(), message.clone()));
                }
            }
        }
        Ok(())
    })
    .map_err(|message| FileOpError::Io(std::io::Error::other(message)))?;
    Ok(outcome)
}

/// Move one or more items into `destination` using the Windows shell.
pub fn move_items(paths: &[PathBuf], destination: &Path) -> FileOpResult<OperationOutcome> {
    validate_batch(paths)?;
    if !destination.is_dir() {
        return Err(FileOpError::Invalid(format!(
            "Destination is not a folder: {}",
            destination.display()
        )));
    }
    for source in paths {
        if source.parent() == Some(destination) {
            return Err(FileOpError::Invalid(format!(
                "The item is already in the destination folder: {}",
                source.display()
            )));
        }
        if source.is_dir() && is_same_or_subpath(destination, source) {
            return Err(FileOpError::Invalid(
                "A folder cannot be moved inside itself.".into(),
            ));
        }
    }
    let mut outcome = OperationOutcome::default();
    with_sta(|| unsafe {
        let operation: IFileOperation =
            CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(|e| e.to_string())?;
        operation
            .SetOperationFlags(FOF_NOCONFIRMMKDIR)
            .map_err(|e| e.to_string())?;
        let dest_item = shell_item(destination).map_err(|e| e.to_string())?;
        let mut added = 0usize;
        for path in paths {
            match shell_item(path) {
                Ok(item) => match operation.MoveItem(&item, &dest_item, None, None) {
                    Ok(()) => {
                        added += 1;
                        outcome.succeeded.push(path.clone());
                    }
                    Err(error) => outcome.failed.push((path.clone(), error.to_string())),
                },
                Err(error) => outcome.failed.push((path.clone(), error.to_string())),
            }
        }
        if added > 0 {
            if let Err(error) = operation.PerformOperations() {
                let message = error.to_string();
                outcome.succeeded.clear();
                for path in paths {
                    outcome.failed.push((path.clone(), message.clone()));
                }
            }
        }
        Ok(())
    })
    .map_err(|message| FileOpError::Io(std::io::Error::other(message)))?;
    Ok(outcome)
}

/// Rename an item in place. The new name is validated before touching disk.
pub fn rename_item(path: &Path, new_name: &str) -> FileOpResult<PathBuf> {
    validate_single(path)?;
    let new_name = new_name.trim();
    validate_file_name(new_name).map_err(FileOpError::Invalid)?;
    let parent = path
        .parent()
        .ok_or_else(|| FileOpError::Invalid("The item has no parent folder.".into()))?;
    let target = parent.join(new_name);
    if target == path {
        return Ok(target);
    }
    if target.exists() {
        return Err(FileOpError::AlreadyExists(target.display().to_string()));
    }
    with_sta(|| unsafe {
        let operation: IFileOperation =
            CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(|e| e.to_string())?;
        operation
            .SetOperationFlags(FOF_NOCONFIRMMKDIR)
            .map_err(|e| e.to_string())?;
        let item: IShellItem = SHCreateItemFromParsingName(
            &HSTRING::from(path.to_string_lossy().into_owned()),
            None,
        )
        .map_err(|e| e.to_string())?;
        operation
            .RenameItem(
                &item,
                &HSTRING::from(new_name),
                None::<&IFileOperationProgressSink>,
            )
            .map_err(|e| e.to_string())?;
        operation
            .PerformOperations()
            .map_err(|e| e.to_string())?;
        Ok(())
    })
    .map_err(|message| FileOpError::Io(std::io::Error::other(message)))?;
    Ok(target)
}

/// Move items to the Recycle Bin. Protected locations are rejected.
pub fn delete_to_recycle_bin(paths: &[PathBuf]) -> FileOpResult<OperationOutcome> {
    validate_batch(paths)?;
    let mut outcome = OperationOutcome::default();
    for path in paths {
        match trash::delete(path) {
            Ok(()) => outcome.succeeded.push(path.clone()),
            Err(error) => outcome.failed.push((path.clone(), error.to_string())),
        }
    }
    Ok(outcome)
}

/// Explicit permanent deletion. This must always be preceded by a separate
/// confirmation in the UI; it is never used by the cleanup assistant.
pub fn delete_permanent(paths: &[PathBuf]) -> FileOpResult<OperationOutcome> {
    validate_batch(paths)?;
    let mut outcome = OperationOutcome::default();
    for path in paths {
        match remove_any(path) {
            Ok(()) => outcome.succeeded.push(path.clone()),
            Err(error) => outcome.failed.push((path.clone(), error.to_string())),
        }
    }
    Ok(outcome)
}

fn remove_any(path: &Path) -> std::io::Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    let attributes = {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes()
    };
    const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        // Remove the link/junction itself, never the target tree.
        if attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
            std::fs::remove_dir(path)
        } else {
            std::fs::remove_file(path)
        }
    } else if metadata.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        let mut permissions = metadata.permissions();
        if permissions.readonly() {
            permissions.set_readonly(false);
            let _ = std::fs::set_permissions(path, permissions);
        }
        std::fs::remove_file(path)
    }
}

fn validate_single(path: &Path) -> FileOpResult<()> {
    if !path.exists() && std::fs::symlink_metadata(path).is_err() {
        return Err(FileOpError::Invalid(format!(
            "The item no longer exists: {}",
            path.display()
        )));
    }
    if path.parent().is_none() {
        return Err(FileOpError::Protected(
            "Drive roots cannot be modified.".into(),
        ));
    }
    if crate::intelligence::risk::is_protected_path(path) {
        return Err(FileOpError::Protected(format!(
            "{} is a protected system location.",
            path.display()
        )));
    }
    Ok(())
}

fn validate_batch(paths: &[PathBuf]) -> FileOpResult<()> {
    if paths.is_empty() {
        return Err(FileOpError::Invalid("No items were selected.".into()));
    }
    for path in paths {
        validate_single(path)?;
    }
    Ok(())
}

/// Resolve a non-conflicting destination for paste fallback flows.
pub fn unique_target(directory: &Path, name: &str) -> PathBuf {
    available_destination(directory, name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn rename_validates_names() {
        let dir = temp_dir();
        let file = dir.path().join("a.txt");
        fs::write(&file, b"x").unwrap();
        assert!(rename_item(&file, "bad/name").is_err());
        assert!(rename_item(&file, "CON").is_err());
        let renamed = rename_item(&file, "b.txt").unwrap();
        assert!(renamed.exists());
        assert!(!file.exists());
    }

    #[test]
    fn rename_refuses_protected_paths() {
        let result = rename_item(Path::new("C:\\Windows\\System32\\kernel32.dll"), "x.dll");
        assert!(matches!(result, Err(FileOpError::Protected(_))));
    }

    #[test]
    fn permanent_delete_removes_tree() {
        let dir = temp_dir();
        let nested = dir.path().join("nested");
        fs::create_dir_all(nested.join("deeper")).unwrap();
        fs::write(nested.join("deeper").join("f.txt"), b"abc").unwrap();
        let outcome = delete_permanent(&[nested.clone()]).unwrap();
        assert!(outcome.ok());
        assert!(!nested.exists());
    }

    #[test]
    fn recycled_files_are_rejected_when_missing() {
        let dir = temp_dir();
        let missing = dir.path().join("missing.txt");
        assert!(delete_to_recycle_bin(&[missing]).is_err());
    }
}
