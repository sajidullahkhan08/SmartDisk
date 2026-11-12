//! Fast directory enumeration.
//!
//! `FindFirstFileExW` with `FindExInfoBasic` (skip 8.3 short names) and
//! `FIND_FIRST_EX_LARGE_FETCH` (let NTFS return big batches of entries in one
//! call) replaces `std::fs::read_dir` on the scan hot path. All the data the
//! scanner needs (name, attributes, size, timestamps) arrives in the same
//! `WIN32_FIND_DATAW` records, so no extra metadata syscall is made per entry.

use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::Path;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{FILETIME, HANDLE, ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_FILES};
use windows::Win32::Storage::FileSystem::{
    FindClose, FindFirstFileExW, FindNextFileW, FindExInfoBasic, FindExSearchNameMatch,
    FIND_FIRST_EX_FLAGS, FIND_FIRST_EX_LARGE_FETCH, WIN32_FIND_DATAW,
};

/// One directory entry, straight from the find-data record.
pub struct RawEntry {
    pub name: OsString,
    pub attributes: u32,
    pub size: u64,
    pub modified_unix: Option<i64>,
    pub created_unix: Option<i64>,
}

/// Result of listing one directory. `next_error` reports a failure that
/// happened after some entries were already returned, so callers can keep the
/// partial listing and record the issue.
pub struct DirListing {
    pub entries: Vec<RawEntry>,
    pub next_error: Option<io::Error>,
}

fn filetime_to_unix(ft: &FILETIME) -> Option<i64> {
    let ticks = ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64;
    if ticks == 0 {
        return None;
    }
    // 100 ns intervals since 1601-01-01 to unix seconds.
    Some((ticks / 10_000_000) as i64 - 11_644_473_600)
}

fn entry_from_data(data: &WIN32_FIND_DATAW) -> RawEntry {
    let name_len = data
        .cFileName
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(data.cFileName.len());
    let name = OsString::from_wide(&data.cFileName[..name_len]);
    RawEntry {
        name,
        attributes: data.dwFileAttributes,
        size: ((data.nFileSizeHigh as u64) << 32) | data.nFileSizeLow as u64,
        modified_unix: filetime_to_unix(&data.ftLastWriteTime),
        created_unix: filetime_to_unix(&data.ftCreationTime),
    }
}

fn is_dot_name(entry: &RawEntry) -> bool {
    let s = entry.name.to_string_lossy();
    s == "." || s == ".."
}

fn win32_code(err: &windows::core::Error) -> u32 {
    err.code().0 as u32 & 0xFFFF
}

/// List `dir` (which may be a verbatim-prefixed path). A missing directory is
/// an error; an empty directory is an empty listing.
pub fn list_directory(dir: &Path) -> io::Result<DirListing> {
    let mut pattern: Vec<u16> = dir.as_os_str().encode_wide().collect();
    pattern.extend_from_slice(&[b'\\' as u16, b'*' as u16, 0]);

    let mut data = WIN32_FIND_DATAW::default();
    let handle: HANDLE = match unsafe {
        FindFirstFileExW(
            PCWSTR(pattern.as_ptr()),
            FindExInfoBasic,
            (&mut data as *mut WIN32_FIND_DATAW).cast(),
            FindExSearchNameMatch,
            None,
            FIND_FIRST_EX_FLAGS(FIND_FIRST_EX_LARGE_FETCH.0),
        )
    } {
        Ok(h) => h,
        Err(e) => {
            let code = win32_code(&e);
            // An empty directory looks like "file not found" for the wildcard.
            if code == ERROR_FILE_NOT_FOUND.0 {
                return Ok(DirListing {
                    entries: Vec::new(),
                    next_error: None,
                });
            }
            return Err(io::Error::from_raw_os_error(code as i32));
        }
    };

    let mut entries = Vec::with_capacity(128);
    let first = entry_from_data(&data);
    if !is_dot_name(&first) {
        entries.push(first);
    }
    let mut next_error = None;
    loop {
        data = WIN32_FIND_DATAW::default();
        match unsafe { FindNextFileW(handle, &mut data) } {
            Ok(()) => {
                let entry = entry_from_data(&data);
                if !is_dot_name(&entry) {
                    entries.push(entry);
                }
            }
            Err(e) => {
                let code = win32_code(&e);
                if code != ERROR_NO_MORE_FILES.0 {
                    next_error = Some(io::Error::from_raw_os_error(code as i32));
                }
                break;
            }
        }
    }
    let _ = unsafe { FindClose(handle) };
    Ok(DirListing {
        entries,
        next_error,
    })
}
