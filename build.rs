//! Embeds the tray/app icons as Windows resources (ADR 0010). Ordinal 1 is
//! also picked up by Explorer as the executable's icon.

fn main() {
    println!("cargo:rerun-if-changed=assets/kbd.ico");
    println!("cargo:rerun-if-changed=assets/kbd-disabled.ico");
    // CARGO_CFG_WINDOWS guards the resource compiler for any future
    // cross-compilation from non-Windows hosts.
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        winresource::WindowsResource::new()
            .set_icon_with_id("assets/kbd.ico", "1")
            .set_icon_with_id("assets/kbd-disabled.ico", "2")
            .compile()
            .expect("failed to embed icon resources");
    }
}
