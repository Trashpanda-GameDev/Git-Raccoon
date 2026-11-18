//! Hand cursors for things that can be taken hold of and moved (the
//! graph's column headings): an open hand over one, a closed hand while
//! holding it.
//!
//! The system's own "grab" cursors show four arrows on Windows (winit
//! gives it the move cursor for both), so these are images: pixel art,
//! drawn by `scripts/cursors.py`. Where images can't be cursors, the
//! system's grab cursors stand in.

use std::sync::{Arc, OnceLock};

use eframe::egui::{self, CursorIcon, CustomCursorImage};

/// Each hand at its size and doubled (pixel art doesn't scale by halves).
const GRAB: [&[u8]; 2] = [
    include_bytes!("../../assets/cursors/grab-32.png"),
    include_bytes!("../../assets/cursors/grab-64.png"),
];
const GRABBING: [&[u8]; 2] = [
    include_bytes!("../../assets/cursors/grabbing-32.png"),
    include_bytes!("../../assets/cursors/grabbing-64.png"),
];

/// An open hand: this can be taken hold of.
pub fn grab(ctx: &egui::Context) {
    static IMAGES: OnceLock<Vec<CustomCursorImage>> = OnceLock::new();
    show(ctx, CursorIcon::Grab, IMAGES.get_or_init(|| decode(&GRAB)));
}

/// A closed hand: holding it.
pub fn grabbing(ctx: &egui::Context) {
    static IMAGES: OnceLock<Vec<CustomCursorImage>> = OnceLock::new();
    show(
        ctx,
        CursorIcon::Grabbing,
        IMAGES.get_or_init(|| decode(&GRABBING)),
    );
}

/// The image nearest the size the system's cursors have at this scaling
/// (kept, so the same bitmap isn't sent to the system again each frame).
fn show(ctx: &egui::Context, icon: CursorIcon, images: &[CustomCursorImage]) {
    let wanted = 32.0 * ctx.pixels_per_point();
    let nearest = images
        .iter()
        .min_by(|a, b| {
            let off = |i: &CustomCursorImage| (f32::from(i.size[0]) - wanted).abs();
            off(a).total_cmp(&off(b))
        })
        .cloned();
    ctx.set_cursor_icon(icon);
    ctx.set_cursor_image(nearest);
}

/// The images, held at the middle of the hand.
fn decode(pngs: &[&[u8]]) -> Vec<CustomCursorImage> {
    pngs.iter()
        .filter_map(|png| {
            let image = image::load_from_memory(png).ok()?.to_rgba8();
            let (w, h) = image.dimensions();
            let size = [u16::try_from(w).ok()?, u16::try_from(h).ok()?];
            Some(CustomCursorImage {
                rgba: Arc::from(image.into_raw()),
                size,
                hotspot: [size[0] / 2, size[1] / 2],
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_hand_decodes_at_its_size() {
        for (pngs, sizes) in [(&GRAB, [32, 64]), (&GRABBING, [32, 64])] {
            let images = decode(pngs);
            assert_eq!(images.iter().map(|i| i.size[0]).collect::<Vec<_>>(), sizes);
            for i in &images {
                assert_eq!(
                    i.rgba.len(),
                    usize::from(i.size[0]) * usize::from(i.size[1]) * 4
                );
            }
        }
    }
}
