//! Decoding the before/after versions of image files for previews.

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use git_raccoon_core::details::{BlobData, FileBlobs};

/// Largest preview texture side; bigger images are scaled down.
const MAX_SIDE: u32 = 4096;

const EXTENSIONS: [&str; 12] = [
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "tga", "tif", "tiff", "qoi", "jfif",
];

/// Whether a path is an image format we can preview.
pub fn is_image(path: &str) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_, ext)| EXTENSIONS.contains(&ext.to_lowercase().as_str()))
}

pub enum Preview {
    Image {
        texture: TextureHandle,
        width: u32,
        height: u32,
    },
    TooLarge,
    Failed(String),
}

pub struct Side {
    /// Size of the file in bytes.
    pub size: usize,
    pub preview: Preview,
}

/// The two versions of an image file; a side is `None` when the file did
/// not exist there (added or deleted in the commit).
pub struct ImagePair {
    pub old: Option<Side>,
    pub new: Option<Side>,
}

impl ImagePair {
    pub fn decode(ctx: &egui::Context, file: &str, blobs: FileBlobs) -> Self {
        let side = |blob: Option<BlobData>, which: &str| {
            blob.map(|b| Side {
                size: b.size,
                preview: match b.bytes {
                    None => Preview::TooLarge,
                    Some(bytes) => decode(ctx, &format!("{which}:{file}"), &bytes),
                },
            })
        };
        ImagePair {
            old: side(blobs.old, "old"),
            new: side(blobs.new, "new"),
        }
    }
}

fn decode(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Preview {
    let image = match image::load_from_memory(bytes) {
        Ok(img) => img,
        Err(e) => return Preview::Failed(e.to_string()),
    };
    let (width, height) = (image.width(), image.height());
    let image = if width > MAX_SIDE || height > MAX_SIDE {
        image.thumbnail(MAX_SIDE, MAX_SIDE)
    } else {
        image
    };
    let rgba = image.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let texture = ctx.load_texture(
        name,
        ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()),
        TextureOptions::LINEAR,
    );
    Preview::Image {
        texture,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_image_paths() {
        assert!(is_image("assets/icon.PNG"));
        assert!(is_image("art/hero.tga"));
        assert!(!is_image("badge.svg"));
        assert!(!is_image("README.md"));
        assert!(!is_image("png"));
    }
}
