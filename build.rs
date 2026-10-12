//! The daemon embeds the web UI bundle that `scripts/build-web.sh` generates.
//! Fail early, with instructions, if it has not been built yet.

use std::path::Path;

const BUNDLE: [&str; 2] = ["src/web/pkg/ttlcd_web.js", "src/web/pkg/ttlcd_web_bg.wasm"];

fn main() {
    for file in BUNDLE {
        println!("cargo:rerun-if-changed={file}");
        if !Path::new(file).exists() {
            panic!(
                "{file} is missing: run scripts/build-web.sh (inside `nix develop`) \
                 to build the web UI before building the daemon"
            );
        }
    }
}
