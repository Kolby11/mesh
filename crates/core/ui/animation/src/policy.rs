use std::time::Duration;

/// Immutable motion preferences captured for one render/interaction decision.
///
/// The policy is deliberately small and renderer-facing. Callers classify
/// motion as essential or non-essential at the scheduling boundary, while the
/// policy keeps the reduced-motion decision consistent across animation and
/// scrolling implementations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MotionPolicy {
    pub reduced_motion: bool,
}

impl MotionPolicy {
    pub const fn new(reduced_motion: bool) -> Self {
        Self { reduced_motion }
    }

    pub const fn duration(self, duration: Duration, essential: bool) -> Duration {
        if self.reduced_motion && !essential {
            Duration::ZERO
        } else {
            duration
        }
    }
}

impl Default for MotionPolicy {
    fn default() -> Self {
        Self::new(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduced_motion_clamps_only_nonessential_durations() {
        let policy = MotionPolicy::new(true);
        assert_eq!(
            policy.duration(Duration::from_millis(180), false),
            Duration::ZERO
        );
        assert_eq!(
            policy.duration(Duration::from_millis(180), true),
            Duration::from_millis(180)
        );
    }

    #[test]
    fn default_policy_preserves_durations() {
        let policy = MotionPolicy::default();
        assert_eq!(
            policy.duration(Duration::from_millis(180), false),
            Duration::from_millis(180)
        );
    }
}
