use super::{ComputedStyle, Corners, Dimension, Edges};

impl ComputedStyle {
    /// Scale typography separately from the shell's absolute lengths.
    pub fn rescale_text(&mut self, target: f32) {
        let factor = target / self.text_scale;
        self.font_size *= factor;
        self.letter_spacing *= factor;
        self.text_scale = target;
    }

    /// Scale absolute lengths into shell logical coordinates, preserving relative values.
    pub fn rescale_lengths(&mut self, target: f32) {
        if self.ui_scale == target {
            return;
        }
        let factor = target / self.ui_scale;
        fn dim(value: &mut Dimension, factor: f32) {
            if let Dimension::Px(value) = value {
                *value *= factor;
            }
        }
        fn edges(value: &mut Edges, factor: f32) {
            value.top *= factor;
            value.right *= factor;
            value.bottom *= factor;
            value.left *= factor;
        }
        fn corners(value: &mut Corners, factor: f32) {
            value.top_left *= factor;
            value.top_right *= factor;
            value.bottom_left *= factor;
            value.bottom_right *= factor;
        }
        for value in [
            &mut self.width,
            &mut self.height,
            &mut self.min_width,
            &mut self.max_width,
            &mut self.min_height,
            &mut self.max_height,
            &mut self.flex_basis,
        ] {
            dim(value, factor);
        }
        for value in [&mut self.padding, &mut self.margin, &mut self.border_width] {
            edges(value, factor);
        }
        corners(&mut self.border_radius, factor);
        self.font_size *= factor;
        self.letter_spacing *= factor;
        self.gap *= factor;
        for value in [
            &mut self.row_gap,
            &mut self.column_gap,
            &mut self.inset_top,
            &mut self.inset_right,
            &mut self.inset_bottom,
            &mut self.inset_left,
        ] {
            if let Some(value) = value {
                *value *= factor;
            }
        }
        self.transform.translate_x *= factor;
        self.transform.translate_y *= factor;
        for shadow in
            std::iter::once(&mut self.box_shadow).chain(self.additional_box_shadows.iter_mut())
        {
            shadow.offset_x *= factor;
            shadow.offset_y *= factor;
            shadow.blur_radius *= factor;
            shadow.spread_radius *= factor;
        }
        self.outline.width *= factor;
        self.outline.offset *= factor;
        for filter in [&mut self.filter, &mut self.backdrop_filter] {
            filter.blur_radius *= factor;
            filter.refract_depth *= factor;
            filter.refract_edge *= factor;
            filter.dispersion *= factor;
        }
        if let Some((x, y)) = &mut self.tooltip_offset {
            *x *= factor;
            *y *= factor;
        }
        self.ui_scale = target;
    }
}
