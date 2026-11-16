//! Embeds the app icon and name in the Windows executable, and the token
//! the feedback form posts issues with (when the release build has one).

/// Mixed into the token, so it isn't plain text in the executable. It
/// keeps it from a casual search, not from anyone determined.
const KEY: &[u8] = b"raccoons-wash-their-food-in-the-river";

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-env-changed=GIT_RACCOON_FEEDBACK_TOKEN");
    let token = std::env::var("GIT_RACCOON_FEEDBACK_TOKEN").unwrap_or_default();
    let scrambled: Vec<String> = token
        .trim()
        .bytes()
        .zip(KEY.iter().cycle())
        .map(|(b, k)| (b ^ k).to_string())
        .collect();
    let out =
        std::path::Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("feedback_token.rs");
    std::fs::write(
        out,
        format!(
            "const SCRAMBLED: &[u8] = &[{}];\nconst KEY: &[u8] = &{KEY:?};\n",
            scrambled.join(", ")
        ),
    )
    .expect("write feedback_token.rs");
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
