//! When to say the battery is running low: once at 20%, once more at 10%,
//! and not again until it's been charged. Apps pass each reading to
//! [`LowBattery`] and show a notification when it says so.

use crate::device::Battery;

#[derive(Debug, Default)]
pub struct LowBattery {
    /// The lowest of [`LEVELS`](Self::LEVELS) already warned about.
    warned: Option<u8>,
}

impl LowBattery {
    /// Where to warn, highest first.
    pub const LEVELS: [u8; 2] = [20, 10];
    /// Above this the battery counts as charged, even if it charged while the
    /// app wasn't looking. It's well above the first warning, so a reading
    /// going back and forth by a point doesn't warn again.
    const CHARGED: u8 = 30;

    /// Call with each battery reading. Returns true when it's time to warn.
    pub fn update(&mut self, battery: Battery) -> bool {
        if battery.charging || battery.level > Self::CHARGED {
            self.warned = None;
            return false;
        }
        let Some(&level) = Self::LEVELS.iter().rev().find(|&&level| battery.level <= level) else { return false };
        if self.warned.is_some_and(|warned| warned <= level) {
            return false;
        }
        self.warned = Some(level);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(level: u8) -> Battery {
        Battery { level, charging: false }
    }

    #[test]
    fn warns_at_twenty_and_ten_percent_only_once_each() {
        let mut low = LowBattery::default();
        let warnings: Vec<_> = [50, 30, 21, 20, 19, 21, 20, 12, 10, 9, 5].map(|level| low.update(at(level))).to_vec();
        assert_eq!(warnings, [false, false, false, true, false, false, false, false, true, false, false]);
    }

    #[test]
    fn warns_once_when_it_starts_out_low() {
        let mut low = LowBattery::default();
        assert!(low.update(at(8)));
        assert!(!low.update(at(15)));
        assert!(!low.update(at(7)));
    }

    #[test]
    fn warns_again_after_charging() {
        let mut low = LowBattery::default();
        assert!(low.update(at(20)));
        assert!(!low.update(Battery { level: 20, charging: true }));
        assert!(low.update(at(19)));
        // Charged while disconnected.
        assert!(!low.update(at(90)));
        assert!(low.update(at(20)));
    }
}
