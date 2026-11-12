//! Shell integration: open, reveal in Explorer, native properties dialog and
//! clipboard helpers. No command strings are built from untrusted file names
//! beyond a single quoted path argument to explorer.exe.

use std::path::{Path, PathBuf};

use windows::core::{PCWSTR, HSTRING};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{ShellExecuteExW, ShellExecuteW, SEE_MASK_INVOKEIDLIST, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Open a file or folder with its default handler.
pub fn open_path(path: &Path) -> Result<(), String> {
    let file = wide(&path.to_string_lossy());
    let operation = wide("open");
    let rc = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if rc.0 as isize <= 32 {
        Err(format!(
            "Windows could not open the item (error code {}).",
            rc.0 as isize
        ))
    } else {
        Ok(())
    }
}

/// Ask Windows to show the "Open with" chooser.
pub fn open_with(path: &Path) -> Result<(), String> {
    let file = wide(&path.to_string_lossy());
    let operation = wide("openas");
    let rc = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if rc.0 as isize <= 32 {
        open_path(path)
    } else {
        Ok(())
    }
}

/// Reveal an item in Windows Explorer with the item selected.
pub fn reveal_in_explorer(path: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    let display = path.to_string_lossy();
    if display.contains('"') {
        return Err("The path contains a quote character and cannot be revealed.".into());
    }
    let target = if path.exists() {
        path.to_path_buf()
    } else {
        path.parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    };
    let mut command = std::process::Command::new("explorer.exe");
    command.raw_arg(format!("/select,\"{}\"", target.display()));
    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not start Explorer: {e}"))
}

/// Open the standard Windows properties dialog for a file or folder.
///
/// The dialog is owned by a short-lived STA thread, as required by COM and the
/// shell property UI.
pub fn show_properties(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err("The item no longer exists.".into());
    }
    let path = path.to_path_buf();
    std::thread::Builder::new()
        .name("smartdisk-properties".into())
        .spawn(move || unsafe {
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let initialized = hr.is_ok();
            let file = wide(&path.to_string_lossy());
            let verb = wide("properties");
            let mut info = SHELLEXECUTEINFOW::default();
            info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
            info.fMask = SEE_MASK_INVOKEIDLIST;
            info.lpVerb = PCWSTR(verb.as_ptr());
            info.lpFile = PCWSTR(file.as_ptr());
            info.nShow = SW_SHOWNORMAL.0 as i32;
            let _ = ShellExecuteExW(&mut info);
            if initialized {
                CoUninitialize();
            }
        })
        .map(|_| ())
        .map_err(|e| format!("Could not open the properties dialog: {e}"))
}

/// Open a folder in Explorer (without selecting a child).
pub fn open_folder(path: &Path) -> Result<(), String> {
    let folder = wide(&path.to_string_lossy());
    let operation = wide("explore");
    let rc = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(folder.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if rc.0 as isize <= 32 {
        Err("Windows could not open the folder in Explorer.".into())
    } else {
        Ok(())
    }
}

/// Copy arbitrary text (typically a path) to the clipboard.
pub fn set_clipboard_text(text: &str) -> Result<(), String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Clipboard unavailable: {e}"))?;
    clipboard
        .set_text(text.to_string())
        .map_err(|e| format!("Could not write to the clipboard: {e}"))
}

/// Read text from the clipboard, used by the path entry.
pub fn clipboard_text() -> Option<String> {
    arboard::Clipboard::new()
        .ok()
        .and_then(|mut c| c.get_text().ok())
}

/// Sanitised display name for shell errors.
pub fn hstring(s: &str) -> HSTRING {
    HSTRING::from(s)
}
