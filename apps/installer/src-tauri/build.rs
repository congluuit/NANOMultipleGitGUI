use std::path::PathBuf;

/// The app's NSIS installer (`*_x64-setup.exe`) is embedded into the setup binary. Release
/// builds point `NANOMGG_SETUP_PAYLOAD` at it; without it the payload is empty and the setup
/// only shows its pages (development).
fn main() {
    println!("cargo:rerun-if-env-changed=NANOMGG_SETUP_PAYLOAD");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("payload.exe");
    match std::env::var("NANOMGG_SETUP_PAYLOAD")
        .ok()
        .filter(|p| !p.trim().is_empty())
    {
        Some(src) => {
            println!("cargo:rerun-if-changed={src}");
            std::fs::copy(&src, &out)
                .unwrap_or_else(|e| panic!("NANOMGG_SETUP_PAYLOAD={src}: {e}"));
        }
        None => std::fs::write(&out, b"").unwrap(),
    }
    tauri_build::build();
}
