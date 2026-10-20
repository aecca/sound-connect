//! "Ambient Sound During Calls": while an app records from a microphone, the
//! headset switches to ambient sound so you hear yourself, and back once the
//! call is over. [`CallAmbience`] decides; this feeds it what PipeWire says.

use std::time::Instant;

use gpui_kit::*;
use sc_core::{CallAmbience, ListeningMode, Setting};

use crate::headset::Headset;
use crate::microphone;
use crate::settings::Settings;

/// Whether this computer can tell when the microphone is in use: once
/// PipeWire has said what's recording.
pub struct Calls {
    pub supported: bool,
}

impl Global for Calls {}

pub fn start(headset: Entity<Headset>, cx: &mut App) {
    cx.set_global(Calls { supported: false });
    let Some(changes) = microphone::watch() else { return };

    cx.spawn(async move |cx| {
        let mut calls = CallAmbience::default();
        let mut recording = false;
        loop {
            if recording || !calls.in_call() {
                let Ok(change) = changes.recv().await else { return };
                recording = change;
                if !cx.update(|cx| cx.global::<Calls>().supported) {
                    cx.update(|cx| cx.set_global(Calls { supported: true }));
                }
            } else {
                // The microphone went unused during a call: look again once
                // it's had time to settle.
                cx.background_executor().timer(CallAmbience::SETTLE).await;
                if let Ok(change) = changes.try_recv() {
                    recording = change;
                }
            }
            cx.update(|cx| {
                if !cx.global::<Settings>().ambient_during_calls {
                    calls = CallAmbience::default();
                    return;
                }
                headset.update(cx, |headset, cx| {
                    let mode = headset.mode.filter(|_| headset.offers(ListeningMode::Ambient));
                    if let Some(mode) = calls.update(recording, mode, Instant::now()) {
                        headset.change(Setting::Mode(mode), cx);
                    }
                });
            });
        }
    })
    .detach();
}
