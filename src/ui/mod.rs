slint::include_modules!();

pub mod backend;

/// Entry point for the desktop UI. All state, background services, and
/// callbacks live in `backend`; this module only exists to host the generated
/// Slint types (`include_modules!` must stay here so `super::*` works).
pub fn run() -> Result<(), slint::PlatformError> {
    backend::run()
}
