//! Linux transport over BlueZ, through BlueR.
//!
//! BlueZ owns SDP and RFCOMM: we register a client profile for the headset's
//! control service, ask BlueZ to connect it, and BlueZ hands the connected
//! socket to our profile. It all runs on a single-threaded Tokio runtime
//! inside the Bluetooth thread.

use std::error::Error;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use bluer::rfcomm::{Profile, ProfileHandle, ReqError, Role, Stream};
use bluer::{Device, Uuid};
use futures::{FutureExt, StreamExt};
use sc_core::{Driver, Frame, Output, SERVICES, Setting, driver_for};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use tokio::time;

use crate::Event;

/// Wait between attempts to find the headset, growing while it keeps failing.
const RETRY_CONNECT: [Duration; 4] =
    [Duration::from_secs(3), Duration::from_secs(5), Duration::from_secs(10), Duration::from_secs(30)];
const REFRESH_EVERY: Duration = Duration::from_secs(60);
/// How often the driver may resend a request the headset didn't answer.
const TICK: Duration = Duration::from_millis(100);
/// How long BlueZ gets to look up the channel and open it.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const DISCONNECT_TIMEOUT: Duration = Duration::from_secs(5);

pub fn run(name_filter: Option<String>, requests: mpsc::Receiver<Setting>, events: async_channel::Sender<Event>) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to start the Bluetooth runtime");
    runtime.block_on(link(name_filter, forward(requests), events));
}

/// Moves settings from the UI onto a channel the runtime can await.
fn forward(requests: mpsc::Receiver<Setting>) -> UnboundedReceiver<Setting> {
    let (tx, rx) = unbounded_channel();
    std::thread::Builder::new()
        .name("bluetooth-requests".into())
        .spawn(move || {
            while let Ok(request) = requests.recv() {
                if tx.send(request).is_err() {
                    return;
                }
            }
        })
        .expect("failed to start the Bluetooth request thread");
    rx
}

async fn link(
    name_filter: Option<String>,
    mut requests: UnboundedReceiver<Setting>,
    events: async_channel::Sender<Event>,
) {
    let services = services();
    let mut bluez = None;
    let mut searching = false;
    let mut failures = 0;
    loop {
        if bluez.is_none() {
            bluez = bluer::Session::new()
                .await
                .inspect_err(|error| eprintln!("bluetooth: can't reach BlueZ: {error}"))
                .ok();
        }
        let channel = match &bluez {
            Some(bluez) => connect(bluez, &services, name_filter.as_deref()).await,
            None => None,
        };
        match channel {
            Some(mut channel) => {
                searching = false;
                failures = 0;
                let _ = events.send(Event::Connected { name: channel.headset.name.clone() }).await;
                let keep_going =
                    serve(&mut channel.stream, channel.headset.driver.as_mut(), &mut requests, &events).await;
                channel.close().await;
                let _ = events.send(Event::Disconnected).await;
                if !keep_going {
                    return;
                }
            }
            None => {
                failures += 1;
                if !searching {
                    searching = true;
                    let _ = events.send(Event::Searching).await;
                }
            }
        }

        // Wait before looking again. Settings changed meanwhile are dropped:
        // the UI reads everything again once a headset connects.
        let delay = RETRY_CONNECT[failures.min(RETRY_CONNECT.len() - 1)];
        let app_gone = time::timeout(delay, async { while requests.recv().await.is_some() {} }).await.is_ok();
        if app_gone || events.is_closed() {
            return;
        }
    }
}

/// Talks to the headset until the channel closes. Returns false when the app
/// has gone away and the thread should stop.
async fn serve(
    stream: &mut Stream,
    driver: &mut dyn Driver,
    requests: &mut UnboundedReceiver<Setting>,
    events: &async_channel::Sender<Event>,
) -> bool {
    let mut frames = deliver(driver.start(Instant::now()), events).await;
    let mut tick = time::interval(TICK);
    let mut refresh = time::interval_at(time::Instant::now() + REFRESH_EVERY, REFRESH_EVERY);
    let mut buffer = [0; 1024];

    loop {
        if write(stream, frames).await.is_err() {
            return true;
        }
        frames = tokio::select! {
            read = stream.read(&mut buffer) => {
                // Zero bytes means the headset hung up.
                let Ok(n @ 1..) = read else { return true };
                deliver(driver.receive(&buffer[..n], Instant::now()), events).await
            }
            setting = requests.recv() => match setting {
                Some(setting) => driver.set(setting, Instant::now()),
                None => return false,
            },
            _ = tick.tick() => driver.tick(Instant::now()),
            _ = refresh.tick() => driver.refresh(Instant::now()),
        };
    }
}

/// Passes the driver's updates on and returns the frames it wants written.
async fn deliver(output: Output, events: &async_channel::Sender<Event>) -> Vec<Frame> {
    for update in output.updates {
        let _ = events.send(Event::Update(update)).await;
    }
    output.writes
}

async fn write(stream: &mut Stream, frames: Vec<Frame>) -> std::io::Result<()> {
    for frame in frames {
        stream.write_all(&frame).await?;
    }
    Ok(())
}

/// The supported control services, newest first, as BlueZ names them.
fn services() -> Vec<(&'static str, Uuid)> {
    SERVICES.iter().filter_map(|&service| Some((service, Uuid::parse_str(service).ok()?))).collect()
}

/// A paired, connected device that offers a supported control service.
struct Headset {
    device: Device,
    name: String,
    service: Uuid,
    driver: Box<dyn Driver>,
}

impl Headset {
    async fn check(
        device: Device,
        services: &[(&'static str, Uuid)],
        name_filter: Option<&str>,
    ) -> bluer::Result<Option<Self>> {
        if !device.is_paired().await? || !device.is_connected().await? {
            return Ok(None);
        }
        let name = device.alias().await?;
        if name_filter.is_some_and(|filter| !name.contains(filter)) {
            return Ok(None);
        }
        let uuids = device.uuids().await?.unwrap_or_default();
        let Some(&(service_name, service)) = services.iter().find(|(_, uuid)| uuids.contains(uuid)) else {
            return Ok(None);
        };
        let driver = driver_for(&name, service_name);
        Ok(driver.map(|driver| Self { device, name, service, driver }))
    }

    /// Asks BlueZ to drop the profile connection, which closes its copy of
    /// the socket. Ours alone doesn't keep the channel open.
    async fn disconnect(&self) {
        let _ = time::timeout(DISCONNECT_TIMEOUT, self.device.disconnect_profile(&self.service)).await;
    }
}

/// An open control channel, with the profiles that keep it open.
struct Channel {
    headset: Headset,
    stream: Stream,
    _profiles: Vec<(Uuid, ProfileHandle)>,
}

impl Channel {
    async fn close(self) {
        self.headset.disconnect().await;
        // Dropping the stream closes our socket; dropping the profiles
        // unregisters them before the next attempt registers new ones.
    }
}

/// Finds a connected headset offering a supported service and opens its
/// control channel.
async fn connect(
    bluez: &bluer::Session,
    services: &[(&'static str, Uuid)],
    name_filter: Option<&str>,
) -> Option<Channel> {
    let headsets = headsets(bluez, services, name_filter).await;
    if headsets.is_empty() {
        return None;
    }
    let mut profiles = match register(bluez, services, &headsets).await {
        Ok(profiles) => profiles,
        Err(error) => {
            eprintln!("bluetooth: can't register the control profile with BlueZ: {error}");
            return None;
        }
    };
    for headset in headsets {
        let Some((_, profile)) = profiles.iter_mut().find(|(uuid, _)| *uuid == headset.service) else {
            continue;
        };
        match open(&headset, profile).await {
            Ok(stream) => return Some(Channel { headset, stream, _profiles: profiles }),
            Err(error) => {
                eprintln!("bluetooth: can't open the control channel of {}: {error}", headset.name);
                // BlueZ may have connected the profile after we gave up on it.
                headset.disconnect().await;
            }
        }
    }
    None
}

async fn headsets(
    bluez: &bluer::Session,
    services: &[(&'static str, Uuid)],
    name_filter: Option<&str>,
) -> Vec<Headset> {
    let mut found = Vec::new();
    for name in bluez.adapter_names().await.unwrap_or_default() {
        let Ok(adapter) = bluez.adapter(&name) else { continue };
        for address in adapter.device_addresses().await.unwrap_or_default() {
            let Ok(device) = adapter.device(address) else { continue };
            if let Ok(Some(headset)) = Headset::check(device, services, name_filter).await {
                found.push(headset);
            }
        }
    }
    found
}

/// Registers a client profile for each service the headsets use. BlueZ
/// looks up the RFCOMM channel and hands the socket over through it.
async fn register(
    bluez: &bluer::Session,
    services: &[(&'static str, Uuid)],
    headsets: &[Headset],
) -> bluer::Result<Vec<(Uuid, ProfileHandle)>> {
    let mut profiles = Vec::new();
    for &(_, uuid) in services {
        if !headsets.iter().any(|headset| headset.service == uuid) {
            continue;
        }
        let profile = Profile {
            uuid,
            name: Some("Sound Connect".into()),
            role: Some(Role::Client),
            auto_connect: Some(false),
            ..Default::default()
        };
        profiles.push((uuid, bluez.register_profile(profile).await?));
    }
    Ok(profiles)
}

/// Connects the headset's service and accepts the socket BlueZ hands over.
/// BlueZ only answers `ConnectProfile` once the socket is taken, so both
/// run at once.
async fn open(headset: &Headset, profile: &mut ProfileHandle) -> Result<Stream, Box<dyn Error>> {
    // Turn away sockets from attempts that already gave up.
    while let Some(Some(request)) = profile.next().now_or_never() {
        request.reject(ReqError::Canceled);
    }
    let address = headset.device.address();
    let accept = async {
        while let Some(request) = profile.next().await {
            if request.device() == address {
                return request.accept();
            }
            request.reject(ReqError::Rejected);
        }
        Err(bluer::Error { kind: bluer::ErrorKind::Failed, message: "the profile was unregistered".into() })
    };
    let connect = headset.device.connect_profile(&headset.service);
    let (_, stream) = time::timeout(CONNECT_TIMEOUT, async { tokio::try_join!(connect, accept) }).await??;
    Ok(stream)
}
