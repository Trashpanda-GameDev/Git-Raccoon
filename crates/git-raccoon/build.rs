//! Embeds the app icon and name in the Windows executable, and the tokens
//! the feedback form posts issues and uploads screenshots with (when the
//! release build has them).

/// Mixed into the token, so it isn't plain text in the executable. It
/// keeps it from a casual search, not from anyone determined.
const KEY: &[u8] = b"raccoons-wash-their-food-in-the-river";

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    let scramble = |var: &str| -> String {
        println!("cargo:rerun-if-env-changed={var}");
        let token = std::env::var(var).unwrap_or_default();
        token
            .trim()
            .bytes()
            .zip(KEY.iter().cycle())
            .map(|(b, k)| (b ^ k).to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let issues = scramble("GIT_RACCOON_FEEDBACK_TOKEN");
    let screenshots = scramble("GIT_RACCOON_SCREENSHOT_TOKEN");
    let out =
        std::path::Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("feedback_token.rs");
    std::fs::write(
        out,
        format!(
            "const SCRAMBLED: &[u8] = &[{issues}];\nconst SCREENSHOT_SCRAMBLED: &[u8] = &[{screenshots}];\nconst KEY: &[u8] = &{KEY:?};\n"
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
