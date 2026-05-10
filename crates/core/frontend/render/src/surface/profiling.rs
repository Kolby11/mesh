use std::cell::RefCell;
use std::time::Duration;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RasterMetrics {
    pub icon_image_raster_micros: u64,
}

thread_local! {
    static RASTER_METRICS: RefCell<RasterMetrics> = RefCell::new(RasterMetrics::default());
}

pub fn reset_raster_metrics() {
    RASTER_METRICS.with(|metrics| {
        *metrics.borrow_mut() = RasterMetrics::default();
    });
}

pub fn record_icon_image_raster(duration: Duration) {
    let micros = duration.as_micros().min(u128::from(u64::MAX)) as u64;
    RASTER_METRICS.with(|metrics| {
        let mut metrics = metrics.borrow_mut();
        metrics.icon_image_raster_micros = metrics.icon_image_raster_micros.saturating_add(micros);
    });
}

pub fn raster_metrics() -> RasterMetrics {
    RASTER_METRICS.with(|metrics| *metrics.borrow())
}
