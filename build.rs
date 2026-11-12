fn main() {
    println!("cargo:rerun-if-changed=ui");
    println!("cargo:rerun-if-changed=resources");

    #[cfg(windows)]
    embed_resource::compile("resources/smartdisk.rc", embed_resource::NONE)
        .manifest_optional()
        .expect("failed to embed Windows resources");

    if let Err(err) = slint_build::compile("ui/main_window.slint") {
        panic!("failed to compile Slint UI: {err}");
    }
}
