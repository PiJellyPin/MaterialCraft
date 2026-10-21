// SPDX-License-Identifier: MIT OR Apache-2.0
use std::{env, fs, path::PathBuf};

/// Embeds assets/logo.png when it exists.
fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let out = PathBuf::from(env::var("OUT_DIR").unwrap_or_default());
    let logo = root.join("assets/logo.png");
    println!("cargo:rerun-if-changed=assets/logo.png");
    println!("cargo:rerun-if-changed=build.rs");
    let code = if logo.is_file() {
        format!("pub const LOGO_PNG: Option<&[u8]> = Some(include_bytes!({:?}));", logo.to_string_lossy())
    } else {
        "pub const LOGO_PNG: Option<&[u8]> = None;".to_string()
    };
    let _ = fs::write(out.join("logo.rs"), code);
}
