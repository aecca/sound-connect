//! Exposes the headphone drivers to Swift through UniFFI, so the native
//! macOS app and the Rust apps share one implementation of every protocol.
//!
//! The Swift side owns the Bluetooth channel: it looks for one of the
//! [`service_uuids`], opens the channel of its [`channel_service`], gets a
//! [`Headset`] for the device with [`headset_for`], passes it what the channel
//! reads and writes out every frame it returns.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use sc_core::{Battery, BatteryLayout, Brand, Driver, EqLayout, Feature, ListeningMode};

uniffi::setup_scaffolding!();

// Plain types from sc-core, exported as they are.

#[uniffi::remote(Enum)]
pub enum Brand {
    Sony,
    Bose,
}

#[uniffi::remote(Enum)]
pub enum BatteryLayout {
    Single,
    Earbuds,
}

#[uniffi::remote(Enum)]
pub enum ListeningMode {
    NoiseCancelling,
    NoiseCancellingLow,
    WindReduction,
    Ambient,
    Off,
}

#[uniffi::remote(Enum)]
pub enum Feature {
    SpeakToChat,
    Dsee,
    VoicePrompts,
    PauseWhenTakenOff,
}

#[uniffi::remote(Enum)]
pub enum EqLayout {
    FiveBands,
    TenBands,
}

#[uniffi::remote(Record)]
pub struct Battery {
    level: u8,
    charging: bool,
}

// Types that need a shape Swift can take: ranges, static strings, fixed-size
// arrays and unnamed fields become plain records and named fields.

/// A listening mode under the name the brand gives it.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Mode {
    pub mode: ListeningMode,
    pub name: String,
}

/// What can be adjusted in ambient mode.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Ambient {
    pub min_level: u8,
    pub max_level: u8,
    pub focus_on_voice: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum AutoOff {
    Never,
    Minutes { minutes: u16 },
    WhenTakenOff,
}

/// A headset and the controls it offers. See `sc_core::Device`.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Device {
    pub brand: Brand,
    pub model: String,
    pub tested: bool,
    pub battery: BatteryLayout,
    pub modes: Vec<Mode>,
    pub ambient: Option<Ambient>,
    pub features: Vec<Feature>,
    pub auto_off: Vec<AutoOff>,
    pub equalizer: Option<EqLayout>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, uniffi::Enum)]
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

/// Something the headset reported. See `sc_core::Update`.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Update {
    Device { device: Device },
    Unsupported { reason: String },
    Firmware { version: String },
    Codec { name: String },
    Battery { battery: Battery },
    Earbuds { left: Option<Battery>, right: Option<Battery> },
    CaseBattery { battery: Battery },
    Mode { mode: ListeningMode },
    AmbientLevel { level: u8 },
    FocusOnVoice { on: bool },
    Feature { feature: Feature, on: bool },
    AutoOff { auto_off: AutoOff },
    Equalizer { equalizer: Equalizer },
}

/// Something the user changed. See `sc_core::Setting`.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Setting {
    Mode { mode: ListeningMode },
    AmbientLevel { level: u8 },
    FocusOnVoice { on: bool },
    Feature { feature: Feature, on: bool },
    AutoOff { auto_off: AutoOff },
    EqualizerPreset { preset: EqPreset },
    EqualizerBands { bands: EqBands },
}

/// What to do after the headset said something: frames to write, in order,
/// and changes to show.
#[derive(Debug, uniffi::Record)]
pub struct Output {
    pub writes: Vec<Vec<u8>>,
    pub updates: Vec<Update>,
}

/// One connected headset and the driver that speaks to it.
#[derive(uniffi::Object)]
pub struct Headset(Mutex<Box<dyn Driver>>);

/// The control services of supported headsets, in order of preference.
#[uniffi::export]
pub fn service_uuids() -> Vec<String> {
    sc_core::SERVICES.iter().map(|service| (*service).to_owned()).collect()
}

/// The service whose RFCOMM channel carries the protocol of a headset recognized by `service`.
#[uniffi::export]
pub fn channel_service(service: String) -> String {
    sc_core::channel_service(&service).to_owned()
}

/// The headset with this Bluetooth name, given the control service it offers.
#[uniffi::export]
pub fn headset_for(name: String, service: String) -> Option<Arc<Headset>> {
    sc_core::driver_for(&name, &service).map(|driver| Arc::new(Headset(Mutex::new(driver))))
}

#[uniffi::export]
impl Headset {
    pub fn device(&self) -> Device {
        self.driver().device().clone().into()
    }

    /// Call once the channel is open.
    pub fn start(&self) -> Output {
        self.driver().start(Instant::now()).into()
    }

    pub fn receive(&self, bytes: Vec<u8>) -> Output {
        self.driver().receive(&bytes, Instant::now()).into()
    }

    /// Returns the frames to write for a change the user made.
    pub fn set(&self, setting: Setting) -> Vec<Vec<u8>> {
        self.driver().set(setting.into(), Instant::now())
    }

    /// Call every minute or so, for values that change on their own.
    pub fn refresh(&self) -> Vec<Vec<u8>> {
        self.driver().refresh(Instant::now())
    }

    /// Call a few times a second.
    pub fn tick(&self) -> Vec<Vec<u8>> {
        self.driver().tick(Instant::now())
    }
}

impl Headset {
    fn driver(&self) -> std::sync::MutexGuard<'_, Box<dyn Driver>> {
        // A panic while holding the lock leaves the driver as it was; keep using it.
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Ambient sound during calls. See `sc_core::CallAmbience`.
#[derive(Default, uniffi::Object)]
pub struct CallAmbience(Mutex<sc_core::CallAmbience>);

#[uniffi::export]
impl CallAmbience {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::default()
    }

    /// Call every second or so with whether any other app is recording and
    /// the headset's mode (`None` without ambient sound). Returns the mode to
    /// switch the headset to, if any.
    pub fn update(&self, recording: bool, mode: Option<ListeningMode>) -> Option<ListeningMode> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).update(recording, mode, Instant::now())
    }
}

/// Every equalizer preset, in the order Sony's app lists them.
#[uniffi::export]
pub fn eq_presets() -> Vec<EqPreset> {
    sc_core::EqPreset::ALL.into_iter().map(Into::into).collect()
}

#[uniffi::export]
pub fn eq_preset_name(preset: EqPreset) -> String {
    sc_core::EqPreset::from(preset).name()
}

#[uniffi::export]
pub fn brand_name(brand: Brand) -> String {
    brand.name().to_owned()
}

#[uniffi::export]
pub fn feature_name(feature: Feature) -> String {
    feature.name().to_owned()
}

#[uniffi::export]
pub fn feature_description(feature: Feature) -> String {
    feature.description().to_owned()
}

#[uniffi::export]
pub fn auto_off_name(auto_off: AutoOff) -> String {
    sc_core::AutoOff::from(auto_off).name()
}

// Conversions to and from the sc-core types.

impl From<sc_core::Device> for Device {
    fn from(device: sc_core::Device) -> Self {
        Self {
            brand: device.brand,
            model: device.model,
            tested: device.tested,
            battery: device.battery,
            modes: device.modes.into_iter().map(|mode| Mode { mode: mode.mode, name: mode.name.to_owned() }).collect(),
            ambient: device.ambient.map(|ambient| Ambient {
                min_level: *ambient.levels.start(),
                max_level: *ambient.levels.end(),
                focus_on_voice: ambient.focus_on_voice,
            }),
            features: device.features,
            auto_off: device.auto_off.into_iter().map(Into::into).collect(),
            equalizer: device.equalizer,
        }
    }
}

impl From<sc_core::AutoOff> for AutoOff {
    fn from(auto_off: sc_core::AutoOff) -> Self {
        match auto_off {
            sc_core::AutoOff::Never => Self::Never,
            sc_core::AutoOff::Minutes(minutes) => Self::Minutes { minutes },
            sc_core::AutoOff::WhenTakenOff => Self::WhenTakenOff,
        }
    }
}

impl From<AutoOff> for sc_core::AutoOff {
    fn from(auto_off: AutoOff) -> Self {
        match auto_off {
            AutoOff::Never => Self::Never,
            AutoOff::Minutes { minutes } => Self::Minutes(minutes),
            AutoOff::WhenTakenOff => Self::WhenTakenOff,
        }
    }
}

impl From<sc_core::EqPreset> for EqPreset {
    fn from(preset: sc_core::EqPreset) -> Self {
        use sc_core::EqPreset as P;
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

impl From<EqPreset> for sc_core::EqPreset {
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

impl From<sc_core::EqBands> for EqBands {
    fn from(bands: sc_core::EqBands) -> Self {
        match bands {
            sc_core::EqBands::Five { clear_bass, bands } => Self::Five { clear_bass, bands: bands.to_vec() },
            sc_core::EqBands::Ten(bands) => Self::Ten { bands: bands.to_vec() },
        }
    }
}

impl From<EqBands> for sc_core::EqBands {
    fn from(bands: EqBands) -> Self {
        match bands {
            EqBands::Five { clear_bass, bands } => Self::Five { clear_bass, bands: fixed(bands) },
            EqBands::Ten { bands } => Self::Ten(fixed(bands)),
        }
    }
}

/// Copies a list into a fixed-size array: missing values are zero, extra
/// ones are dropped.
fn fixed<T: Copy + Default, const N: usize>(values: Vec<T>) -> [T; N] {
    let mut array = [T::default(); N];
    for (slot, value) in array.iter_mut().zip(values) {
        *slot = value;
    }
    array
}

impl From<sc_core::Equalizer> for Equalizer {
    fn from(equalizer: sc_core::Equalizer) -> Self {
        Self { preset: equalizer.preset.into(), bands: equalizer.bands.into() }
    }
}

impl From<sc_core::Update> for Update {
    fn from(update: sc_core::Update) -> Self {
        use sc_core::Update as U;
        match update {
            U::Device(device) => Self::Device { device: device.into() },
            U::Unsupported(reason) => Self::Unsupported { reason },
            U::Firmware(version) => Self::Firmware { version },
            U::Codec(name) => Self::Codec { name },
            U::Battery(battery) => Self::Battery { battery },
            U::Earbuds { left, right } => Self::Earbuds { left, right },
            U::CaseBattery(battery) => Self::CaseBattery { battery },
            U::Mode(mode) => Self::Mode { mode },
            U::AmbientLevel(level) => Self::AmbientLevel { level },
            U::FocusOnVoice(on) => Self::FocusOnVoice { on },
            U::Feature(feature, on) => Self::Feature { feature, on },
            U::AutoOff(auto_off) => Self::AutoOff { auto_off: auto_off.into() },
            U::Equalizer(equalizer) => Self::Equalizer { equalizer: equalizer.into() },
        }
    }
}

impl From<Setting> for sc_core::Setting {
    fn from(setting: Setting) -> Self {
        match setting {
            Setting::Mode { mode } => Self::Mode(mode),
            Setting::AmbientLevel { level } => Self::AmbientLevel(level),
            Setting::FocusOnVoice { on } => Self::FocusOnVoice(on),
            Setting::Feature { feature, on } => Self::Feature(feature, on),
            Setting::AutoOff { auto_off } => Self::AutoOff(auto_off.into()),
            Setting::EqualizerPreset { preset } => Self::EqualizerPreset(preset.into()),
            Setting::EqualizerBands { bands } => Self::EqualizerBands(bands.into()),
        }
    }
}

impl From<sc_core::Output> for Output {
    fn from(output: sc_core::Output) -> Self {
        Self { writes: output.writes, updates: output.updates.into_iter().map(Into::into).collect() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_drivers_by_service() {
        let services = service_uuids();
        let sony = headset_for("WH-1000XM5".into(), services[0].clone()).expect("Sony is supported");
        assert_eq!(sony.device().brand, Brand::Sony);
        assert!(headset_for("Speaker".into(), "0000110B-0000-1000-8000-00805F9B34FB".into()).is_none());
        assert_eq!(channel_service(services[0].clone()), services[0]);
        assert_eq!(channel_service(services[2].clone()), "00001101-0000-1000-8000-00805F9B34FB");
    }

    #[test]
    fn converts_equalizer_bands_both_ways() {
        let bands = sc_core::EqBands::Five { clear_bass: 3, bands: [2, 1, 0, -1, -2] };
        assert_eq!(sc_core::EqBands::from(EqBands::from(bands)), bands);
        let short = EqBands::Ten { bands: vec![1, 2] };
        assert_eq!(sc_core::EqBands::from(short), sc_core::EqBands::Ten([1, 2, 0, 0, 0, 0, 0, 0, 0, 0]));
    }
}
