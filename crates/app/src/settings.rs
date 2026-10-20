//! The app's own preferences, kept in
//! `$XDG_CONFIG_HOME/sound-connect/settings.json`. The headset keeps its own
//! settings; these are about what the app does.

use std::path::PathBuf;
use std::{fs, io};

use gpui_kit::{App, BorrowAppContext, Global};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Settings {
    /// Switch to ambient sound while an app uses the microphone.
    pub ambient_during_calls: bool,
    /// Show a notification when the battery runs low.
    pub low_battery_alerts: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { ambient_during_calls: false, low_battery_alerts: true }
    }
}

impl Global for Settings {}

impl Settings {
    /// The saved settings, or the defaults when there are none yet.
    pub fn load() -> Self {
        let Some(path) = path() else { return Self::default() };
        match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|error| {
                eprintln!("ignoring {}: {error}", path.display());
                Self::default()
            }),
            Err(error) => {
                if error.kind() != io::ErrorKind::NotFound {
                    eprintln!("couldn't read {}: {error}", path.display());
                }
                Self::default()
            }
        }
    }

    fn save(&self) {
        let Some(path) = path() else { return };
        let saved = path.parent().map_or(Ok(()), fs::create_dir_all).and_then(|()| {
            let text = serde_json::to_string_pretty(self).expect("settings are plain values");
            fs::write(&path, text + "\n")
        });
        if let Err(error) = saved {
            eprintln!("couldn't save {}: {error}", path.display());
        }
    }
}

/// Changes the settings and saves them.
pub fn change(cx: &mut App, change: impl FnOnce(&mut Settings)) {
    cx.update_global(|settings: &mut Settings, _| {
        change(settings);
        settings.save();
    });
}

fn path() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(config.join("sound-connect/settings.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_it_writes_and_tolerates_missing_or_unknown_keys() {
        let settings = Settings { ambient_during_calls: true, low_battery_alerts: false };
        let text = serde_json::to_string(&settings).unwrap();
        assert_eq!(text, r#"{"ambient-during-calls":true,"low-battery-alerts":false}"#);
        assert_eq!(serde_json::from_str::<Settings>(&text).unwrap(), settings);
        assert_eq!(serde_json::from_str::<Settings>(r#"{"from-a-newer-version":1}"#).unwrap(), Settings::default());
        let older = serde_json::from_str::<Settings>(r#"{"ambient-during-calls":true}"#).unwrap();
        assert!(older.low_battery_alerts);
    }
}
