//! Shared page images for rasterized glyphs.
//!
//! Drawing each glyph from its own image costs one texture per glyph on the
//! GPU, and Ganesh's op batching is quadratic in distinct textures: 40 lines
//! of text took ~120 ms per frame against ~7 ms on the CPU. Packing glyphs
//! into a few pages lets consecutive glyph draws share one texture.
//!
//! Placed pixels never move or change, so a page snapshot stays valid for
//! every glyph placed before it. A glyph placed after the latest snapshot
//! draws from its own image until [`GlyphPages::refresh`] snapshots the page
//! again, which bounds snapshot copies to one per dirty page per text node.

use skia_safe::{AlphaType, ColorType, Data, IRect, Image, ImageInfo, images};

const PAGE_SIZE: u32 = 1024;
/// One pixel of transparent gutter keeps filtered or transformed draws from
/// sampling a neighbor.
const GUTTER: u32 = 1;
/// Larger glyphs (big display text, emoji at large sizes) keep their own image.
pub(super) const MAX_PAGED_GLYPH: u32 = 128;
const MAX_MASK_PAGES: usize = 4;
const MAX_COLOR_PAGES: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PageSlot {
    page: usize,
    sequence: u64,
    pub(super) src: IRect,
}

struct GlyphPage {
    color: bool,
    pixels: Vec<u8>,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
    placed: u64,
    snapshot: Option<Image>,
    snapshot_placed: u64,
}

impl GlyphPage {
    fn new(color: bool) -> Self {
        Self {
            color,
            pixels: vec![0; (PAGE_SIZE * PAGE_SIZE * bytes_per_pixel(color)) as usize],
            cursor_x: 0,
            cursor_y: 0,
            row_height: 0,
            placed: 0,
            snapshot: None,
            snapshot_placed: 0,
        }
    }

    /// Reserve a `width`×`height` cell on the current or next shelf.
    fn reserve(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        let (cell_width, cell_height) = (width + GUTTER, height + GUTTER);
        if self.cursor_x + cell_width > PAGE_SIZE {
            self.cursor_x = 0;
            self.cursor_y += self.row_height;
            self.row_height = 0;
        }
        if self.cursor_y + cell_height > PAGE_SIZE {
            return None;
        }
        let origin = (self.cursor_x, self.cursor_y);
        self.cursor_x += cell_width;
        self.row_height = self.row_height.max(cell_height);
        Some(origin)
    }

    fn used_height(&self) -> u32 {
        (self.cursor_y + self.row_height).min(PAGE_SIZE)
    }

    fn snapshot_if_stale(&mut self) {
        if self.snapshot_placed == self.placed {
            return;
        }
        let height = self.used_height().max(1);
        let row_bytes = (PAGE_SIZE * bytes_per_pixel(self.color)) as usize;
        let info = ImageInfo::new(
            (PAGE_SIZE as i32, height as i32),
            if self.color {
                ColorType::RGBA8888
            } else {
                ColorType::Alpha8
            },
            AlphaType::Premul,
            None,
        );
        let data = Data::new_copy(&self.pixels[..row_bytes * height as usize]);
        if let Some(image) = images::raster_from_data(&info, data, row_bytes) {
            self.snapshot = Some(image);
            self.snapshot_placed = self.placed;
        }
    }
}

fn bytes_per_pixel(color: bool) -> u32 {
    if color { 4 } else { 1 }
}

#[derive(Default)]
pub(super) struct GlyphPages {
    pages: Vec<GlyphPage>,
}

impl GlyphPages {
    /// Copy a glyph bitmap into a page. `None` when the glyph is too large to
    /// page or every page of its kind is full; the caller then clears the
    /// pages together with every slot it handed out.
    pub(super) fn place(
        &mut self,
        color: bool,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Option<PageSlot> {
        if width == 0 || height == 0 || width > MAX_PAGED_GLYPH || height > MAX_PAGED_GLYPH {
            return None;
        }
        let bpp = bytes_per_pixel(color);
        if pixels.len() != (width * height * bpp) as usize {
            return None;
        }
        let (page, (x, y)) = match self
            .pages
            .iter_mut()
            .enumerate()
            .filter(|(_, page)| page.color == color)
            .find_map(|(index, page)| page.reserve(width, height).map(|origin| (index, origin)))
        {
            Some(found) => found,
            None => {
                let limit = if color {
                    MAX_COLOR_PAGES
                } else {
                    MAX_MASK_PAGES
                };
                if self.pages.iter().filter(|page| page.color == color).count() >= limit {
                    return None;
                }
                let mut page = GlyphPage::new(color);
                let origin = page.reserve(width, height)?;
                self.pages.push(page);
                (self.pages.len() - 1, origin)
            }
        };
        let target = &mut self.pages[page];
        let page_row = (PAGE_SIZE * bpp) as usize;
        let glyph_row = (width * bpp) as usize;
        for row in 0..height as usize {
            let start = (y as usize + row) * page_row + (x * bpp) as usize;
            target.pixels[start..start + glyph_row]
                .copy_from_slice(&pixels[row * glyph_row..(row + 1) * glyph_row]);
        }
        target.placed += 1;
        Some(PageSlot {
            page,
            sequence: target.placed,
            src: IRect::from_xywh(x as i32, y as i32, width as i32, height as i32),
        })
    }

    /// The page image holding `slot`, or `None` when the slot was placed after
    /// the page's latest snapshot.
    pub(super) fn image(&self, slot: PageSlot) -> Option<&Image> {
        let page = self.pages.get(slot.page)?;
        if slot.sequence > page.snapshot_placed {
            return None;
        }
        page.snapshot.as_ref()
    }

    /// Snapshot every page that gained glyphs since its last snapshot.
    pub(super) fn refresh(&mut self) {
        for page in &mut self.pages {
            page.snapshot_if_stale();
        }
    }

    pub(super) fn clear(&mut self) {
        self.pages.clear();
    }

    #[cfg(test)]
    pub(super) fn bytes(&self) -> usize {
        self.pages.iter().map(|page| page.pixels.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placed_glyphs_join_the_next_snapshot_and_keep_their_pixels() {
        let mut pages = GlyphPages::default();
        let first = pages.place(false, 2, 2, &[1, 2, 3, 4]).unwrap();
        assert!(pages.image(first).is_none(), "not snapshotted yet");
        pages.refresh();
        let snapshot = pages.image(first).unwrap().clone();
        let second = pages.place(false, 3, 1, &[5, 6, 7]).unwrap();
        assert!(pages.image(second).is_none());
        assert!(
            pages.image(first).is_some(),
            "earlier snapshot still covers it"
        );
        pages.refresh();
        let image = pages.image(second).unwrap();
        let pixmap = image.peek_pixels().unwrap();
        let bytes = pixmap.bytes().unwrap();
        let at = |x: i32, y: i32| bytes[(y as usize) * PAGE_SIZE as usize + x as usize];
        assert_eq!(
            [
                at(first.src.left, first.src.top),
                at(first.src.left + 1, first.src.top + 1)
            ],
            [1, 4]
        );
        assert_eq!(at(second.src.left + 2, second.src.top), 7);
        assert_eq!(
            at(first.src.right(), first.src.top),
            0,
            "gutter stays clear"
        );
        drop(snapshot);
    }

    #[test]
    fn oversized_glyphs_and_full_pages_are_refused() {
        let mut pages = GlyphPages::default();
        let big = vec![0; (MAX_PAGED_GLYPH as usize + 1).pow(2)];
        assert!(
            pages
                .place(false, MAX_PAGED_GLYPH + 1, MAX_PAGED_GLYPH + 1, &big)
                .is_none()
        );
        let cell = vec![0; (MAX_PAGED_GLYPH * MAX_PAGED_GLYPH) as usize];
        let per_page = (PAGE_SIZE / (MAX_PAGED_GLYPH + GUTTER)).pow(2) as usize;
        for _ in 0..per_page * MAX_MASK_PAGES {
            assert!(
                pages
                    .place(false, MAX_PAGED_GLYPH, MAX_PAGED_GLYPH, &cell)
                    .is_some()
            );
        }
        assert!(
            pages
                .place(false, MAX_PAGED_GLYPH, MAX_PAGED_GLYPH, &cell)
                .is_none()
        );
        assert_eq!(
            pages.bytes(),
            MAX_MASK_PAGES * (PAGE_SIZE * PAGE_SIZE) as usize
        );
    }
}
