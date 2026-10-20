//! Whether an app is recording from a microphone, as in a call.
//!
//! PipeWire knows: `pw-dump --monitor`, which comes with it, prints every
//! object as JSON and then each one again whenever it changes. A recording
//! is an input stream that's running. Streams that capture what's playing
//! (screen recorders) or only measure levels (volume meters) don't count.

use std::collections::HashSet;
use std::io::{self, BufReader};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use serde_json::Value;

/// Wait before watching again after `pw-dump` stops, e.g. because PipeWire
/// restarted.
const RESTART_AFTER: Duration = Duration::from_secs(10);

/// Starts watching. The receiver gets `true` when recording starts and
/// `false` when it stops. `None` without PipeWire.
pub fn watch() -> Option<async_channel::Receiver<bool>> {
    let child = spawn().inspect_err(|error| eprintln!("can't tell when the microphone is in use: {error}")).ok()?;
    let (changes, receiver) = async_channel::unbounded();
    thread::Builder::new()
        .name("microphone".into())
        .spawn(move || {
            let mut child = Some(child);
            loop {
                if let Some(mut running) = child.take() {
                    report(&mut running, &changes);
                    let _ = running.kill();
                    let _ = running.wait();
                }
                if changes.is_closed() {
                    return;
                }
                thread::sleep(RESTART_AFTER);
                child = spawn().ok();
            }
        })
        .expect("failed to start the microphone thread");
    Some(receiver)
}

fn spawn() -> io::Result<Child> {
    Command::new("pw-dump")
        .args(["--monitor", "--no-colors"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
}

/// Sends each change until `pw-dump` exits or the app stops listening.
fn report(child: &mut Child, changes: &async_channel::Sender<bool>) {
    let Some(stdout) = child.stdout.take() else { return };
    let mut streams = Recordings::default();
    let mut recording = false;
    for objects in serde_json::Deserializer::from_reader(BufReader::new(stdout)).into_iter::<Vec<Value>>() {
        let Ok(objects) = objects else { break };
        streams.apply(&objects);
        if streams.any() != recording {
            recording = !recording;
            if changes.send_blocking(recording).is_err() {
                return;
            }
        }
    }
    // Nothing is known to be recording once PipeWire stops saying.
    if recording {
        let _ = changes.send_blocking(false);
    }
}

/// The running input streams, by PipeWire object ID.
#[derive(Debug, Default)]
struct Recordings(HashSet<u64>);

impl Recordings {
    /// Applies one batch of objects as `pw-dump` prints them. One that went
    /// away comes with `"info": null`.
    fn apply(&mut self, objects: &[Value]) {
        for object in objects {
            let Some(id) = object["id"].as_u64() else { continue };
            if records(&object["info"]) {
                self.0.insert(id);
            } else {
                self.0.remove(&id);
            }
        }
    }

    fn any(&self) -> bool {
        !self.0.is_empty()
    }
}

fn records(info: &Value) -> bool {
    let props = &info["props"];
    // Properties come as JSON booleans or as strings, depending on who set them.
    let flag = |key: &str| props[key] == true || props[key] == "true";
    info["state"] == "running"
        && props["media.class"] == "Stream/Input/Audio"
        && !flag("stream.monitor")
        && !flag("stream.capture.sink")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A recording stream as `pw-dump --monitor` printed it (PipeWire 1.0.5),
    /// trimmed to the fields that matter here.
    fn stream(id: u64, state: &str, props: Value) -> Value {
        let mut base = serde_json::json!({
            "application.name": "pw-record",
            "media.category": "Capture",
            "media.class": "Stream/Input/Audio",
            "node.name": "pw-record",
            "stream.is-live": true,
        });
        base.as_object_mut().unwrap().extend(props.as_object().unwrap().clone());
        serde_json::json!({
            "id": id,
            "type": "PipeWire:Interface:Node",
            "info": { "change-mask": ["state"], "state": state, "error": null, "props": base },
        })
    }

    fn plain() -> Value {
        serde_json::json!({})
    }

    #[test]
    fn follows_a_recording_from_start_to_end() {
        let mut recordings = Recordings::default();
        recordings.apply(&[stream(42, "suspended", plain())]);
        assert!(!recordings.any());
        recordings.apply(&[stream(42, "running", plain())]);
        assert!(recordings.any());
        recordings.apply(&[serde_json::json!({ "id": 42, "info": null })]);
        assert!(!recordings.any());
    }

    #[test]
    fn counts_until_the_last_recording_stops() {
        let mut recordings = Recordings::default();
        recordings.apply(&[stream(42, "running", plain()), stream(50, "running", plain())]);
        recordings.apply(&[stream(42, "idle", plain())]);
        assert!(recordings.any());
        recordings.apply(&[stream(50, "suspended", plain())]);
        assert!(!recordings.any());
    }

    #[test]
    fn ignores_level_meters_screen_recorders_and_other_objects() {
        let mut recordings = Recordings::default();
        recordings.apply(&[
            stream(1, "running", serde_json::json!({ "stream.monitor": true })),
            stream(2, "running", serde_json::json!({ "stream.capture.sink": "true" })),
            stream(3, "running", serde_json::json!({ "media.class": "Stream/Output/Audio" })),
            serde_json::json!({ "id": 4, "type": "PipeWire:Interface:Link", "info": { "state": "active" } }),
            serde_json::json!({ "id": 5, "type": "PipeWire:Interface:Metadata", "metadata": [] }),
        ]);
        assert!(!recordings.any());
    }

    #[test]
    fn reads_batches_as_they_come() {
        let printed = format!(
            "[\n{}\n]\n[\n{}\n]\n",
            stream(42, "running", plain()),
            serde_json::json!({ "id": 42, "info": null })
        );
        let mut recordings = Recordings::default();
        let mut seen = Vec::new();
        for batch in serde_json::Deserializer::from_str(&printed).into_iter::<Vec<Value>>() {
            recordings.apply(&batch.unwrap());
            seen.push(recordings.any());
        }
        assert_eq!(seen, [true, false]);
    }
}
