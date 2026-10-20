//! Linux transport over BlueZ, through BlueR.
//!
//! BlueZ owns SDP and RFCOMM: we register a client profile for the service
//! carrying the headset's protocol, ask BlueZ to connect it, and BlueZ hands
//! the connected socket to our profile. It all runs on a single-threaded Tokio runtime
//! inside the Bluetooth thread.

use std::collections::HashSet;
use std::error::Error;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use bluer::rfcomm::{Profile, ProfileHandle, ReqError, Role, Stream};
use bluer::{Device, Uuid};
use futures::{FutureExt, StreamExt};
use sc_core::{Driver, Frame, Output, SERVICES, channel_service, driver_for};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use tokio::time;

use crate::{Event, PairedHeadset, Request};

/// Wait between attempts to find the headset, growing while it keeps failing.
const RETRY_CONNECT: [Duration; 4] =
    [Duration::from_secs(3), Duration::from_secs(5), Duration::from_secs(10), Duration::from_secs(30)];
const REFRESH_EVERY: Duration = Duration::from_secs(60);
/// How often the driver may resend a request the headset didn't answer.
const TICK: Duration = Duration::from_millis(100);
/// How long BlueZ gets to look up the channel and open it.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const DISCONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a headset gets to set up its audio after connecting, before its
/// control channel is opened.
const SETTLE_AFTER_CONNECTING: Duration = Duration::from_millis(1500);

pub fn run(name_filter: Option<String>, requests: mpsc::Receiver<Request>, events: async_channel::Sender<Event>) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to start the Bluetooth runtime");
    runtime.block_on(link(name_filter, forward(requests), events));
}

/// Moves requests from the UI onto a channel the runtime can await.
fn forward(requests: mpsc::Receiver<Request>) -> UnboundedReceiver<Request> {
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
    mut requests: UnboundedReceiver<Request>,
    events: async_channel::Sender<Event>,
) {
    let services = services();
    let mut bluez = None;
    let mut searching = false;
    let mut failures = 0;
    let mut offered = Vec::new();
    // A headset the user connected, whose control channel should open next.
    let mut expected = false;
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
                expected = false;
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
                if std::mem::take(&mut expected) {
                    let _ = events.send(Event::ControlsUnreachable).await;
                }
            }
        }

        // Offer the paired headsets that aren't connected.
        if let Some(bluez) = &bluez {
            let paired = paired(bluez, &services, name_filter.as_deref()).await;
            if paired != offered {
                offered.clone_from(&paired);
                let _ = events.send(Event::Paired(paired)).await;
            }
        }

        // Wait before looking again, unless the user picks a headset to connect.
        let delay = RETRY_CONNECT[failures.min(RETRY_CONNECT.len() - 1)];
        match time::timeout(delay, picked(&mut requests)).await {
            Ok(Some(address)) => {
                let connected = match &bluez {
                    Some(bluez) => connect_paired(bluez, &address).await,
                    None => Err("BlueZ isn't running".into()),
                };
                match connected {
                    Ok(()) => {
                        failures = 0;
                        expected = true;
                        time::sleep(SETTLE_AFTER_CONNECTING).await;
                    }
                    Err(error) => {
                        eprintln!("bluetooth: can't connect the headset the user picked: {error}");
                        let _ = events.send(Event::ConnectFailed).await;
                    }
                }
            }
            // The app has gone away.
            Ok(None) => return,
            Err(_) => {}
        }
        if events.is_closed() {
            return;
        }
    }
}

/// Waits for the user to pick a paired headset to connect, and returns its
/// address. Settings changed meanwhile are dropped: the UI reads everything
/// again once a headset connects. `None` once the app has gone away.
async fn picked(requests: &mut UnboundedReceiver<Request>) -> Option<String> {
    while let Some(request) = requests.recv().await {
        if let Request::Connect(address) = request {
            return Some(address);
        }
    }
    None
}

/// Talks to the headset until the channel closes. Returns false when the app
/// has gone away and the thread should stop.
async fn serve(
    stream: &mut Stream,
    driver: &mut dyn Driver,
    requests: &mut UnboundedReceiver<Request>,
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
            request = requests.recv() => match request {
                Some(Request::Change(setting)) => driver.set(setting, Instant::now()),
                // A headset is already connected.
                Some(Request::Connect(_)) => Vec::new(),
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
    /// The service whose channel carries its protocol.
    channel: Uuid,
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
        let Some(&(service_name, _)) = services.iter().find(|(_, uuid)| uuids.contains(uuid)) else {
            return Ok(None);
        };
        let Some(channel) = channel_for(service_name, &uuids) else { return Ok(None) };
        let driver = driver_for(&name, service_name);
        Ok(driver.map(|driver| Self { device, name, channel, driver }))
    }

    /// Asks BlueZ to drop the profile connection, which closes its copy of
    /// the socket. Ours alone doesn't keep the channel open.
    async fn disconnect(&self) {
        let _ = time::timeout(DISCONNECT_TIMEOUT, self.device.disconnect_profile(&self.channel)).await;
    }
}

/// The service carrying the protocol of a headset recognized by `service`, if the headset offers it.
fn channel_for(service: &str, offered: &HashSet<Uuid>) -> Option<Uuid> {
    Uuid::parse_str(channel_service(service)).ok().filter(|channel| offered.contains(channel))
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
    let mut profiles = match register(bluez, &headsets).await {
        Ok(profiles) => profiles,
        Err(error) => {
            eprintln!("bluetooth: can't register the control profile with BlueZ: {error}");
            return None;
        }
    };
    for headset in headsets {
        let Some((_, profile)) = profiles.iter_mut().find(|(uuid, _)| *uuid == headset.channel) else {
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
    for device in devices(bluez).await {
        if let Ok(Some(headset)) = Headset::check(device, services, name_filter).await {
            found.push(headset);
        }
    }
    found
}

/// Every device BlueZ knows, on every adapter.
async fn devices(bluez: &bluer::Session) -> Vec<Device> {
    let mut devices = Vec::new();
    for name in bluez.adapter_names().await.unwrap_or_default() {
        let Ok(adapter) = bluez.adapter(&name) else { continue };
        for address in adapter.device_addresses().await.unwrap_or_default() {
            devices.extend(adapter.device(address));
        }
    }
    devices
}

/// Supported headsets that are paired but not connected, by name.
async fn paired(
    bluez: &bluer::Session,
    services: &[(&'static str, Uuid)],
    name_filter: Option<&str>,
) -> Vec<PairedHeadset> {
    let mut headsets = Vec::new();
    for device in devices(bluez).await {
        if let Ok(Some(headset)) = disconnected_headset(&device, services, name_filter).await {
            headsets.push(headset);
        }
    }
    headsets.sort_by(|a, b| a.name.cmp(&b.name));
    headsets
}

/// The device, if it's a paired headset that isn't connected and offers a
/// supported service together with the one carrying its protocol, as
/// [`Headset::check`] requires once it's connected.
async fn disconnected_headset(
    device: &Device,
    services: &[(&'static str, Uuid)],
    name_filter: Option<&str>,
) -> bluer::Result<Option<PairedHeadset>> {
    if !device.is_paired().await? || device.is_connected().await? {
        return Ok(None);
    }
    let name = device.alias().await?;
    if name_filter.is_some_and(|filter| !name.contains(filter)) {
        return Ok(None);
    }
    let uuids = device.uuids().await?.unwrap_or_default();
    let supported =
        services.iter().any(|&(service, uuid)| uuids.contains(&uuid) && channel_for(service, &uuids).is_some());
    Ok(supported.then(|| PairedHeadset { address: device.address().to_string(), name }))
}

/// Connects the paired headset with this address, as the system's Bluetooth
/// settings would.
async fn connect_paired(bluez: &bluer::Session, address: &str) -> Result<(), Box<dyn Error>> {
    let address: bluer::Address = address.parse()?;
    for name in bluez.adapter_names().await? {
        let device = bluez.adapter(&name)?.device(address)?;
        if device.is_paired().await.unwrap_or(false) {
            return Ok(time::timeout(CONNECT_TIMEOUT, device.connect()).await??);
        }
    }
    Err("it isn't paired with this computer".into())
}

/// Registers a client profile for each service the headsets use. BlueZ
/// looks up the RFCOMM channel and hands the socket over through it.
async fn register(bluez: &bluer::Session, headsets: &[Headset]) -> bluer::Result<Vec<(Uuid, ProfileHandle)>> {
    let mut profiles: Vec<(Uuid, ProfileHandle)> = Vec::new();
    for headset in headsets {
        let uuid = headset.channel;
        if profiles.iter().any(|(registered, _)| *registered == uuid) {
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
    let connect = headset.device.connect_profile(&headset.channel);
    let (_, stream) = time::timeout(CONNECT_TIMEOUT, async { tokio::try_join!(connect, accept) }).await??;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use sc_core::drivers::{bose, sony};

    use super::*;

    #[test]
    fn opens_the_service_carrying_the_protocol() {
        let uuid = |service: &str| Uuid::parse_str(service).expect("valid UUID");
        let spp = uuid(bose::CHANNEL_SERVICE);
        let offered = HashSet::from([uuid(sony::SERVICE), uuid(bose::SERVICE), spp]);
        assert_eq!(channel_for(sony::SERVICE, &offered), Some(uuid(sony::SERVICE)));
        assert_eq!(channel_for(bose::SERVICE, &offered), Some(spp));
        // Bose's own channel never carries BMAP, so without Serial Port there's nothing to open.
        assert_eq!(channel_for(bose::SERVICE, &HashSet::from([uuid(bose::SERVICE)])), None);
    }
}
