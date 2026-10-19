//! Connects to the headset, prints what it reports and optionally changes
//! its listening mode. Run it with `scripts/macos.sh probe [mode] [level]` on
//! macOS, or `cargo run -p sc-bluetooth --example probe -- [mode] [level]` on
//! Linux. Modes: nc, low, wind, ambient, off; the level is for ambient.

use std::time::{Duration, Instant};

use sc_bluetooth::Event;
use sc_core::{ListeningMode, Setting, Update};

fn main() {
    let mut args = std::env::args().skip(1);
    let mode = args.next().map(|arg| match arg.as_str() {
        "nc" => ListeningMode::NoiseCancelling,
        "low" => ListeningMode::NoiseCancellingLow,
        "wind" => ListeningMode::WindReduction,
        "ambient" => ListeningMode::Ambient,
        "off" => ListeningMode::Off,
        other => panic!("unknown mode {other:?}; use nc, low, wind, ambient or off"),
    });
    let level: Option<u8> = args.next().map(|level| level.parse().expect("the level is a number"));

    let link = sc_bluetooth::start(None);
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut sent = false;

    while Instant::now() < deadline {
        wait_a_moment();
        let event = match link.events.try_recv() {
            Ok(event) => event,
            Err(async_channel::TryRecvError::Empty) => continue,
            Err(async_channel::TryRecvError::Closed) => break,
        };
        println!("{event:?}");
        match event {
            // Wait until the headset has reported its state.
            Event::Update(Update::Mode(_)) if !sent => {
                let Some(mode) = mode else { break };
                sent = true;
                println!("-> {mode:?}");
                link.requests.send(Setting::Mode(mode)).expect("the link is running");
                if let Some(level) = level {
                    link.requests.send(Setting::AmbientLevel(level)).expect("the link is running");
                }
            }
            Event::Update(Update::Mode(_)) => break,
            Event::Update(Update::Unsupported(_)) | Event::Unsupported(_) => break,
            _ => {}
        }
    }
}

/// IOBluetooth delivers channel data on the main run loop, so spin it.
#[cfg(target_os = "macos")]
fn wait_a_moment() {
    use objc2_foundation::{NSDate, NSDefaultRunLoopMode, NSRunLoop};
    let until = NSDate::dateWithTimeIntervalSinceNow(0.02);
    unsafe { NSRunLoop::mainRunLoop().runMode_beforeDate(NSDefaultRunLoopMode, &until) };
}

#[cfg(not(target_os = "macos"))]
fn wait_a_moment() {
    std::thread::sleep(Duration::from_millis(20));
}
