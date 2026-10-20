//! Small immutable raster assets share upload identities across scene frames.

use super::glyph_pages::{GlyphPages, MAX_PAGED_GLYPH, PageSlot};
use mesh_core_elements::lru::ByteLruCache;
use skia_safe::{AlphaType, ColorType, Image, ImageInfo, Rect};
use std::cell::RefCell;

struct ImagePages {
    pages: GlyphPages,
    slots: ByteLruCache<u32, (Image, Option<PageSlot>)>,
    revision: u64,
}

impl Default for ImagePages {
    fn default() -> Self {
        Self {
            pages: GlyphPages::default(),
            slots: ByteLruCache::new(2048, 16 * 1024 * 1024),
            revision: 0,
        }
    }
}

thread_local! {
    static PAGES: RefCell<ImagePages> = RefCell::new(ImagePages::default());
}

pub(crate) fn refresh() {
    PAGES.with(|pages| {
        let mut pages = pages.borrow_mut();
        let revision = mesh_core_resources::resource_revision();
        if pages.revision != revision {
            *pages = ImagePages {
                revision,
                ..ImagePages::default()
            };
        }
        pages.pages.refresh();
    });
}

/// Newly placed assets use their standalone image until the next scene begins.
/// Old snapshots remain immutable when pages grow or the bounded atlas resets.
pub(crate) fn resolve(image: &Image) -> (Image, Option<Rect>) {
    if !paging_enabled()
        || !crate::prepared_scene::recording_images()
        || image.width() > MAX_PAGED_GLYPH as i32
        || image.height() > MAX_PAGED_GLYPH as i32
    {
        return (image.clone(), None);
    }
    PAGES.with(|pages| {
        let mut pages = pages.borrow_mut();
        if let Some((_, slot)) = pages.slots.get(&image.unique_id()) {
            let Some(slot) = *slot else {
                return (image.clone(), None);
            };
            return pages.pages.image(slot).map_or_else(
                || (image.clone(), None),
                |page| (page.clone(), Some(Rect::from(slot.src))),
            );
        }
        let color = image.color_type() != ColorType::Alpha8;
        let info = ImageInfo::new(
            image.dimensions(),
            if color {
                ColorType::RGBA8888
            } else {
                ColorType::Alpha8
            },
            AlphaType::Premul,
            None,
        );
        let row_bytes = image.width() as usize * if color { 4 } else { 1 };
        let mut pixels = vec![0; row_bytes * image.height() as usize];
        if !image.read_pixels(
            &info,
            &mut pixels,
            row_bytes,
            (0, 0),
            skia_safe::image::CachingHint::Allow,
        ) {
            return (image.clone(), None);
        }
        let slot = pages
            .pages
            .place(color, image.width() as u32, image.height() as u32, &pixels);
        pages
            .slots
            .insert(image.unique_id(), (image.clone(), slot), pixels.len());
        (image.clone(), None)
    })
}

#[cfg(not(test))]
fn paging_enabled() -> bool {
    true
}

#[cfg(test)]
thread_local! { static PAGING_ENABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(true) }; }
#[cfg(test)]
fn paging_enabled() -> bool {
    PAGING_ENABLED.with(|enabled| enabled.get())
}

#[cfg(test)]
pub(crate) struct TestPaging(bool);
#[cfg(test)]
impl TestPaging {
    pub(crate) fn new(enabled: bool) -> Self {
        Self(PAGING_ENABLED.with(|current| current.replace(enabled)))
    }
}
#[cfg(test)]
impl Drop for TestPaging {
    fn drop(&mut self) {
        PAGING_ENABLED.with(|current| current.set(self.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prepared_scene::{SceneImages, with_images};
    use skia_safe::{Data, Paint, images};

    fn source(color: bool, value: u8) -> Image {
        let info = ImageInfo::new(
            (8, 8),
            if color {
                ColorType::BGRA8888
            } else {
                ColorType::Alpha8
            },
            if color {
                AlphaType::Unpremul
            } else {
                AlphaType::Premul
            },
            None,
        );
        let bytes = vec![value; 64 * if color { 4 } else { 1 }];
        images::raster_from_data(&info, Data::new_copy(&bytes), 8 * if color { 4 } else { 1 })
            .unwrap()
    }

    fn draw(image: &Image, source: Option<Rect>) -> crate::PixelBuffer {
        let mut pixels = crate::PixelBuffer::new(16, 16);
        crate::PixelCanvasSession::new(&mut pixels).with_canvas(|canvas| {
            canvas.draw_image_rect(
                image,
                source
                    .as_ref()
                    .map(|rect| (rect, skia_safe::canvas::SrcRectConstraint::Strict)),
                Rect::from_xywh(3., 3., 8., 8.),
                &Paint::default(),
            );
        });
        pixels
    }

    #[test]
    fn page_snapshots_share_identity_and_preserve_rgba_and_mask_pixels() {
        let _guard = crate::resource_state_test_lock();
        PAGES.with(|pages| *pages.borrow_mut() = ImagePages::default());
        refresh();
        for color in [true, false] {
            refresh();
            let first = source(color, 160);
            let second = source(color, 80);
            let images = SceneImages::default();
            with_images(&images, || {
                assert!(resolve(&first).1.is_none());
                assert!(resolve(&second).1.is_none());
                refresh();
                let (page, rect) = resolve(&first);
                let (other_page, other_rect) = resolve(&second);
                assert!(rect.is_some() && other_rect.is_some());
                assert_eq!(page.unique_id(), other_page.unique_id());
                assert_eq!(draw(&first, None).data(), draw(&page, rect).data());
                assert_eq!(
                    draw(&second, None).data(),
                    draw(&other_page, other_rect).data()
                );
                assert_eq!(resolve(&first).0.unique_id(), page.unique_id());
                PAGES.with(|pages| *pages.borrow_mut() = ImagePages::default());
                assert_eq!(
                    draw(&first, None).data(),
                    draw(&page, rect).data(),
                    "retired pages remain immutable"
                );
            });
        }
    }

    #[test]
    fn a_full_atlas_keeps_existing_cells_and_uses_standalone_overflow() {
        let _guard = crate::resource_state_test_lock();
        PAGES.with(|pages| *pages.borrow_mut() = ImagePages::default());
        refresh();
        let images = SceneImages::default();
        let info = ImageInfo::new((128, 128), ColorType::RGBA8888, AlphaType::Premul, None);
        let mut sources = Vec::new();
        with_images(&images, || {
            for _ in 0..100 {
                let image = images::raster_from_data(
                    &info,
                    Data::new_copy(&vec![255; 128 * 128 * 4]),
                    128 * 4,
                )
                .unwrap();
                resolve(&image);
                sources.push(image);
            }
            refresh();
            assert!(resolve(&sources[0]).1.is_some());
            assert!(resolve(sources.last().unwrap()).1.is_none());
            assert!(
                resolve(&sources[0]).1.is_some(),
                "overflow must not churn earlier upload identities"
            );
            PAGES.with(|pages| {
                let pages = pages.borrow();
                assert!(pages.pages.bytes() <= 8 * 1024 * 1024);
                assert!(pages.slots.bytes() <= pages.slots.max_bytes());
            });
        });
    }

    #[test]
    fn raster_sessions_keep_the_original_image() {
        let image = source(true, 128);
        let (resolved, rect) = resolve(&image);
        assert_eq!(resolved.unique_id(), image.unique_id());
        assert!(rect.is_none());
    }
}
