//! Sound Connect: control Sony and Bose headphones from the menu bar.

mod assets;
mod battery_alerts;
mod calls;
mod control;
mod equalizer;
mod headset;
mod icon;
mod microphone;
mod panel;
mod settings;
mod status;
mod tray;
mod update;

use std::process::ExitCode;
use std::time::Duration;

use gpui_kit::component::Theme;
use gpui_kit::*;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Lets installers check the app starts, without opening any windows.
    if args.iter().any(|arg| arg == "--version") {
        println!("Sound Connect {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if !args.is_empty() {
        return control::run(&args);
    }
    // Already running: that copy opens its panel instead.
    let control::Claim::First(listener) = control::claim() else { return ExitCode::SUCCESS };

    gpui_kit::application().with_assets(assets::Assets).run(|cx| {
        gpui_kit::init(cx);
        Theme::sync_system_appearance(None, cx);
        // Live in the menu bar only: no Dock icon, no app menu.
        cx.set_activation_policy(ActivationPolicy::Accessory);

        let link = sc_bluetooth::start(None);
        let headset = cx.new(|_| headset::Headset::new(link.requests));

        let events = link.events;
        let model = headset.clone();
        cx.spawn(async move |cx| {
            while let Ok(event) = events.recv().await {
                model.update(cx, |headset, cx| {
                    headset.apply(event);
                    cx.notify();
                });
            }
        })
        .detach();

        cx.set_global(settings::Settings::load());
        tray::install(headset.clone(), cx);
        calls::start(headset.clone(), cx);
        battery_alerts::start(headset.clone(), cx);
        if let Some(listener) = listener {
            control::serve(listener, headset.clone(), cx);
        }
        update::start(cx);

        // Development aid: open the panel right away, e.g. to screenshot it.
        if std::env::var_os("SOUND_CONNECT_OPEN_PANEL").is_some() {
            cx.spawn(async move |cx| {
                cx.background_executor().timer(Duration::from_secs(3)).await;
                cx.update(|cx| tray::open_now(&headset, cx));
            })
            .detach();
        }
    });
    ExitCode::SUCCESS
}
