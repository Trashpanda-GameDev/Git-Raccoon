# Git Raccoon

A fast, friendly desktop Git client, written in Rust with [egui](https://github.com/emilk/egui) (eframe on wgpu) and [git2](https://docs.rs/git2).

See [ROADMAP.md](ROADMAP.md) for what is planned.

## Layout

- `crates/git-raccoon-core`: the Git logic: history and graph layout, commit details and diffs, staging, committing, branches, tags, stashes, undo, fetch/pull/push, search, and watching for changes. No UI code.
- `crates/git-raccoon`: the desktop app.

## Running it

You need [Rust](https://rustup.rs) (stable) and [Git](https://git-scm.com). Git Raccoon reads repositories itself, but runs your own `git` to change them (commit, check out, fetch, push and so on), so your hooks, signing, SSH agent and credential helpers apply just as on the command line.

Then, from the repository root:

```bash
cargo run --release
```

To open a repository straight away, pass its path:

```bash
cargo run --release -- path/to/repo
```

`cargo run` without `--release` also works and starts faster to build; dependencies are optimised in dev builds too.

### Per-platform setup

- **Windows:** Rust's installer sets up the Visual Studio C++ Build Tools it needs. Nothing else is required.
- **macOS:** `xcode-select --install`.
- **Linux (Debian/Ubuntu):** `sudo apt install build-essential pkg-config libssl-dev libxkbcommon-dev libwayland-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev libvulkan1 mesa-vulkan-drivers`

## Tests

```bash
cargo test --workspace
```

The UI tests in `crates/git-raccoon/tests/ui.rs` render the real app headlessly. They need a GPU, or a software renderer such as Mesa's (`mesa-vulkan-drivers` on Linux). Set `GIT_RACCOON_SCREENSHOTS=<dir>` to save a screenshot of each step.

To check performance on a large repository:

```bash
GIT_RACCOON_BIG_REPO=path/to/big/repo cargo test --release -p git-raccoon --test ui big_repo -- --ignored --nocapture
```

Lints are strict (`cargo clippy --workspace --all-targets`): exceptions use `#[expect(..., reason = "...")]`.
