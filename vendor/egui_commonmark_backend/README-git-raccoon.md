# egui_commonmark_backend 0.25.0, patched

A copy of [egui_commonmark_backend](https://crates.io/crates/egui_commonmark_backend)
0.25.0 (MIT or Apache-2.0, see its license files here), used through
`[patch.crates-io]` in the workspace's `Cargo.toml`.

One change, in `src/misc.rs` (`Link::end`): a link's text gets the link
colour, and the line's vertical alignment instead of the top of the row.
Without it, links in Markdown sat higher than the text around them and
weren't coloured as links. Drop this copy once a release fixes that.
