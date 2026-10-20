//! The headphone protocols Sound Connect speaks, one driver per brand,
//! without doing any I/O.
//!
//! A transport finds a paired headset offering one of the [`SERVICES`], opens
//! the channel of its [`channel_service`], gets the headset's [`Driver`] from
//! [`driver_for`] and moves bytes between the two. Apps work with the
//! brand-neutral [`Device`], [`Update`] and [`Setting`].

pub mod calls;
pub mod device;
pub mod driver;
pub mod drivers;
pub mod equalizer;
pub mod low_battery;
mod queue;
pub mod registry;

pub use calls::CallAmbience;
pub use device::{
    Ambient, AutoOff, Battery, BatteryLayout, Brand, ConnectedDevice, Device, Feature, ListeningMode, Mode,
    NoiseButton, Setting, SpeakToChatEnd, Update, VoiceSensitivity,
};
pub use driver::{Driver, Frame, Output};
pub use equalizer::{EqBands, EqLayout, EqPreset, Equalizer};
pub use low_battery::LowBattery;
pub use registry::{SERVICES, channel_service, driver_for};
