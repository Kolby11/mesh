//! The final damage stage shared by paint and presentation.
//!
//! Damage producers (retained display-list diffs, visual and tooltip damage,
//! blur expansion, debug overlays) report logical rectangles that may overlap.
//! [`DeviceDamage`] converts them once to device pixels with the
//! [`FractionalScale`] cover rule, clips them to the buffer and splits them
//! into disjoint rectangles. The shell clears and paints exactly these, and
//! presentation copies and reports exactly these, so no pixel is cleared,
//! painted, copied or sent to `damage_buffer` twice in one frame.

use crate::display_list::DamageRect;
use crate::fractional_scale::FractionalScale;
use smallvec::SmallVec;

/// Most disjoint rectangles one frame keeps. Past this the set collapses to
/// its bounding box: matching the protocol's per-commit damage budget keeps
/// paint regions and `damage_buffer` calls the same set.
pub const MAX_DEVICE_DAMAGE_RECTS: usize = 16;

/// Disjoint device-pixel damage for one buffer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceDamage {
    rects: SmallVec<[DamageRect; MAX_DEVICE_DAMAGE_RECTS]>,
}

impl DeviceDamage {
    /// Convert logical rectangles to disjoint device rectangles inside a
    /// `buffer_width` × `buffer_height` buffer.
    pub fn from_logical(
        logical: &[DamageRect],
        scale: f32,
        buffer_width: u32,
        buffer_height: u32,
    ) -> Self {
        let scale = FractionalScale::new(scale);
        let mut damage = Self::default();
        for rect in logical {
            if let Some(device) = scale.clip_damage_rect(*rect, buffer_width, buffer_height) {
                damage.add(device);
            }
        }
        damage
    }

    /// Build from rectangles already in device pixels.
    pub fn from_device(rects: &[DamageRect]) -> Self {
        let mut damage = Self::default();
        for rect in rects {
            damage.add(*rect);
        }
        damage
    }

    /// Add a device rectangle, keeping the set disjoint and bounded.
    pub fn add(&mut self, rect: DamageRect) {
        if rect.width == 0 || rect.height == 0 {
            return;
        }
        let mut pieces: SmallVec<[DamageRect; 8]> = SmallVec::new();
        pieces.push(rect);
        for existing in &self.rects {
            if pieces.is_empty() {
                return;
            }
            let mut remaining: SmallVec<[DamageRect; 8]> = SmallVec::new();
            for piece in pieces.drain(..) {
                subtract_into(piece, *existing, &mut remaining);
            }
            pieces = remaining;
        }
        self.rects.extend(pieces);
        if self.rects.len() > MAX_DEVICE_DAMAGE_RECTS {
            let bounds = self
                .rects
                .iter()
                .copied()
                .reduce(DamageRect::union)
                .unwrap_or(rect);
            self.rects.clear();
            self.rects.push(bounds);
        }
    }

    pub fn rects(&self) -> &[DamageRect] {
        &self.rects
    }

    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    pub fn area(&self) -> u64 {
        self.rects.iter().map(|rect| rect.area()).sum()
    }
}

/// Push the parts of `rect` outside `cut` into `out`: up to four bands
/// (above, below, left and right of the overlap).
fn subtract_into<const N: usize>(
    rect: DamageRect,
    cut: DamageRect,
    out: &mut SmallVec<[DamageRect; N]>,
) where
    [DamageRect; N]: smallvec::Array<Item = DamageRect>,
{
    if !rect.intersects(cut) {
        out.push(rect);
        return;
    }
    let (left, top) = (rect.x, rect.y);
    let (right, bottom) = (rect.x + rect.width, rect.y + rect.height);
    let cut_left = cut.x.max(left);
    let cut_top = cut.y.max(top);
    let cut_right = (cut.x + cut.width).min(right);
    let cut_bottom = (cut.y + cut.height).min(bottom);
    let mut push = |x: u32, y: u32, x2: u32, y2: u32| {
        if x2 > x && y2 > y {
            out.push(DamageRect {
                x,
                y,
                width: x2 - x,
                height: y2 - y,
            });
        }
    };
    push(left, top, right, cut_top);
    push(left, cut_bottom, right, bottom);
    push(left, cut_top, cut_left, cut_bottom);
    push(cut_right, cut_top, right, cut_bottom);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: u32, y: u32, width: u32, height: u32) -> DamageRect {
        DamageRect {
            x,
            y,
            width,
            height,
        }
    }

    fn assert_disjoint(damage: &DeviceDamage) {
        let rects = damage.rects();
        for (index, left) in rects.iter().enumerate() {
            for right in &rects[index + 1..] {
                assert!(!left.intersects(*right), "{left:?} overlaps {right:?}");
            }
        }
    }

    #[test]
    fn overlapping_logical_rects_become_disjoint_device_rects_with_the_same_union() {
        // At 1.5x, logical edges 10 and 11 round out to device 15 and 17, so
        // two abutting logical rects overlap by a device column.
        let damage =
            DeviceDamage::from_logical(&[rect(0, 0, 11, 10), rect(10, 0, 10, 10)], 1.5, 100, 100);
        assert_disjoint(&damage);
        assert_eq!(damage.area(), 30 * 15);
    }

    #[test]
    fn contained_and_duplicate_rects_add_nothing() {
        let damage = DeviceDamage::from_device(&[
            rect(0, 0, 50, 50),
            rect(10, 10, 5, 5),
            rect(0, 0, 50, 50),
        ]);
        assert_eq!(damage.rects(), &[rect(0, 0, 50, 50)]);
    }

    #[test]
    fn a_cross_splits_into_disjoint_bands() {
        let damage = DeviceDamage::from_device(&[rect(10, 0, 10, 30), rect(0, 10, 30, 10)]);
        assert_disjoint(&damage);
        assert_eq!(damage.area(), 10 * 30 + 2 * 10 * 10);
    }

    #[test]
    fn damage_is_clipped_to_the_buffer() {
        let damage = DeviceDamage::from_logical(&[rect(90, 90, 50, 50)], 1.0, 100, 100);
        assert_eq!(damage.rects(), &[rect(90, 90, 10, 10)]);
    }

    #[test]
    fn too_many_rects_collapse_to_their_bounds() {
        let rects: Vec<_> = (0..20).map(|index| rect(index * 4, 0, 2, 2)).collect();
        let damage = DeviceDamage::from_device(&rects);
        assert!(damage.rects().len() <= MAX_DEVICE_DAMAGE_RECTS);
        assert_disjoint(&damage);
        for original in rects {
            let covered: u64 = damage
                .rects()
                .iter()
                .filter(|kept| kept.intersects(original))
                .map(|kept| {
                    let x = kept.x.max(original.x);
                    let y = kept.y.max(original.y);
                    let x2 = (kept.x + kept.width).min(original.x + original.width);
                    let y2 = (kept.y + kept.height).min(original.y + original.height);
                    u64::from(x2 - x) * u64::from(y2 - y)
                })
                .sum();
            assert_eq!(covered, original.area(), "{original:?} lost damage");
        }
    }
}
