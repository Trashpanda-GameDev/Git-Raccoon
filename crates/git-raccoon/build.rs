//! Embeds the app icon and name in the Windows executable. The feedback
//! relay's address (GIT_RACCOON_FEEDBACK_RELAY) is read with option_env!,
//! so a change to it rebuilds.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-env-changed=GIT_RACCOON_FEEDBACK_RELAY");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Git Raccoon");
        res.set("FileDescription", "Git Raccoon");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the Windows icon: {e}");
        }
    }
}
