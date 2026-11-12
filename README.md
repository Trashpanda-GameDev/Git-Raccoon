# Git Raccoon

A graphical Git client, built with [Tauri 2](https://tauri.app) (Rust + [git2](https://docs.rs/git2)) and React + TypeScript.

## Development

Prerequisites: Node.js 22+, Rust (stable), and the [Tauri Linux system packages](https://tauri.app/start/prerequisites/#linux)
(`libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev build-essential pkg-config`).

```bash
npm install
npm run tauri dev      # run the app with hot reload
npm run build          # type-check and build the frontend
cd src-tauri && cargo test
```

On a headless machine, run the app under a virtual display: `xvfb-run -a npm run tauri dev`.
