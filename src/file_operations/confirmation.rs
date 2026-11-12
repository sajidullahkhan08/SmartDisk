use std::path::Path;

use crate::core::util::format_size;

/// Human-readable description of a pending destructive action. The UI must
/// show this before executing anything.
#[derive(Clone, Debug)]
pub struct DeleteConfirmation {
    pub title: String,
    pub message: String,
    pub item_count: usize,
    pub total_size: u64,
    pub permanent: bool,
}

pub fn describe_delete(
    items: &[&Path],
    sizes: &[u64],
    permanent: bool,
) -> DeleteConfirmation {
    let total: u64 = sizes.iter().sum();
    let count = items.len();
    let noun = if count == 1 { "item" } else { "items" };
    let message = if permanent {
        format!(
            "You are about to permanently delete {count} {noun} ({size}).\n\n\
             This bypasses the Recycle Bin and cannot be undone. Only continue if \
             you are certain that these items are not needed.",
            size = format_size(total)
        )
    } else {
        format!(
            "Move {count} {noun} ({size}) to the Recycle Bin?\n\n\
             The Recycle Bin does not free disk space until it is emptied. \
             You can restore items from the Recycle Bin if needed.",
            size = format_size(total)
        )
    };
    DeleteConfirmation {
        title: if permanent {
            "Permanently delete items".into()
        } else {
            "Move items to Recycle Bin".into()
        },
        message,
        item_count: count,
        total_size: total,
        permanent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmation_mentions_recycle_bin_and_size() {
        let p = Path::new("C:\\Temp\\x.tmp");
        let c = describe_delete(&[p], &[2048], false);
        assert!(c.message.contains("Recycle Bin"));
        assert!(c.message.contains("2.00 KB"));
        assert_eq!(c.item_count, 1);
        assert!(!c.permanent);
    }

    #[test]
    fn permanent_confirmation_is_clear() {
        let p = Path::new("C:\\Temp\\x.tmp");
        let c = describe_delete(&[p], &[0], true);
        assert!(c.message.contains("cannot be undone"));
        assert!(c.permanent);
    }
}
