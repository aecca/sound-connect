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
    WindReduction,
}

/// How the headset lays out its noise-control message; keep the one it
/// reported when sending changes back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum NoiseLayout {
    Standard,
    WithWind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct NoiseControl {
    pub mode: NoiseMode,
    pub focus_on_voice: bool,
    /// 1...20
    pub ambient_level: u8,
    pub layout: NoiseLayout,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum EqLayout {
    /// Five bands plus Clear Bass.
    FiveBands,
    /// Ten bands, no Clear Bass.
    TenBands,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum EqBands {
    /// Steps from -10 to +10: 400 Hz, 1 kHz, 2.5 kHz, 6.3 kHz, 16 kHz.
    Five { clear_bass: i8, bands: Vec<i8> },
    /// Ten raw device values from 0 to 12; their dB scale isn't known yet.
    Ten { bands: Vec<u8> },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Equalizer {
    pub preset: EqPreset,
    pub bands: EqBands,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum BatteryLayout {
    Single,
    Earbuds,
}

/// What a headset model supports. See `sc_protocol::profile`.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Profile {
    /// The model's name, or "" when it isn't recognized.
    pub model: String,
    /// Confirmed working on real hardware.
    pub tested: bool,
    pub battery: BatteryLayout,
    pub wind_reduction: bool,
    pub speak_to_chat: bool,
    pub equalizer: Option<EqLayout>,
    pub dsee: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Request {
    Init,
    FirmwareVersion,
    Battery,
    EarbudBattery,
    CaseBattery,
    Codec,
    NoiseControl,
    SetNoiseControl { value: NoiseControl },
    SpeakToChat,
    SetSpeakToChat { on: bool },
    Equalizer { layout: EqLayout },
    SetEqualizerPreset { layout: EqLayout, preset: EqPreset },
    /// Switches to the Manual preset with these values.
    SetEqualizerBands { bands: EqBands },
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
    /// `None` for an earbud that isn't reporting, e.g. while in its case.
    EarbudBattery { left: Option<Battery>, right: Option<Battery> },
    CaseBattery { battery: Battery },
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

/// The profile for a headset, from its Bluetooth name.
#[uniffi::export]
pub fn profile_for(name: String) -> Profile {
    core::profile_for(&name).into()
}

/// What to ask the headset right after connecting.
#[uniffi::export]
pub fn initial_requests(profile: Profile) -> Vec<Request> {
    core::Profile::from(profile).initial_requests().into_iter().map(Into::into).collect()
}

/// What to ask to refresh the battery levels.
#[uniffi::export]
pub fn battery_requests(profile: Profile) -> Vec<Request> {
    core::Profile::from(profile).battery_requests().into_iter().map(Into::into).collect()
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

// Conversions between the FFI types and the core ones. They mirror each other
// field by field.

impl From<core::NoiseMode> for NoiseMode {
    fn from(mode: core::NoiseMode) -> Self {
        match mode {
            core::NoiseMode::NoiseCancelling => Self::NoiseCancelling,
            core::NoiseMode::Ambient => Self::Ambient,
            core::NoiseMode::Off => Self::Off,
            core::NoiseMode::WindReduction => Self::WindReduction,
        }
    }
}

impl From<NoiseMode> for core::NoiseMode {
    fn from(mode: NoiseMode) -> Self {
        match mode {
            NoiseMode::NoiseCancelling => Self::NoiseCancelling,
            NoiseMode::Ambient => Self::Ambient,
            NoiseMode::Off => Self::Off,
            NoiseMode::WindReduction => Self::WindReduction,
        }
    }
}

impl From<core::NoiseLayout> for NoiseLayout {
    fn from(layout: core::NoiseLayout) -> Self {
        match layout {
            core::NoiseLayout::Standard => Self::Standard,
            core::NoiseLayout::WithWind => Self::WithWind,
        }
    }
}

impl From<NoiseLayout> for core::NoiseLayout {
    fn from(layout: NoiseLayout) -> Self {
        match layout {
            NoiseLayout::Standard => Self::Standard,
            NoiseLayout::WithWind => Self::WithWind,
        }
    }
}

impl From<core::NoiseControl> for NoiseControl {
    fn from(nc: core::NoiseControl) -> Self {
        Self {
            mode: nc.mode.into(),
            focus_on_voice: nc.focus_on_voice,
            ambient_level: nc.ambient_level,
            layout: nc.layout.into(),
        }
    }
}

impl From<NoiseControl> for core::NoiseControl {
    fn from(nc: NoiseControl) -> Self {
        Self {
            mode: nc.mode.into(),
            focus_on_voice: nc.focus_on_voice,
            ambient_level: nc.ambient_level,
            layout: nc.layout.into(),
        }
    }
}

impl From<core::Battery> for Battery {
    fn from(b: core::Battery) -> Self {
        Self { level: b.level, charging: b.charging }
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

impl From<core::EqLayout> for EqLayout {
    fn from(layout: core::EqLayout) -> Self {
        match layout {
            core::EqLayout::FiveBands => Self::FiveBands,
            core::EqLayout::TenBands => Self::TenBands,
        }
    }
}

impl From<EqLayout> for core::EqLayout {
    fn from(layout: EqLayout) -> Self {
        match layout {
            EqLayout::FiveBands => Self::FiveBands,
            EqLayout::TenBands => Self::TenBands,
        }
    }
}

impl From<core::EqBands> for EqBands {
    fn from(bands: core::EqBands) -> Self {
        match bands {
            core::EqBands::Five { clear_bass, bands } => Self::Five { clear_bass, bands: bands.to_vec() },
            core::EqBands::Ten(bands) => Self::Ten { bands: bands.to_vec() },
        }
    }
}

impl From<EqBands> for core::EqBands {
    fn from(bands: EqBands) -> Self {
        match bands {
            EqBands::Five { clear_bass, bands } => Self::Five { clear_bass, bands: fixed(bands) },
            EqBands::Ten { bands } => Self::Ten(fixed(bands)),
        }
    }
}

/// Copies a list into a fixed-size array, padding with zeros or dropping extras.
fn fixed<T: Copy + Default, const N: usize>(values: Vec<T>) -> [T; N] {
    let mut out = [T::default(); N];
    for (slot, value) in out.iter_mut().zip(values) {
        *slot = value;
    }
    out
}

impl From<core::BatteryLayout> for BatteryLayout {
    fn from(layout: core::BatteryLayout) -> Self {
        match layout {
            core::BatteryLayout::Single => Self::Single,
            core::BatteryLayout::Earbuds => Self::Earbuds,
        }
    }
}

impl From<core::Profile> for Profile {
    fn from(p: core::Profile) -> Self {
        Self {
            model: p.model.to_string(),
            tested: p.tested,
            battery: p.battery.into(),
            wind_reduction: p.wind_reduction,
            speak_to_chat: p.speak_to_chat,
            equalizer: p.equalizer.map(Into::into),
            dsee: p.dsee,
        }
    }
}

impl From<Profile> for core::Profile {
    /// Only the capabilities matter here; the model name is looked up again.
    fn from(p: Profile) -> Self {
        Self {
            model: core::profile_for(&p.model).model,
            tested: p.tested,
            battery: match p.battery {
                BatteryLayout::Single => core::BatteryLayout::Single,
                BatteryLayout::Earbuds => core::BatteryLayout::Earbuds,
            },
            wind_reduction: p.wind_reduction,
            speak_to_chat: p.speak_to_chat,
            equalizer: p.equalizer.map(Into::into),
            dsee: p.dsee,
        }
    }
}

impl From<Request> for core::Request {
    fn from(request: Request) -> Self {
        match request {
            Request::Init => Self::Init,
            Request::FirmwareVersion => Self::FirmwareVersion,
            Request::Battery => Self::Battery,
            Request::EarbudBattery => Self::EarbudBattery,
            Request::CaseBattery => Self::CaseBattery,
            Request::Codec => Self::Codec,
            Request::NoiseControl => Self::NoiseControl,
            Request::SetNoiseControl { value } => Self::SetNoiseControl(value.into()),
            Request::SpeakToChat => Self::SpeakToChat,
            Request::SetSpeakToChat { on } => Self::SetSpeakToChat(on),
            Request::Equalizer { layout } => Self::Equalizer(layout.into()),
            Request::SetEqualizerPreset { layout, preset } => Self::SetEqualizerPreset(layout.into(), preset.into()),
            Request::SetEqualizerBands { bands } => Self::SetEqualizerBands(bands.into()),
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
            core::Request::EarbudBattery => Self::EarbudBattery,
            core::Request::CaseBattery => Self::CaseBattery,
            core::Request::Codec => Self::Codec,
            core::Request::NoiseControl => Self::NoiseControl,
            core::Request::SetNoiseControl(value) => Self::SetNoiseControl { value: value.into() },
            core::Request::SpeakToChat => Self::SpeakToChat,
            core::Request::SetSpeakToChat(on) => Self::SetSpeakToChat { on },
            core::Request::Equalizer(layout) => Self::Equalizer { layout: layout.into() },
            core::Request::SetEqualizerPreset(layout, preset) => {
                Self::SetEqualizerPreset { layout: layout.into(), preset: preset.into() }
            }
            core::Request::SetEqualizerBands(bands) => Self::SetEqualizerBands { bands: bands.into() },
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
            core::Update::Battery(battery) => Self::Battery { battery: battery.into() },
            core::Update::EarbudBattery { left, right } => {
                Self::EarbudBattery { left: left.map(Into::into), right: right.map(Into::into) }
            }
            core::Update::CaseBattery(battery) => Self::CaseBattery { battery: battery.into() },
            core::Update::Codec(codec) => Self::Codec { name: codec.name() },
            core::Update::NoiseControl(nc) => Self::NoiseControl { value: nc.into() },
            core::Update::SpeakToChat(on) => Self::SpeakToChat { on },
            core::Update::Equalizer(eq) => {
                Self::Equalizer { value: Equalizer { preset: eq.preset.into(), bands: eq.bands.into() } }
            }
            core::Update::Dsee(on) => Self::Dsee { on },
        }
    }
}
