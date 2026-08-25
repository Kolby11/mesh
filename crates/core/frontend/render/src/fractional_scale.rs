use crate::display_list::DamageRect;
use mesh_core_elements::LayoutRect;

/// The device-space rounding contract shared by layout consumers, painting,
/// buffer allocation, SHM copies, and Wayland damage.
///
/// Logical geometry is represented by edges, not by independently rounded
/// positions and sizes. The near edge is floored and the far edge is ceiled,
/// so adjacent fractional rectangles cannot leave an uncovered device pixel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FractionalScale {
    factor: f32,
}

/// A conservative integer device-space coverage rectangle. Negative origins
/// are retained until the rectangle is clipped to a buffer or surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl DeviceRect {
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(self) -> i32 {
        self.x.saturating_add(self.width.max(0))
    }

    pub fn bottom(self) -> i32 {
        self.y.saturating_add(self.height.max(0))
    }

    pub fn is_empty(self) -> bool {
        self.width <= 0 || self.height <= 0
    }

    pub fn intersect(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        (right > x && bottom > y).then_some(Self::new(x, y, right - x, bottom - y))
    }

    pub fn to_damage_rect(self) -> Option<DamageRect> {
        if self.is_empty() || self.x < 0 || self.y < 0 {
            return None;
        }
        Some(DamageRect {
            x: self.x as u32,
            y: self.y as u32,
            width: self.width as u32,
            height: self.height as u32,
        })
    }

    pub fn to_nonnegative_damage_rect(self) -> Option<DamageRect> {
        let clipped = self.intersect(Self::new(0, 0, i32::MAX, i32::MAX))?;
        clipped.to_damage_rect()
    }

    pub fn clip_to_buffer(self, width: u32, height: u32) -> Option<DamageRect> {
        let width = i32::try_from(width).unwrap_or(i32::MAX);
        let height = i32::try_from(height).unwrap_or(i32::MAX);
        self.intersect(Self::new(0, 0, width, height))?
            .to_damage_rect()
    }

    pub fn from_damage_rect(rect: DamageRect) -> Self {
        Self::new(
            rect.x.min(i32::MAX as u32) as i32,
            rect.y.min(i32::MAX as u32) as i32,
            rect.width.min(i32::MAX as u32) as i32,
            rect.height.min(i32::MAX as u32) as i32,
        )
    }
}

impl FractionalScale {
    pub fn new(factor: f32) -> Self {
        Self {
            factor: if factor.is_finite() && factor > 0.0 {
                factor
            } else {
                1.0
            },
        }
    }

    pub const fn identity() -> Self {
        Self { factor: 1.0 }
    }

    pub const fn factor(self) -> f32 {
        self.factor
    }

    /// Convert a logical extent to the physical buffer extent. The far edge
    /// uses the same ceil rule as every other device-space rectangle.
    pub fn physical_extent(self, logical: u32) -> u32 {
        self.physical_extent_f32(logical as f32)
    }

    pub fn physical_extent_f32(self, logical: f32) -> u32 {
        let edge = self.cover_edges(0.0, 0.0, logical.max(0.0), logical.max(0.0));
        edge.right().max(1) as u32
    }

    /// Recover a conservative logical extent from a physical buffer extent.
    pub fn logical_extent(self, physical: u32) -> u32 {
        ((physical as f32) / self.factor).ceil().max(1.0) as u32
    }

    pub fn device_rect(self, rect: DamageRect) -> DeviceRect {
        self.cover_edges(
            rect.x as f32,
            rect.y as f32,
            rect.x.saturating_add(rect.width) as f32,
            rect.y.saturating_add(rect.height) as f32,
        )
    }

    pub fn device_layout_rect(self, rect: LayoutRect) -> DeviceRect {
        self.cover_edges(rect.x, rect.y, rect.x + rect.width, rect.y + rect.height)
    }

    pub fn clip_damage_rect(
        self,
        rect: DamageRect,
        buffer_width: u32,
        buffer_height: u32,
    ) -> Option<DamageRect> {
        self.device_rect(rect)
            .clip_to_buffer(buffer_width, buffer_height)
    }

    pub fn protocol_buffer_scale(self, viewporter_available: bool) -> i32 {
        let is_integer = (self.factor - self.factor.round()).abs() < f32::EPSILON;
        let rounded = if is_integer {
            self.factor.round()
        } else if viewporter_available {
            self.factor.ceil()
        } else {
            self.factor.round()
        };
        rounded.max(1.0) as i32
    }

    fn cover_edges(self, left: f32, top: f32, right: f32, bottom: f32) -> DeviceRect {
        let left = self.floor_scaled(left);
        let top = self.floor_scaled(top);
        let right = self.ceil_scaled(right);
        let bottom = self.ceil_scaled(bottom);
        DeviceRect::new(
            left,
            top,
            right.saturating_sub(left).max(0),
            bottom.saturating_sub(top).max(0),
        )
    }

    fn floor_scaled(self, value: f32) -> i32 {
        (value * self.factor).floor() as i32
    }

    fn ceil_scaled(self, value: f32) -> i32 {
        (value * self.factor).ceil() as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_coverage_rounds_near_and_far_edges_once() {
        let scale = FractionalScale::new(1.5);
        assert_eq!(
            scale.device_rect(DamageRect {
                x: 1,
                y: 3,
                width: 2,
                height: 2,
            }),
            DeviceRect::new(1, 4, 4, 4)
        );
    }

    #[test]
    fn adjacent_fractional_layout_edges_use_the_same_coverage_rule() {
        let scale = FractionalScale::new(1.25);
        let first = scale.device_layout_rect(LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        });
        let second = scale.device_layout_rect(LayoutRect {
            x: 1.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        });
        assert_eq!(first.right(), 2);
        assert_eq!(second.x, 1);
        assert!(first.intersect(second).is_some());
    }

    #[test]
    fn physical_extent_and_clipped_damage_share_far_edge_ceil() {
        let scale = FractionalScale::new(1.25);
        assert_eq!(scale.physical_extent(401), 502);
        assert_eq!(
            scale.clip_damage_rect(
                DamageRect {
                    x: 400,
                    y: 0,
                    width: 2,
                    height: 1,
                },
                502,
                2,
            ),
            Some(DamageRect {
                x: 500,
                y: 0,
                width: 2,
                height: 2,
            })
        );
    }

    #[test]
    fn protocol_buffer_scale_uses_the_same_scale_normalization() {
        let scale = FractionalScale::new(1.5);
        assert_eq!(scale.protocol_buffer_scale(true), 2);
        assert_eq!(scale.protocol_buffer_scale(false), 2);
        assert_eq!(FractionalScale::new(2.0).protocol_buffer_scale(true), 2);
    }
}
