#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(error) = smartdisk::app::run() {
        eprintln!("SmartDisk failed to start: {error}");
    }
}
