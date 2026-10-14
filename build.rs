//! Embeds the app icon and version info into tide.exe, so Explorer, the
//! taskbar and Alt+Tab show the tide icon and the file's Details tab shows
//! the version from Cargo.toml.

fn main() {
    println!("cargo:rerun-if-changed=assets/tide.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/tide.ico");
    // Task Manager shows FileDescription as the process name, so keep it short.
    res.set("FileDescription", "tide");
    res.set("LegalCopyright", "github.com/Deep7k/tide");
    // FileVersion, ProductVersion and ProductName come from Cargo.toml.
    res.compile().expect("embed Windows resources (icon, version info)");
}
