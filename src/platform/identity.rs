//! Stable NTFS/file identity without unstable `std` extension methods.
//!
//! `GetFileInformationByHandle` provides the volume serial, file index and link
//! count on every supported Windows version.

use std::fs::File;

use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileIdentity {
    pub volume: u32,
    pub index: u64,
    pub links: u32,
}

pub fn identity_from_file(file: &File) -> Option<FileIdentity> {
    use std::os::windows::io::AsRawHandle;

    let handle = HANDLE(file.as_raw_handle());
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(handle, &mut info).ok()?;
    }
    Some(FileIdentity {
        volume: info.dwVolumeSerialNumber,
        index: ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64,
        links: info.nNumberOfLinks,
    })
}

pub fn identity_of_path(path: &std::path::Path) -> Option<FileIdentity> {
    File::open(path).ok().and_then(|file| identity_from_file(&file))
}
