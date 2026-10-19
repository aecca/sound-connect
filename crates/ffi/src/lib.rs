//! Exposes the protocol core to Swift through UniFFI, so the native macOS
//! app and the Rust apps share one implementation of the headset protocol.
//!
//! The Swift side owns the Bluetooth channel: it passes received bytes to
//! [`Session::receive`] and writes every frame the session returns.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use sc_protocol as core;

uniffi::setup_scaffolding!();

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum NoiseMode {
    NoiseCancelling,
    Ambient,
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct NoiseControl {
    pub mode: NoiseMode,
    pub focus_on_voice: bool,
    /// 1...20
    pub ambient_level: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Battery {
    pub level: u8,
    pub charging: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum EqPreset {
    Off,
    Bright,
    Excited,
    Mellow,
    Relaxed,
    Vocal,
    TrebleBoost,
    BassBoost,
    Speech,
    Manual,
    Custom1,
    Custom2,
    Unknown { code: u8 },
}

/// Five bands plus Clear Bass, each from -10 to +10.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Equalizer {
    pub preset: EqPreset,
    pub clear_bass: i8,
    /// 400 Hz, 1 kHz, 2.5 kHz, 6.3 kHz, 16 kHz.
    pub bands: Vec<i8>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Request {
    Init,
    FirmwareVersion,
    Battery,
    Codec,
    NoiseControl,
    SetNoiseControl { value: NoiseControl },
    SpeakToChat,
    SetSpeakToChat { on: bool },
    Equalizer,
    SetEqualizerPreset { preset: EqPreset },
    /// Switches to the Manual preset with these values (five bands).
    SetEqualizerBands { clear_bass: i8, bands: Vec<i8> },
    Dsee,
    SetDsee { on: bool },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Update {
    /// The headset answered the handshake. `v2` is false for older headsets,
    /// which aren't supported yet.
    Ready { v2: bool },
    Firmware { version: String },
    Battery { battery: Battery },
    Codec { name: String },
    NoiseControl { value: NoiseControl },
    SpeakToChat { on: bool },
    Equalizer { value: Equalizer },
    Dsee { on: bool },
}

#[derive(Debug, uniffi::Record)]
pub struct Output {
    /// Frames to write to the headset, in order.
    pub writes: Vec<Vec<u8>>,
    pub updates: Vec<Update>,
}

/// Conversation state with one headset. See `sc_protocol::Session`.
#[derive(uniffi::Object)]
pub struct Session(Mutex<core::Session>);

#[uniffi::export]
impl Session {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self(Mutex::new(core::Session::default())))
    }

    /// Queues a request; returns the frames to write now.
    pub fn request(&self, request: Request) -> Vec<Vec<u8>> {
        self.0.lock().unwrap().request(request.into(), Instant::now())
    }

    pub fn receive(&self, bytes: Vec<u8>) -> Output {
        let out = self.0.lock().unwrap().receive(&bytes, Instant::now());
        Output { writes: out.writes, updates: out.updates.into_iter().map(Into::into).collect() }
    }

    /// Call a few times per second; returns frames to resend or send next.
    pub fn tick(&self) -> Vec<Vec<u8>> {
        self.0.lock().unwrap().tick(Instant::now())
    }
}

/// What to ask the headset right after connecting.
#[uniffi::export]
pub fn initial_requests() -> Vec<Request> {
    core::INITIAL_REQUESTS.into_iter().map(Into::into).collect()
}

/// Every preset, in the order Sony's app lists them.
#[uniffi::export]
pub fn eq_presets() -> Vec<EqPreset> {
    core::EqPreset::ALL.into_iter().map(Into::into).collect()
}

/// The preset's display name, e.g. "Bass Boost".
#[uniffi::export]
pub fn eq_preset_name(preset: EqPreset) -> String {
    core::EqPreset::from(preset).name()
}

/// The control-channel service UUIDs Sony headsets advertise, newest first.
#[uniffi::export]
pub fn service_uuids() -> Vec<String> {
    core::SERVICE_UUIDS.iter().map(|uuid| uuid.to_string()).collect()
}

impl From<core::NoiseMode> for NoiseMode {
    fn from(mode: core::NoiseMode) -> Self {
        match mode {
            core::NoiseMode::NoiseCancelling => Self::NoiseCancelling,
            core::NoiseMode::Ambient => Self::Ambient,
            core::NoiseMode::Off => Self::Off,
        }
    }
}

impl From<NoiseMode> for core::NoiseMode {
    fn from(mode: NoiseMode) -> Self {
        match mode {
            NoiseMode::NoiseCancelling => Self::NoiseCancelling,
            NoiseMode::Ambient => Self::Ambient,
            NoiseMode::Off => Self::Off,
        }
    }
}

impl From<core::NoiseControl> for NoiseControl {
    fn from(nc: core::NoiseControl) -> Self {
        Self { mode: nc.mode.into(), focus_on_voice: nc.focus_on_voice, ambient_level: nc.ambient_level }
    }
}

impl From<NoiseControl> for core::NoiseControl {
    fn from(nc: NoiseControl) -> Self {
        Self { mode: nc.mode.into(), focus_on_voice: nc.focus_on_voice, ambient_level: nc.ambient_level }
    }
}

impl From<core::EqPreset> for EqPreset {
    fn from(preset: core::EqPreset) -> Self {
        use core::EqPreset as P;
        match preset {
            P::Off => Self::Off,
            P::Bright => Self::Bright,
            P::Excited => Self::Excited,
            P::Mellow => Self::Mellow,
            P::Relaxed => Self::Relaxed,
            P::Vocal => Self::Vocal,
            P::TrebleBoost => Self::TrebleBoost,
            P::BassBoost => Self::BassBoost,
            P::Speech => Self::Speech,
            P::Manual => Self::Manual,
            P::Custom1 => Self::Custom1,
            P::Custom2 => Self::Custom2,
            P::Unknown(code) => Self::Unknown { code },
        }
    }
}

impl From<EqPreset> for core::EqPreset {
    fn from(preset: EqPreset) -> Self {
        match preset {
            EqPreset::Off => Self::Off,
            EqPreset::Bright => Self::Bright,
            EqPreset::Excited => Self::Excited,
            EqPreset::Mellow => Self::Mellow,
            EqPreset::Relaxed => Self::Relaxed,
            EqPreset::Vocal => Self::Vocal,
            EqPreset::TrebleBoost => Self::TrebleBoost,
            EqPreset::BassBoost => Self::BassBoost,
            EqPreset::Speech => Self::Speech,
            EqPreset::Manual => Self::Manual,
            EqPreset::Custom1 => Self::Custom1,
            EqPreset::Custom2 => Self::Custom2,
            EqPreset::Unknown { code } => Self::Unknown(code),
        }
    }
}

impl From<Request> for core::Request {
    fn from(request: Request) -> Self {
        match request {
            Request::Init => Self::Init,
            Request::FirmwareVersion => Self::FirmwareVersion,
            Request::Battery => Self::Battery,
            Request::Codec => Self::Codec,
            Request::NoiseControl => Self::NoiseControl,
            Request::SetNoiseControl { value } => Self::SetNoiseControl(value.into()),
            Request::SpeakToChat => Self::SpeakToChat,
            Request::SetSpeakToChat { on } => Self::SetSpeakToChat(on),
            Request::Equalizer => Self::Equalizer,
            Request::SetEqualizerPreset { preset } => Self::SetEqualizerPreset(preset.into()),
            Request::SetEqualizerBands { clear_bass, bands } => {
                let mut five = [0; 5];
                for (slot, value) in five.iter_mut().zip(bands) {
                    *slot = value;
                }
                Self::SetEqualizerBands { clear_bass, bands: five }
            }
            Request::Dsee => Self::Dsee,
            Request::SetDsee { on } => Self::SetDsee(on),
        }
    }
}

impl From<core::Request> for Request {
    fn from(request: core::Request) -> Self {
        match request {
            core::Request::Init => Self::Init,
            core::Request::FirmwareVersion => Self::FirmwareVersion,
            core::Request::Battery => Self::Battery,
            core::Request::Codec => Self::Codec,
            core::Request::NoiseControl => Self::NoiseControl,
            core::Request::SetNoiseControl(value) => Self::SetNoiseControl { value: value.into() },
            core::Request::SpeakToChat => Self::SpeakToChat,
            core::Request::SetSpeakToChat(on) => Self::SetSpeakToChat { on },
            core::Request::Equalizer => Self::Equalizer,
            core::Request::SetEqualizerPreset(preset) => Self::SetEqualizerPreset { preset: preset.into() },
            core::Request::SetEqualizerBands { clear_bass, bands } => {
                Self::SetEqualizerBands { clear_bass, bands: bands.to_vec() }
            }
            core::Request::Dsee => Self::Dsee,
            core::Request::SetDsee(on) => Self::SetDsee { on },
        }
    }
}

impl From<core::Update> for Update {
    fn from(update: core::Update) -> Self {
        match update {
            core::Update::Ready { v2 } => Self::Ready { v2 },
            core::Update::Firmware(version) => Self::Firmware { version },
            core::Update::Battery(b) => Self::Battery { battery: Battery { level: b.level, charging: b.charging } },
            core::Update::Codec(codec) => Self::Codec { name: codec.name() },
            core::Update::NoiseControl(nc) => Self::NoiseControl { value: nc.into() },
            core::Update::SpeakToChat(on) => Self::SpeakToChat { on },
            core::Update::Equalizer(eq) => Self::Equalizer {
                value: Equalizer { preset: eq.preset.into(), clear_bass: eq.clear_bass, bands: eq.bands.to_vec() },
            },
            core::Update::Dsee(on) => Self::Dsee { on },
        }
    }
}
