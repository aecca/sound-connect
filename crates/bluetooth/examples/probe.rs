//! Connects to the headset, prints what it reports and optionally changes
//! the noise mode. Run it with `scripts/macos.sh probe [nc|ambient|off] [level]`
//! on macOS, or `cargo run -p sc-bluetooth --example probe -- [args]` on Linux.

use std::time::{Duration, Instant};

use sc_bluetooth::{Event, NoiseControl, NoiseMode, Request, Update};

fn main() {
    let mut args = std::env::args().skip(1);
    let mode = args.next().map(|arg| match arg.as_str() {
        "nc" => NoiseMode::NoiseCancelling,
        "ambient" => NoiseMode::Ambient,
        "off" => NoiseMode::Off,
        other => panic!("unknown mode {other:?}; use nc, ambient or off"),
    });
    let level: u8 = args.next().map_or(20, |l| l.parse().expect("level must be 1-20"));

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
            // Wait for the current state before changing anything.
            Event::Update(Update::NoiseControl(current)) if !sent => {
                if let Some(mode) = mode {
                    sent = true;
                    let change = NoiseControl { mode, ambient_level: level, ..current };
                    println!("-> {change:?}");
                    link.requests.send(Request::SetNoiseControl(change)).unwrap();
                    link.requests.send(Request::NoiseControl).unwrap();
                } else {
                    break;
                }
            }
            Event::Update(Update::NoiseControl(_)) if sent => break,
            Event::Unsupported(_) => break,
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
