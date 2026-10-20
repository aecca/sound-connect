//! Lets other programs control the running app. The `sound-connect` command
//! talks to it over a Unix socket in `$XDG_RUNTIME_DIR`: a keyboard shortcut
//! can run `sound-connect mode next`, and running `sound-connect` again opens
//! the panel of the app that's already running instead of starting another.
//!
//! A client sends one line, the command, and gets back lines starting with
//! `ok ` or `error `.

use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use gpui_kit::*;
use sc_core::{Device, ListeningMode, Mode, Setting};

use crate::headset::Headset;
use crate::tray;

/// How long the command waits for the app to answer.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(5);

const USAGE: &str = "\
Usage: sound-connect [command]

With no command, opens Sound Connect, or its panel when it's already open.

Commands:
  mode next     Switch to the next listening mode
  mode <mode>   Switch to off, ambient, noise-cancelling, noise-cancelling-low
                or wind-reduction, when the headphones have it
  --version     Show the version";

/// Runs the `sound-connect` command with these arguments.
pub fn run(args: &[String]) -> ExitCode {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let command = match args[..] {
        ["mode", mode] => format!("mode {mode}"),
        ["help" | "--help" | "-h"] => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match send(&command) {
        Ok(answers) => {
            let mut code = ExitCode::SUCCESS;
            for answer in answers {
                match answer {
                    Ok(text) => println!("{text}"),
                    Err(text) => {
                        eprintln!("{text}");
                        code = ExitCode::FAILURE;
                    }
                }
            }
            code
        }
        Err(_) => {
            eprintln!("Sound Connect isn't running.");
            ExitCode::FAILURE
        }
    }
}

/// Asks the running app to open its panel. Fails when none is running.
pub fn show_running() -> io::Result<()> {
    send("show").map(|_| ())
}

/// Sends a command to the running app and collects its answers.
fn send(command: &str) -> io::Result<Vec<Result<String, String>>> {
    let mut stream = UnixStream::connect(socket_path().ok_or(io::ErrorKind::NotFound)?)?;
    stream.set_read_timeout(Some(ANSWER_TIMEOUT))?;
    writeln!(stream, "{command}")?;
    let mut answers = Vec::new();
    for line in BufReader::new(stream).lines() {
        let line = line?;
        answers.push(match line.split_once(' ') {
            Some(("ok", text)) => Ok(text.to_owned()),
            Some((_, text)) => Err(text.to_owned()),
            None if line == "ok" => Ok(String::new()),
            None => Err(line),
        });
    }
    Ok(answers)
}

fn socket_path() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|dir| PathBuf::from(dir).join("sound-connect.sock"))
}

/// A command from a client, and where its answer goes.
struct Request {
    command: String,
    answer: async_channel::Sender<Result<String, String>>,
}

/// Starts answering commands. Without `$XDG_RUNTIME_DIR` there's nowhere
/// private to listen, so other programs can't reach the app.
pub fn serve(headset: Entity<Headset>, cx: &mut App) {
    let Some(path) = socket_path() else {
        return eprintln!("not listening for commands: XDG_RUNTIME_DIR isn't set");
    };
    // Whoever made this socket is gone: show_running() found nobody there.
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(error) => return eprintln!("not listening for commands on {}: {error}", path.display()),
    };

    let (requests, received) = async_channel::unbounded();
    thread::Builder::new()
        .name("control".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let requests = requests.clone();
                // One thread per client, so a slow one doesn't hold up the rest.
                let _ =
                    thread::Builder::new().name("control-client".into()).spawn(move || serve_client(stream, &requests));
            }
        })
        .expect("failed to start the control thread");

    cx.spawn(async move |cx| {
        while let Ok(request) = received.recv().await {
            let answer = cx.update(|cx| answer(&request.command, &headset, cx));
            let _ = request.answer.send(answer).await;
        }
    })
    .detach();
}

fn serve_client(mut stream: UnixStream, requests: &async_channel::Sender<Request>) {
    let mut command = String::new();
    // A client that never says anything shouldn't keep its thread forever.
    let read =
        stream.set_read_timeout(Some(ANSWER_TIMEOUT)).and_then(|()| BufReader::new(&stream).read_line(&mut command));
    if read.is_err() {
        return;
    }
    let (answer, answers) = async_channel::bounded(1);
    let request = Request { command: command.trim().to_owned(), answer };
    if requests.send_blocking(request).is_err() {
        return;
    }
    while let Ok(answer) = answers.recv_blocking() {
        let line = match answer {
            Ok(text) if text.is_empty() => "ok".to_owned(),
            Ok(text) => format!("ok {text}"),
            Err(text) => format!("error {text}"),
        };
        if writeln!(stream, "{line}").is_err() {
            return;
        }
    }
}

fn answer(command: &str, headset: &Entity<Headset>, cx: &mut App) -> Result<String, String> {
    match command.split_once(' ') {
        None if command == "show" => {
            tray::show(headset, cx);
            Ok(String::new())
        }
        Some(("mode", mode)) => headset.update(cx, |headset, cx| switch_mode(headset, mode, cx)),
        _ => Err(format!("Unknown command \"{command}\". Run sound-connect --help to see them all.")),
    }
}

/// Switches to `mode`, a mode's ID or "next", and says which mode that is.
fn switch_mode(headset: &mut Headset, mode: &str, cx: &mut Context<Headset>) -> Result<String, String> {
    let device = headset.device.as_ref().ok_or("No headphones are connected.")?;
    let target = mode_for(device, headset.mode, mode)?;
    let name = target.name.to_owned();
    headset.change(Setting::Mode(target.mode), cx);
    Ok(name)
}

/// The mode a `mode` command asks for: one of the device's by ID, or the
/// one after `current`.
fn mode_for<'a>(device: &'a Device, current: Option<ListeningMode>, asked: &str) -> Result<&'a Mode, String> {
    let target = match asked {
        "next" => device.mode_after(current),
        id => ListeningMode::from_id(id),
    };
    device.modes.iter().find(|offered| Some(offered.mode) == target).ok_or_else(|| {
        let ids: Vec<_> = device.modes.iter().map(|offered| offered.mode.id()).collect();
        format!("These headphones have no mode \"{asked}\". Try next, {}.", ids.join(", "))
    })
}

#[cfg(test)]
mod tests {
    use sc_core::{Brand, Device, ListeningMode::*, Mode};

    // Not a glob import: GPUI's own `test` attribute would shadow the built-in one.
    use super::mode_for;

    fn bose() -> Device {
        let mut device = Device::unidentified(Brand::Bose, "QuietComfort 35 II");
        device.modes = vec![
            Mode { mode: Off, name: "Off" },
            Mode { mode: NoiseCancellingLow, name: "Low" },
            Mode { mode: NoiseCancelling, name: "High" },
        ];
        device
    }

    #[test]
    fn picks_the_mode_asked_for() {
        let device = bose();
        assert_eq!(mode_for(&device, Some(Off), "noise-cancelling").map(|mode| mode.name), Ok("High"));
        assert_eq!(mode_for(&device, Some(NoiseCancelling), "next").map(|mode| mode.name), Ok("Off"));
        assert_eq!(mode_for(&device, None, "next").map(|mode| mode.name), Ok("Off"));
    }

    #[test]
    fn lists_the_modes_there_are_when_asked_for_another() {
        let error = mode_for(&bose(), Some(Off), "ambient").unwrap_err();
        assert_eq!(
            error,
            "These headphones have no mode \"ambient\". Try next, off, noise-cancelling-low, noise-cancelling."
        );
    }
}
