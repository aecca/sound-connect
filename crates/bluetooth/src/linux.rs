//! Linux transport over BlueZ, through BlueR.
//!
//! BlueZ owns SDP and RFCOMM: we register a client profile for Sony's
//! control service, ask BlueZ to connect it, and BlueZ hands the connected
//! socket to our profile. It all runs on a single-threaded Tokio runtime
//! inside the Bluetooth thread.

use std::error::Error;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use bluer::rfcomm::{Profile, ProfileHandle, ReqError, Role, Stream};
use bluer::{Device, Uuid};
use futures::{FutureExt, StreamExt};
use sc_protocol::{Request, Session, Update};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use tokio::time;

use crate::{Event, INITIAL_REQUESTS};

/// Service UUIDs Sony headsets advertise for their control channel, newest first.
const SONY_SERVICES: [Uuid; 2] = [
    // v2: WH-1000XM5 and newer.
    Uuid::from_u128(0x956c7b26_d49a_4ba8_b03f_b17d393cb6e2),
    // v1: older headsets.
    Uuid::from_u128(0x96cc203e_5068_46ad_b32d_e316f5e069ba),
];

/// Wait between attempts to find the headset, growing while it keeps failing.
const RETRY_CONNECT: [Duration; 4] =
    [Duration::from_secs(3), Duration::from_secs(5), Duration::from_secs(10), Duration::from_secs(30)];
const REFRESH_BATTERY_EVERY: Duration = Duration::from_secs(60);
/// How often the session may resend a request the headset didn't ACK.
const TICK: Duration = Duration::from_millis(100);
/// How long BlueZ gets to look up the channel and open it.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const DISCONNECT_TIMEOUT: Duration = Duration::from_secs(5);

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
            Some(bluez) => connect(bluez, name_filter.as_deref()).await,
            None => None,
        };
        match channel {
            Some(mut channel) => {
                searching = false;
                failures = 0;
                let _ = events.send(Event::Connected { name: channel.headset.name.clone() }).await;
                let keep_going = serve(&mut channel.stream, &mut requests, &events).await;
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

        // Wait before looking again. Requests made meanwhile are dropped:
        // the UI refreshes everything once a headset connects.
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
    requests: &mut UnboundedReceiver<Request>,
    events: &async_channel::Sender<Event>,
) -> bool {
    let mut session = Session::default();
    let now = Instant::now();
    let mut frames: Vec<Vec<u8>> = INITIAL_REQUESTS.into_iter().flat_map(|r| session.request(r, now)).collect();
    let mut tick = time::interval(TICK);
    let mut battery = time::interval_at(time::Instant::now() + REFRESH_BATTERY_EVERY, REFRESH_BATTERY_EVERY);
    let mut buffer = [0; 1024];

    loop {
        if write(stream, frames).await.is_err() {
            return true;
        }
        frames = tokio::select! {
            read = stream.read(&mut buffer) => {
                // Zero bytes means the headset hung up.
                let Ok(n @ 1..) = read else { return true };
                let out = session.receive(&buffer[..n], Instant::now());
                for update in out.updates {
                    if update == (Update::Ready { v2: false }) {
                        let _ = events.send(Event::Unsupported(
                            "This headset uses an older protocol that isn't supported yet.".into(),
                        )).await;
                    }
                    let _ = events.send(Event::Update(update)).await;
                }
                out.writes
            }
            request = requests.recv() => match request {
                Some(request) => session.request(request, Instant::now()),
                None => return false,
            },
            _ = tick.tick() => session.tick(Instant::now()),
            _ = battery.tick() => session.request(Request::Battery, Instant::now()),
        };
    }
}

async fn write(stream: &mut Stream, frames: Vec<Vec<u8>>) -> std::io::Result<()> {
    for frame in frames {
        stream.write_all(&frame).await?;
    }
    Ok(())
}

/// A paired, connected device that offers a Sony control service.
struct Headset {
    device: Device,
    name: String,
    service: Uuid,
}

impl Headset {
    async fn check(device: Device, name_filter: Option<&str>) -> bluer::Result<Option<Self>> {
        if !device.is_paired().await? || !device.is_connected().await? {
            return Ok(None);
        }
        let name = device.alias().await?;
        if name_filter.is_some_and(|filter| !name.contains(filter)) {
            return Ok(None);
        }
        let uuids = device.uuids().await?.unwrap_or_default();
        let service = SONY_SERVICES.into_iter().find(|uuid| uuids.contains(uuid));
        Ok(service.map(|service| Self { device, name, service }))
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

/// Finds a connected Sony headset and opens its control channel.
async fn connect(bluez: &bluer::Session, name_filter: Option<&str>) -> Option<Channel> {
    let headsets = headsets(bluez, name_filter).await;
    if headsets.is_empty() {
        return None;
    }
    let mut profiles = match register(bluez, &headsets).await {
        Ok(profiles) => profiles,
        Err(error) => {
            eprintln!("bluetooth: can't register the Sony profile with BlueZ: {error}");
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

async fn headsets(bluez: &bluer::Session, name_filter: Option<&str>) -> Vec<Headset> {
    let mut found = Vec::new();
    for name in bluez.adapter_names().await.unwrap_or_default() {
        let Ok(adapter) = bluez.adapter(&name) else { continue };
        for address in adapter.device_addresses().await.unwrap_or_default() {
            let Ok(device) = adapter.device(address) else { continue };
            if let Ok(Some(headset)) = Headset::check(device, name_filter).await {
                found.push(headset);
            }
        }
    }
    found
}

/// Registers a client profile for each service the headsets use. BlueZ
/// looks up the RFCOMM channel and hands the socket over through it.
async fn register(bluez: &bluer::Session, headsets: &[Headset]) -> bluer::Result<Vec<(Uuid, ProfileHandle)>> {
    let mut profiles = Vec::new();
    for uuid in SONY_SERVICES {
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
