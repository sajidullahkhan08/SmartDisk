use std::path::PathBuf;

use crate::file_operations::operations::OperationOutcome;
use crate::file_operations::{copy_items, delete_to_recycle_bin, move_items};
use crate::platform::shell;

/// In-application file clipboard. Windows Explorer can also paste the copied
/// text paths, but the structured state drives in-app paste behaviour.
#[derive(Clone, Debug, Default)]
pub struct FileClipboard {
    pub paths: Vec<PathBuf>,
    pub cut: bool,
}

impl FileClipboard {
    pub fn copy(paths: Vec<PathBuf>) -> Self {
        Self { paths, cut: false }
    }

    pub fn cut(paths: Vec<PathBuf>) -> Self {
        Self { paths, cut: true }
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    pub fn paste_into(&self, destination: &std::path::Path) -> Option<crate::core::errors::FileOpResult<OperationOutcome>> {
        if self.paths.is_empty() {
            return None;
        }
        Some(if self.cut {
            move_items(&self.paths, destination)
        } else {
            copy_items(&self.paths, destination)
        })
    }
}

pub fn copy_path_text(path: &std::path::Path) -> Result<(), String> {
    shell::set_clipboard_text(&path.to_string_lossy())
}

pub fn copy_paths_text(paths: &[PathBuf]) -> Result<(), String> {
    let text = paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("\r\n");
    shell::set_clipboard_text(&text)
}

/// Delete helper used by the normal UI path. Never silently permanent.
pub fn recycle_items(
    paths: &[PathBuf],
) -> crate::core::errors::FileOpResult<OperationOutcome> {
    delete_to_recycle_bin(paths)
}
