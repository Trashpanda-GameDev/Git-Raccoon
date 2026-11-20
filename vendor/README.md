# Patched crates

Copies of the Markdown viewer used for requests, issues and comments,
[egui_commonmark](https://crates.io/crates/egui_commonmark) and
[egui_commonmark_backend](https://crates.io/crates/egui_commonmark_backend)
0.25.0 (MIT or Apache-2.0, see the license files in each), used through
`[patch.crates-io]` in the workspace's `Cargo.toml`.

Each change is marked with a "Git Raccoon's change" comment:

- Links sit on the line with the text around them, in the link colour.
- List markers line up with their item's text, and the rows of a list are
  as tall as the text. A task's item has its checkbox and no bullet.
- Bold text uses a real bold font when the app provides one (`BOLD`).
- Headings come in steps, all bold, with a line under the top two levels.
- Tables have a bold header and room between cells.
- Code blocks have room around the code, and the app can highlight them
  itself (`HIGHLIGHT`).

To move to a newer release, copy it here and carry the marked changes over,
or drop a copy once a release covers its changes.
