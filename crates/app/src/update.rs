//! Keeps Sound Connect up to date. It checks once a day whether a newer
//! release is out and, when the user asks, installs it with the same
//! one-line installer people used the first time, which replaces this app,
//! opens the new one and quits this one. Both go through `curl`, which that installer needs
//! anyway, so the app carries no HTTP or TLS code of its own.

use std::fmt;
use std::fs::{self, File};
use std::io;
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};
use std::time::Duration;

use gpui_kit::{App, BorrowAppContext, Global};

/// GitHub redirects this to the newest release's page, `.../tag/v1.2.3`.
const LATEST_RELEASE: &str = "https://github.com/aecca/sound-connect/releases/latest";
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
/// When a check fails, e.g. while offline right after logging in.
const RETRY_AFTER: Duration = Duration::from_secs(60 * 60);
/// The installer from the release itself; the tag comes in as `$1`.
const INSTALL: &str = r#"set -o pipefail; curl -fsSL "https://raw.githubusercontent.com/aecca/sound-connect/$1/install.sh" | SOUND_CONNECT_VERSION="$1" bash"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    major: u32,
    minor: u32,
    patch: u32,
}

impl Version {
    /// This build's version.
    fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).expect("the package version is major.minor.patch")
    }

    /// Reads "1.2.3"; anything else, such as a pre-release, isn't a version
    /// this checks for.
    fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split('.').map(|part| {
            // `u32::from_str` also takes a leading '+'; only digits count here.
            part.bytes().all(|byte| byte.is_ascii_digit()).then(|| part.parse().ok()).flatten()
        });
        let version = Self { major: parts.next()??, minor: parts.next()??, patch: parts.next()?? };
        parts.next().is_none().then_some(version)
    }

    /// The version a release page is for, from its URL.
    fn of_release_page(url: &str) -> Option<Self> {
        Self::parse(url.trim_end().rsplit_once("/tag/v")?.1)
    }

    fn tag(self) -> String {
        format!("v{self}")
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    UpToDate,
    Available(Version),
    Installing(Version),
    Failed(Version),
}

/// Whether there's a new version, for the panel to show.
pub struct Updates {
    pub status: Status,
}

impl Global for Updates {}

pub fn start(cx: &mut App) {
    cx.set_global(Updates { status: Status::UpToDate });
    cx.spawn(async |cx| {
        loop {
            let latest = cx.background_executor().spawn(async { latest_release() }).await;
            if let Some(latest) = latest.filter(|&latest| latest > Version::current()) {
                cx.update_global(|updates: &mut Updates, _| {
                    if !matches!(updates.status, Status::Installing(_)) {
                        updates.status = Status::Available(latest);
                    }
                });
            }
            cx.background_executor().timer(if latest.is_some() { CHECK_EVERY } else { RETRY_AFTER }).await;
        }
    })
    .detach();
}

/// Installs the available version, or tries again after a failure.
pub fn install(cx: &mut App) {
    let version = match cx.global::<Updates>().status {
        Status::Available(version) | Status::Failed(version) => version,
        Status::UpToDate | Status::Installing(_) => return,
    };
    cx.update_global(|updates: &mut Updates, _| updates.status = Status::Installing(version));
    cx.spawn(async move |cx| {
        match cx.background_executor().spawn(async move { run_installer(version) }).await {
            // The installer normally quits this app itself; if it's still
            // running, the new one is open already.
            Ok(status) if status.success() => cx.update(|cx| cx.quit()),
            result => {
                if let Err(error) = result {
                    eprintln!("couldn't run the installer: {error}");
                }
                cx.update_global(|updates: &mut Updates, _| updates.status = Status::Failed(version));
            }
        }
    })
    .detach();
}

/// The newest release, or None when it can't be told (e.g. offline).
fn latest_release() -> Option<Version> {
    let output = Command::new("curl")
        .args(["--fail", "--silent", "--head", "--max-time", "30", "--output", "/dev/null"])
        .args(["--write-out", "%{redirect_url}", LATEST_RELEASE])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    output.status.success().then(|| Version::of_release_page(&String::from_utf8_lossy(&output.stdout))).flatten()
}

fn run_installer(version: Version) -> io::Result<ExitStatus> {
    let log = log_file()?;
    Command::new("bash")
        .args(["-c", INSTALL, "bash", &version.tag()])
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .status()
}

/// Where the installer's output goes: $XDG_STATE_HOME/sound-connect/update.log.
fn log_file() -> io::Result<File> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no home folder"))?;
    let folder = state.join("sound-connect");
    fs::create_dir_all(&folder)?;
    File::create(folder.join("update.log"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_versions() {
        assert_eq!(Version::parse("0.10.2"), Some(Version { major: 0, minor: 10, patch: 2 }));
        assert!(Version::parse("0.10.2") > Version::parse("0.9.9"));
        for not_a_version in ["", "1.2", "1.2.3.4", "1.2.x", "1.2.3-beta", "+1.2.3", "v1.2.3"] {
            assert_eq!(Version::parse(not_a_version), None, "{not_a_version}");
        }
        assert_eq!(Version::current().to_string(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn reads_the_version_of_a_release_page() {
        let page = "https://github.com/aecca/sound-connect/releases/tag/v0.2.0";
        assert_eq!(Version::of_release_page(page), Version::parse("0.2.0"));
        assert_eq!(Version::of_release_page(page).map(Version::tag).as_deref(), Some("v0.2.0"));
        // No releases yet: GitHub sends the list instead.
        assert_eq!(Version::of_release_page("https://github.com/aecca/sound-connect/releases"), None);
        assert_eq!(Version::of_release_page("https://github.com/aecca/sound-connect/releases/tag/v1.0.0; rm"), None);
    }
}
