use std::path::Path;

pub fn open(path: &Path) -> Result<(), String> {
    crate::platform::shell::open_path(path)
}

pub fn reveal(path: &Path) -> Result<(), String> {
    crate::platform::shell::reveal_in_explorer(path)
}

pub fn properties(path: &Path) -> Result<(), String> {
    crate::platform::shell::show_properties(path)
}
