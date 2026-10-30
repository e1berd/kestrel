use std::sync::atomic::{AtomicU32, Ordering};

use gpui::{Pixels, px};

pub const ZOOM_STEPS: [u32; 8] = [80, 90, 100, 110, 125, 150, 175, 200];
pub const TEXT_STEPS: [(u32, &str); 4] = [(90, "Small"), (100, "Default"), (115, "Large"), (130, "Larger")];
pub const CODE_FONT_STEPS: [u32; 7] = [11, 12, 13, 14, 15, 16, 18];
pub const LOW_VISION_BOOST: f32 = 1.25;
pub const BASE_REM: f32 = 15.;

static ZOOM_BITS: AtomicU32 = AtomicU32::new(0x3F80_0000);

pub fn zoom() -> f32 {
    f32::from_bits(ZOOM_BITS.load(Ordering::Relaxed))
}

pub fn set_zoom(zoom: f32) {
    ZOOM_BITS.store(zoom.clamp(0.5, 3.).to_bits(), Ordering::Relaxed);
}

pub fn dp(value: f32) -> Pixels {
    px(value * zoom())
}

pub fn step_from(percent: u32, is_larger: bool) -> u32 {
    let next =
        if is_larger { ZOOM_STEPS.iter().find(|step| **step > percent) } else { ZOOM_STEPS.iter().rev().find(|step| **step < percent) };
    next.copied().unwrap_or(percent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_stop_at_the_ends() {
        assert_eq!(step_from(100, true), 110);
        assert_eq!(step_from(100, false), 90);
        assert_eq!(step_from(200, true), 200);
        assert_eq!(step_from(80, false), 80);
    }

    #[test]
    fn an_odd_scale_steps_to_the_nearest_offered_one() {
        assert_eq!(step_from(117, true), 125);
        assert_eq!(step_from(117, false), 110);
    }
}
