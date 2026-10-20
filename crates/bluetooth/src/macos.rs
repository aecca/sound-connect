//! macOS transport over IOBluetooth.
//!
//! Connecting and writing happen on the Bluetooth thread, but IOBluetooth
//! delivers channel data through the main thread's run loop, so the delegate
//! only hands data over through thread-safe state. The main run loop must be
//! running: GPUI's is, and the probe example spins it by hand.

use std::ffi::c_void;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send};
use objc2_foundation::{NSArray, NSDate, NSDefaultRunLoopMode, NSRunLoop};
use objc2_io_bluetooth::{
    IOBluetoothDevice, IOBluetoothRFCOMMChannel, IOBluetoothRFCOMMChannelDelegate, IOBluetoothSDPUUID,
    kBluetoothDeviceClassMajorAudio,
};
use sc_core::{Driver, Frame, Output, SERVICES, channel_service, driver_for};

use crate::{Event, Request};

/// Wait between attempts to find the headset, growing while it keeps failing.
const RETRY_CONNECT: [Duration; 4] =
    [Duration::from_secs(3), Duration::from_secs(5), Duration::from_secs(10), Duration::from_secs(30)];
const REFRESH_EVERY: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(20);
const SDP_TIMEOUT: Duration = Duration::from_secs(5);

/// An open control channel and the driver that speaks over it.
struct Connection {
    name: String,
    channel: Retained<IOBluetoothRFCOMMChannel>,
    driver: Box<dyn Driver>,
}

/// Drives the panel when it runs on a Mac during development; the native Mac
/// app has its own link, which also connects paired headsets, so
/// [`Request::Connect`] is ignored here.
pub fn run(name_filter: Option<String>, requests: mpsc::Receiver<Request>, events: async_channel::Sender<Event>) {
    let delegate = Delegate::new();
    let services = services();
    let mut searching = false;
    let mut failures = 0;
    loop {
        match connect(&delegate, &services, name_filter.as_deref()) {
            Some(mut connection) => {
                searching = false;
                failures = 0;
                let _ = events.send_blocking(Event::Connected { name: connection.name.clone() });
                let keep_going = serve(&delegate, &mut connection, &requests, &events);
                unsafe { connection.channel.closeChannel() };
                let _ = events.send_blocking(Event::Disconnected);
                if !keep_going {
                    return;
                }
            }
            None => {
                failures += 1;
                if !searching {
                    searching = true;
                    let _ = events.send_blocking(Event::Searching);
                }
            }
        }

        // Wait before looking again. Settings changed meanwhile are dropped:
        // the UI reads everything again once a headset connects.
        let deadline = Instant::now() + RETRY_CONNECT[failures.min(RETRY_CONNECT.len() - 1)];
        while Instant::now() < deadline {
            match requests.recv_timeout(deadline - Instant::now()) {
                Ok(_) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        if events.is_closed() {
            return;
        }
    }
}

/// Talks to the headset until the channel closes. Returns false when the app
/// has gone away and the thread should stop.
fn serve(
    delegate: &Delegate,
    connection: &mut Connection,
    requests: &mpsc::Receiver<Request>,
    events: &async_channel::Sender<Event>,
) -> bool {
    let Connection { channel, driver, .. } = connection;
    deliver(channel, events, driver.start(Instant::now()));
    let mut refreshed = Instant::now();

    loop {
        spin_run_loop(POLL);
        let now = Instant::now();

        let incoming = std::mem::take(&mut *delegate.ivars().incoming.lock().unwrap());
        if !incoming.is_empty() {
            deliver(channel, events, driver.receive(&incoming, now));
        }
        if delegate.ivars().closed.swap(false, Ordering::SeqCst) {
            return true;
        }

        match requests.recv_timeout(POLL) {
            Ok(Request::Change(setting)) => write(channel, driver.set(setting, now)),
            Ok(Request::Connect(_)) => {}
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return false,
        }
        write(channel, driver.tick(now));

        if now.duration_since(refreshed) >= REFRESH_EVERY {
            refreshed = now;
            write(channel, driver.refresh(now));
        }
    }
}

fn deliver(channel: &IOBluetoothRFCOMMChannel, events: &async_channel::Sender<Event>, output: Output) {
    write(channel, output.writes);
    for update in output.updates {
        let _ = events.send_blocking(Event::Update(update));
    }
}

fn write(channel: &IOBluetoothRFCOMMChannel, frames: Vec<Frame>) {
    for mut frame in frames {
        let length = u16::try_from(frame.len()).expect("control frames are a few bytes long");
        unsafe { channel.writeSync_length(frame.as_mut_ptr().cast(), length) };
    }
}

fn spin_run_loop(limit: Duration) {
    let until = NSDate::dateWithTimeIntervalSinceNow(limit.as_secs_f64());
    unsafe { NSRunLoop::currentRunLoop().runMode_beforeDate(NSDefaultRunLoopMode, &until) };
}

/// A supported control service and the service whose channel carries its protocol.
struct Service {
    name: &'static str,
    uuid: Retained<IOBluetoothSDPUUID>,
    /// `None` when the protocol is on this service's own channel.
    channel: Option<Retained<IOBluetoothSDPUUID>>,
}

impl Service {
    fn channel(&self) -> &IOBluetoothSDPUUID {
        self.channel.as_deref().unwrap_or(&self.uuid)
    }
}

/// The supported control services, as IOBluetooth wants them.
fn services() -> Vec<Service> {
    SERVICES
        .iter()
        .filter_map(|&name| {
            let channel = match channel_service(name) {
                same if same == name => None,
                other => Some(sdp_uuid(other)?),
            };
            Some(Service { name, uuid: sdp_uuid(name)?, channel })
        })
        .collect()
}

fn sdp_uuid(uuid: &str) -> Option<Retained<IOBluetoothSDPUUID>> {
    let bytes = uuid_bytes(uuid)?;
    unsafe { IOBluetoothSDPUUID::uuidWithBytes_length(bytes.as_ptr().cast(), 16) }
}

/// Finds a connected headset offering a supported service and opens its
/// control channel.
fn connect(delegate: &Delegate, services: &[Service], name_filter: Option<&str>) -> Option<Connection> {
    for device in connected_devices() {
        let name = unsafe { device.name() }.to_string();
        if name_filter.is_some_and(|filter| !name.contains(filter)) {
            continue;
        }

        let find_record = || {
            services.iter().find_map(|service| unsafe {
                device.getServiceRecordForUUID(Some(&service.uuid))?;
                // Some brands carry their protocol on another service's channel, like Bose on Serial Port.
                device.getServiceRecordForUUID(Some(service.channel())).map(|record| (service, record))
            })
        };
        let mut found = find_record();
        if found.is_none() && unsafe { device.deviceClassMajor() } == kBluetoothDeviceClassMajorAudio {
            // Service records aren't cached yet: ask the headset.
            query_services(delegate, &device, services);
            found = find_record();
        }
        let Some((service, record)) = found else { continue };
        let Some(driver) = driver_for(&name, service.name) else { continue };

        let mut channel_id = 0;
        if unsafe { record.getRFCOMMChannelID(&mut channel_id) } != 0 {
            continue;
        }

        delegate.ivars().closed.store(false, Ordering::SeqCst);
        delegate.ivars().incoming.lock().unwrap().clear();
        let mut channel = None;
        let status = unsafe {
            device.openRFCOMMChannelSync_withChannelID_delegate(Some(&mut channel), channel_id, Some(delegate))
        };
        match channel {
            Some(channel) if status == 0 => return Some(Connection { name, channel, driver }),
            // IOBluetooth can hand back a channel even when opening fails;
            // close it or every failed attempt leaks a file descriptor.
            Some(channel) => unsafe {
                channel.closeChannel();
            },
            None => {}
        }
    }
    None
}

fn query_services(delegate: &Delegate, device: &IOBluetoothDevice, services: &[Service]) {
    delegate.ivars().sdp_done.store(false, Ordering::SeqCst);
    let uuids: Vec<Retained<AnyObject>> = services
        .iter()
        .flat_map(|service| [Some(&service.uuid), service.channel.as_ref()])
        .flatten()
        .map(|uuid| unsafe { Retained::cast_unchecked(uuid.clone()) })
        .collect();
    let array = NSArray::from_retained_slice(&uuids);
    unsafe { device.performSDPQuery_uuids(Some(delegate), Some(&array)) };
    let deadline = Instant::now() + SDP_TIMEOUT;
    while !delegate.ivars().sdp_done.load(Ordering::SeqCst) && Instant::now() < deadline {
        spin_run_loop(POLL);
    }
}

fn connected_devices() -> Vec<Retained<IOBluetoothDevice>> {
    let Some(paired) = (unsafe { IOBluetoothDevice::pairedDevices() }) else {
        return Vec::new();
    };
    paired
        .to_vec()
        .into_iter()
        .filter_map(|object| object.downcast::<IOBluetoothDevice>().ok())
        .filter(|device| unsafe { device.isConnected() })
        .collect()
}

/// A UUID such as "956C7B26-D49A-4BA8-B03F-B17D393CB6E2" as its 16 bytes.
fn uuid_bytes(uuid: &str) -> Option<[u8; 16]> {
    let digits: Vec<u8> = uuid.bytes().filter(|&byte| byte != b'-').collect();
    if digits.len() != 32 {
        return None;
    }
    let mut bytes = [0; 16];
    for (byte, pair) in bytes.iter_mut().zip(digits.chunks(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(bytes)
}

#[derive(Default)]
struct Ivars {
    incoming: Mutex<Vec<u8>>,
    closed: AtomicBool,
    sdp_done: AtomicBool,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "SoundConnectChannelDelegate"]
    #[ivars = Ivars]
    struct Delegate;

    impl Delegate {
        #[unsafe(method(sdpQueryComplete:status:))]
        fn sdp_query_complete(&self, _device: *mut AnyObject, _status: i32) {
            self.ivars().sdp_done.store(true, Ordering::SeqCst);
        }
    }

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl IOBluetoothRFCOMMChannelDelegate for Delegate {
        #[unsafe(method(rfcommChannelData:data:length:))]
        unsafe fn rfcomm_channel_data(
            &self,
            _channel: Option<&IOBluetoothRFCOMMChannel>,
            data: *mut c_void,
            length: usize,
        ) {
            let bytes = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), length) };
            self.ivars().incoming.lock().unwrap().extend_from_slice(bytes);
        }

        #[unsafe(method(rfcommChannelClosed:))]
        unsafe fn rfcomm_channel_closed(&self, _channel: Option<&IOBluetoothRFCOMMChannel>) {
            self.ivars().closed.store(true, Ordering::SeqCst);
        }
    }
);

impl Delegate {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(Ivars::default());
        unsafe { msg_send![super(this), init] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_service_uuids() {
        let bytes = uuid_bytes("956C7B26-D49A-4BA8-B03F-B17D393CB6E2").expect("valid UUID");
        assert_eq!(bytes[..4], [0x95, 0x6c, 0x7b, 0x26]);
        assert_eq!(bytes[12..], [0x39, 0x3c, 0xb6, 0xe2]);
        assert!(uuid_bytes("not-a-uuid").is_none());
        assert!(SERVICES.iter().all(|service| uuid_bytes(service).is_some()));
        assert!(SERVICES.iter().all(|service| uuid_bytes(channel_service(service)).is_some()));
    }
}
