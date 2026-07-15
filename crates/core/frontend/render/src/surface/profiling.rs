use std::cell::RefCell;
use std::time::Duration;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RasterMetrics {
    pub icon_image_raster_micros: u64,
    pub glyph_cache_hits: u64,
    pub glyph_cache_misses: u64,
    pub glyph_cache_entries: u64,
    pub glyph_cache_capacity: u64,
    pub font_bytes_cache_hits: u64,
    pub font_bytes_cache_misses: u64,
    pub font_bytes_cache_entries: u64,
    pub font_bytes_cache_capacity: u64,
    pub skia_glyph_cache_hits: u64,
    pub skia_glyph_cache_misses: u64,
    pub skia_glyph_cache_entries: u64,
    pub skia_glyph_cache_capacity: u64,
    pub raster_cache_hits: u64,
    pub raster_cache_misses: u64,
    pub raster_cache_bypasses: u64,
    pub raster_cache_opaque_hits: u64,
    pub raster_cache_translucent_hits: u64,
}

thread_local! {
    static RASTER_METRICS: RefCell<RasterMetrics> = RefCell::new(RasterMetrics::default());
}

pub fn reset_raster_metrics() {
    RASTER_METRICS.with(|metrics| {
        let mut metrics = metrics.borrow_mut();
        let cache_pressure = RasterMetrics {
            glyph_cache_entries: metrics.glyph_cache_entries,
            glyph_cache_capacity: metrics.glyph_cache_capacity,
            font_bytes_cache_entries: metrics.font_bytes_cache_entries,
            font_bytes_cache_capacity: metrics.font_bytes_cache_capacity,
            skia_glyph_cache_entries: metrics.skia_glyph_cache_entries,
            skia_glyph_cache_capacity: metrics.skia_glyph_cache_capacity,
            ..RasterMetrics::default()
        };
        *metrics = cache_pressure;
    });
}

pub fn record_icon_image_raster(duration: Duration) {
    let micros = duration.as_micros().min(u128::from(u64::MAX)) as u64;
    RASTER_METRICS.with(|metrics| {
        let mut metrics = metrics.borrow_mut();
        metrics.icon_image_raster_micros = metrics.icon_image_raster_micros.saturating_add(micros);
    });
}

pub fn record_glyph_cache_lookup(hit: bool, entries: usize, capacity: usize) {
    RASTER_METRICS.with(|metrics| {
        let mut metrics = metrics.borrow_mut();
        if hit {
            metrics.glyph_cache_hits = metrics.glyph_cache_hits.saturating_add(1);
        } else {
            metrics.glyph_cache_misses = metrics.glyph_cache_misses.saturating_add(1);
        }
        metrics.glyph_cache_entries = entries as u64;
        metrics.glyph_cache_capacity = capacity as u64;
    });
}

pub fn update_glyph_cache_entries(entries: usize, capacity: usize) {
    RASTER_METRICS.with(|metrics| {
        let mut metrics = metrics.borrow_mut();
        metrics.glyph_cache_entries = entries as u64;
        metrics.glyph_cache_capacity = capacity as u64;
    });
}

pub fn record_font_bytes_cache_lookup(hit: bool, entries: usize, capacity: usize) {
    RASTER_METRICS.with(|metrics| {
        let mut metrics = metrics.borrow_mut();
        if hit {
            metrics.font_bytes_cache_hits = metrics.font_bytes_cache_hits.saturating_add(1);
        } else {
            metrics.font_bytes_cache_misses = metrics.font_bytes_cache_misses.saturating_add(1);
        }
        metrics.font_bytes_cache_entries = entries as u64;
        metrics.font_bytes_cache_capacity = capacity as u64;
    });
}

pub fn update_font_bytes_cache_entries(entries: usize, capacity: usize) {
    RASTER_METRICS.with(|metrics| {
        let mut metrics = metrics.borrow_mut();
        metrics.font_bytes_cache_entries = entries as u64;
        metrics.font_bytes_cache_capacity = capacity as u64;
    });
}

pub fn record_skia_glyph_cache_lookup(hit: bool, entries: usize, capacity: usize) {
    RASTER_METRICS.with(|metrics| {
        let mut metrics = metrics.borrow_mut();
        if hit {
            metrics.skia_glyph_cache_hits = metrics.skia_glyph_cache_hits.saturating_add(1);
        } else {
            metrics.skia_glyph_cache_misses = metrics.skia_glyph_cache_misses.saturating_add(1);
        }
        metrics.skia_glyph_cache_entries = entries as u64;
        metrics.skia_glyph_cache_capacity = capacity as u64;
    });
}

pub fn update_skia_glyph_cache_entries(entries: usize, capacity: usize) {
    RASTER_METRICS.with(|metrics| {
        let mut metrics = metrics.borrow_mut();
        metrics.skia_glyph_cache_entries = entries as u64;
        metrics.skia_glyph_cache_capacity = capacity as u64;
    });
}

pub fn record_raster_cache_hit(opaque: bool) {
    RASTER_METRICS.with(|metrics| {
        let mut metrics = metrics.borrow_mut();
        metrics.raster_cache_hits = metrics.raster_cache_hits.saturating_add(1);
        if opaque {
            metrics.raster_cache_opaque_hits = metrics.raster_cache_opaque_hits.saturating_add(1);
        } else {
            metrics.raster_cache_translucent_hits =
                metrics.raster_cache_translucent_hits.saturating_add(1);
        }
    });
}

pub fn record_raster_cache_miss() {
    RASTER_METRICS.with(|metrics| {
        let mut metrics = metrics.borrow_mut();
        metrics.raster_cache_misses = metrics.raster_cache_misses.saturating_add(1);
    });
}

pub fn record_raster_cache_bypass() {
    RASTER_METRICS.with(|metrics| {
        let mut metrics = metrics.borrow_mut();
        metrics.raster_cache_bypasses = metrics.raster_cache_bypasses.saturating_add(1);
    });
}

pub fn raster_metrics() -> RasterMetrics {
    RASTER_METRICS.with(|metrics| *metrics.borrow())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_clears_activity_but_preserves_last_known_cache_pressure() {
        reset_raster_metrics();
        record_glyph_cache_lookup(true, 700, 1_024);
        record_font_bytes_cache_lookup(false, 18, 32);
        record_skia_glyph_cache_lookup(true, 250, 512);

        reset_raster_metrics();
        let metrics = raster_metrics();

        assert_eq!(metrics.glyph_cache_hits, 0);
        assert_eq!(metrics.font_bytes_cache_misses, 0);
        assert_eq!(metrics.skia_glyph_cache_hits, 0);
        assert_eq!(metrics.glyph_cache_entries, 700);
        assert_eq!(metrics.glyph_cache_capacity, 1_024);
        assert_eq!(metrics.font_bytes_cache_entries, 18);
        assert_eq!(metrics.font_bytes_cache_capacity, 32);
        assert_eq!(metrics.skia_glyph_cache_entries, 250);
        assert_eq!(metrics.skia_glyph_cache_capacity, 512);
    }
}
