# Git Raccoon

[![CI](https://github.com/Trashpanda-GameDev/Git-Raccoon/actions/workflows/ci.yml/badge.svg)](https://github.com/Trashpanda-GameDev/Git-Raccoon/actions/workflows/ci.yml)

A fast, friendly desktop Git client, written in Rust with [egui](https://github.com/emilk/egui) (eframe on wgpu) and [git2](https://docs.rs/git2).

**Website and downloads: [trashpanda-gamedev.github.io/Git-Raccoon](https://trashpanda-gamedev.github.io/Git-Raccoon/)**

See the [user guide](docs/guide.md) for how to use it, and [ROADMAP.md](ROADMAP.md) for what is planned.

## Layout

- `crates/git-raccoon-core`: the Git logic: history and graph layout, commit details and diffs, staging, committing, branches, tags, stashes, undo, fetch/pull/push, search, and watching for changes. No UI code.
- `crates/git-raccoon`: the desktop app.
- `site/`: the website. `python3 scripts/build-site.py` builds it into `_site/` (with the user guide), and the Website workflow publishes it on GitHub Pages when it changes. Its screenshots come from the app: `cargo test -p git-raccoon --test site_screenshots -- --ignored`.
- `installer/`: the Windows installer that asks where to install (an Inno Setup script), the macOS installer's welcome and closing pages, and the installers' artwork, drawn from the app icon by `python3 scripts/make-artwork.py`.

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

## Releases and updates

Installers for Windows, macOS (Apple Silicon) and Linux (an AppImage) are published on this repository's [Releases](https://github.com/Trashpanda-GameDev/Git-Raccoon/releases) page. An installed Git Raccoon checks there now and then, downloads a newer version in the background, and offers to restart into it from the toolbar (Settings can turn the checks off). Installers are built with [Velopack](https://velopack.io); on Windows, `installer/windows.iss` puts an [Inno Setup](https://jrsoftware.org/isinfo.php) wizard in front of Velopack's setup, to ask where to install; copies run with `cargo run` don't update themselves.

### Making a release

1. Set the version in the root `Cargo.toml` (for example `0.5.1`), and commit it.
2. Tag that commit and push the tag:

   ```bash
   git tag v0.5.1
   git push origin v0.5.1
   ```

   Or push the commit to the `release` branch (`git push origin main:release`): the release then creates the tag itself. It refuses a version that's already released.

The Release workflow builds the three installers and publishes them as one release (with GitHub's built-in token: nothing to set up). To try them first, run the workflow by hand (Actions > Release > Run workflow): it builds them and keeps them as downloadable artifacts without publishing.

They aren't code-signed yet. On Windows, SmartScreen warns ("More info", then "Run anyway") until the installer builds a reputation. On macOS, right-click the app and choose Open the first time (or allow it in System Settings > Privacy & Security).

## License

Git Raccoon is open source, licensed under either of

- the Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE)), or
- the MIT license ([LICENSE-MIT](LICENSE-MIT)),

at your option. It's built on other open-source software, whose licenses are collected in [THIRD-PARTY-NOTICES.txt](crates/git-raccoon/assets/THIRD-PARTY-NOTICES.txt) (shipped with the app, and shown in Settings > Licenses). After changing dependencies, regenerate it with `python3 scripts/third-party-notices.py`.

### Contributing

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in Git Raccoon by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

## Keyboard shortcuts

**Ctrl+K** (or **Ctrl+Shift+P**) opens the command palette: every action, plus checking out, merging, rebasing onto and going to each branch. Type a few letters, choose with the arrow keys, press Enter. On macOS, use Cmd for Ctrl.

| Action | Shortcut |
| --- | --- |
| Command palette | Ctrl+K, Ctrl+Shift+P |
| Settings (theme, interface size, diff layout) | Ctrl+, |
| Open repository | Ctrl+O |
| New tab / Close tab | Ctrl+T / Ctrl+W |
| Next / previous tab | Ctrl+Tab / Ctrl+Shift+Tab |
| Go to tab 1–9 | Ctrl+1 … Ctrl+9 |
| Refresh | F5 |
| Search commits | Ctrl+F |
| Show or hide the sidebar | Ctrl+B |
| Undo the last action | Ctrl+Z |
| Fetch / Pull / Push | Ctrl+Shift+F / Ctrl+Shift+D / Ctrl+Shift+U |
| Show working changes | Ctrl+Shift+W |
| Write a commit message | Ctrl+Shift+M (then Ctrl+Enter to commit) |
| Stage all / Unstage all | Ctrl+Shift+S / Ctrl+Shift+A |
| Stash changes | Ctrl+Shift+H |
| New branch / New tag | Ctrl+Shift+B / Ctrl+Shift+T |
| Move through commits | Up, Down, Page Up, Page Down |
| Close the open diff, conflict or plan | Esc |

Ctrl+Shift shortcuts also work while typing in a text field; the others keep their usual text-editing meaning there.

## Tests

```bash
cargo test --workspace
```

The UI tests in `crates/git-raccoon/tests/ui.rs` render the real app headlessly. They need a GPU, or a software renderer such as Mesa's (`mesa-vulkan-drivers` on Linux). Set `GIT_RACCOON_SCREENSHOTS=<dir>` to save a screenshot of each step.

To check performance on a large repository, make one (a million commits with feature branches and merges takes about half a minute) and run the big-repo test against it:

```bash
git init -b main big && cd big && python3 ../scripts/gen-big-repo.py 1000000 | git fast-import --quiet && git checkout -f main && cd ..
GIT_RACCOON_BIG_REPO=big cargo test --release -p git-raccoon --test ui big_repo -- --ignored --nocapture
```

It reports how long opening took, the slowest frame while jumping and paging around, and peak memory. On a 4-core machine, a million commits open in about 3 seconds (6 the first time, before Git's commit-graph cache exists; the app writes it), every frame stays under 10 ms, and memory peaks around 400 MB.

CI (GitHub Actions) runs on every push to `main` and every pull request: on Linux, the format check, the lints and every test, UI included (on Mesa's software renderer, with the screenshots kept as an artifact); on Windows and macOS, a release build and the Git-logic and app unit tests (CI machines there can't reliably draw, so the UI tests run on Linux only).

Lints are strict (`cargo clippy --workspace --all-targets`): exceptions use `#[expect(..., reason = "...")]`.
