//! macOS transport over IOBluetooth.
//!
//! Connecting and writing happen on the Bluetooth thread, but IOBluetooth
//! delivers channel data through the main thread's run loop, so the delegate
//! only hands data over through thread-safe state. The main run loop must be
//! running: GPUI's is, and the probe example spins it by hand.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::ffi::c_void;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send};
use objc2_foundation::{NSArray, NSDate, NSDefaultRunLoopMode, NSRunLoop};
use objc2_io_bluetooth::{
    IOBluetoothDevice, IOBluetoothRFCOMMChannel, IOBluetoothRFCOMMChannelDelegate, IOBluetoothSDPUUID,
};
use sc_protocol::{Request, Session, Update};

use crate::{Event, INITIAL_REQUESTS};

/// Service UUIDs Sony headsets advertise for their control channel.
const SONY_SERVICES: [[u8; 16]; 2] = [
    // v2: WH-1000XM5 and newer.
    [0x95, 0x6c, 0x7b, 0x26, 0xd4, 0x9a, 0x4b, 0xa8, 0xb0, 0x3f, 0xb1, 0x7d, 0x39, 0x3c, 0xb6, 0xe2],
    // v1: older headsets.
    [0x96, 0xcc, 0x20, 0x3e, 0x50, 0x68, 0x46, 0xad, 0xb3, 0x2d, 0xe3, 0x16, 0xf5, 0xe0, 0x69, 0xba],
];

/// Wait between attempts to find the headset, growing while it keeps failing.
const RETRY_CONNECT: [Duration; 4] =
    [Duration::from_secs(3), Duration::from_secs(5), Duration::from_secs(10), Duration::from_secs(30)];
const REFRESH_BATTERY_EVERY: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(20);

pub fn run(name_filter: Option<String>, requests: mpsc::Receiver<Request>, events: async_channel::Sender<Event>) {
    let delegate = Delegate::new();
    let mut searching = false;
    let mut failures = 0;
    loop {
        match connect(&delegate, name_filter.as_deref()) {
            Some((name, channel)) => {
                searching = false;
                failures = 0;
                let _ = events.send_blocking(Event::Connected { name });
                let keep_going = serve(&delegate, &channel, &requests, &events);
                unsafe { channel.closeChannel() };
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

        // Wait before looking again. Requests made meanwhile are dropped:
        // the UI refreshes everything once a headset connects.
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
    channel: &IOBluetoothRFCOMMChannel,
    requests: &mpsc::Receiver<Request>,
    events: &async_channel::Sender<Event>,
) -> bool {
    let mut session = Session::default();
    let now = Instant::now();
    for request in INITIAL_REQUESTS {
        write(channel, session.request(request, now));
    }
    let mut battery_checked = now;

    loop {
        spin_run_loop(POLL);
        let now = Instant::now();

        let incoming = std::mem::take(&mut *delegate.ivars().incoming.lock().unwrap());
        if !incoming.is_empty() {
            let out = session.receive(&incoming, now);
            write(channel, out.writes);
            for update in out.updates {
                if update == (Update::Ready { v2: false }) {
                    let _ = events.send_blocking(Event::Unsupported(
                        "This headset uses an older protocol that isn't supported yet.".into(),
                    ));
                }
                let _ = events.send_blocking(Event::Update(update));
            }
        }
        if delegate.ivars().closed.swap(false, Ordering::SeqCst) {
            return true;
        }

        match requests.recv_timeout(POLL) {
            Ok(request) => write(channel, session.request(request, now)),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return false,
        }
        write(channel, session.tick(now));

        if now.duration_since(battery_checked) >= REFRESH_BATTERY_EVERY {
            battery_checked = now;
            write(channel, session.request(Request::Battery, now));
        }
    }
}

fn write(channel: &IOBluetoothRFCOMMChannel, frames: Vec<Vec<u8>>) {
    for mut frame in frames {
        // Frames are tiny; the u16 length can't overflow.
        unsafe { channel.writeSync_length(frame.as_mut_ptr().cast(), frame.len() as u16) };
    }
}

fn spin_run_loop(limit: Duration) {
    let until = NSDate::dateWithTimeIntervalSinceNow(limit.as_secs_f64());
    unsafe { NSRunLoop::currentRunLoop().runMode_beforeDate(NSDefaultRunLoopMode, &until) };
}

/// Finds a connected Sony headset and opens its control channel.
fn connect(delegate: &Delegate, name_filter: Option<&str>) -> Option<(String, Retained<IOBluetoothRFCOMMChannel>)> {
    let uuids: Vec<Retained<IOBluetoothSDPUUID>> = SONY_SERVICES
        .iter()
        .filter_map(|bytes| unsafe { IOBluetoothSDPUUID::uuidWithBytes_length(bytes.as_ptr().cast(), 16) })
        .collect();

    for device in connected_devices() {
        let name = unsafe { device.name() }.to_string();
        if name_filter.is_some_and(|filter| !name.contains(filter)) {
            continue;
        }

        let mut record = uuids.iter().find_map(|uuid| unsafe { device.getServiceRecordForUUID(Some(uuid)) });
        if record.is_none() && looks_like_sony(&name) {
            // Service records aren't cached yet: ask the headset.
            delegate.ivars().sdp_done.store(false, Ordering::SeqCst);
            let any_uuids: Vec<Retained<AnyObject>> =
                uuids.iter().map(|u| unsafe { Retained::cast_unchecked(u.clone()) }).collect();
            let array = NSArray::from_retained_slice(&any_uuids);
            unsafe { device.performSDPQuery_uuids(Some(delegate), Some(&array)) };
            let deadline = Instant::now() + Duration::from_secs(5);
            while !delegate.ivars().sdp_done.load(Ordering::SeqCst) && Instant::now() < deadline {
                spin_run_loop(POLL);
            }
            record = uuids.iter().find_map(|uuid| unsafe { device.getServiceRecordForUUID(Some(uuid)) });
        }
        let Some(record) = record else { continue };

        let mut channel_id = 0;
        if unsafe { record.getRFCOMMChannelID(&mut channel_id) } != 0 {
            continue;
        }

        delegate.ivars().closed.store(false, Ordering::SeqCst);
        delegate.ivars().incoming.lock().unwrap().clear();
        let mut channel = None;
        let status =
            unsafe { device.openRFCOMMChannelSync_withChannelID_delegate(Some(&mut channel), channel_id, Some(delegate)) };
        match channel {
            Some(channel) if status == 0 => return Some((name, channel)),
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

fn looks_like_sony(name: &str) -> bool {
    ["WH-", "WF-", "WI-", "MDR-", "LinkBuds", "ULT"].iter().any(|prefix| name.starts_with(prefix))
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
