//! Recognizing headsets and picking the driver that speaks to them.

use crate::driver::Driver;
use crate::drivers::bose::{self, BoseDriver};
use crate::drivers::sony::{self, SonyDriver};

/// The control services of supported headsets, in order of preference.
/// Transports look for these among a headset's services to know whether it's
/// supported and which channel to open.
pub const SERVICES: [&str; 3] = [sony::SERVICE, sony::SERVICE_V1, bose::SERVICE];

/// The driver for a headset with this Bluetooth name, given the control
/// service (one of [`SERVICES`]) it offers.
pub fn driver_for(name: &str, service: &str) -> Option<Box<dyn Driver>> {
    let is = |uuid: &str| service.eq_ignore_ascii_case(uuid);
    if is(sony::SERVICE) || is(sony::SERVICE_V1) {
        Some(Box::new(SonyDriver::new(name)))
    } else if is(bose::SERVICE) {
        Some(Box::new(BoseDriver::new(name)))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::Brand;

    #[test]
    fn picks_the_driver_by_service() {
        let sony = driver_for("WH-1000XM5", &sony::SERVICE.to_lowercase()).expect("Sony is supported");
        assert_eq!(sony.device().brand, Brand::Sony);
        let bose = driver_for("Bose QC35 II", bose::SERVICE).expect("Bose is supported");
        assert_eq!(bose.device().brand, Brand::Bose);
        assert!(driver_for("Speaker", "0000110B-0000-1000-8000-00805F9B34FB").is_none());
    }
}
