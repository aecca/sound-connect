//! Talks the binary protocol Sony headphones use over Bluetooth, without
//! doing any I/O. See `sc-bluetooth` for the platform transports.

pub mod command;
pub mod frame;
pub mod profile;
pub mod session;

pub use command::{
    Battery, Codec, EqBands, EqLayout, EqPreset, Equalizer, NoiseControl, NoiseLayout, NoiseMode, Request, Update,
};
pub use profile::{BatteryLayout, Profile, profile_for};
pub use session::Session;

/// Service UUIDs Sony headsets advertise for their control channel, newest
/// protocol (v2) first.
pub const SERVICE_UUIDS: [&str; 2] = ["956C7B26-D49A-4BA8-B03F-B17D393CB6E2", "96CC203E-5068-46AD-B32D-E316F5E069BA"];

