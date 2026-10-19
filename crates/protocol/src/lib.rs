//! Talks the binary protocol Sony headphones use over Bluetooth, without
//! doing any I/O. See `sc-bluetooth` for the platform transports.

pub mod command;
pub mod frame;
pub mod session;

pub use command::{Battery, Codec, NoiseControl, NoiseMode, Request, Update};
pub use session::Session;
