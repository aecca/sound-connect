//! Lets other programs control the running app. The `sound-connect` command
//! talks to it over a Unix socket in `$XDG_RUNTIME_DIR`: a keyboard shortcut
//! can run `sound-connect mode next`, a status bar `sound-connect status
//! --json --follow`, and running `sound-connect` again opens the panel of the
//! app that's already running instead of starting another.
//!
//! A client sends one line, the command, and gets back lines starting with
//! `ok ` or `error `: one, or one per change while it follows the status.

use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use gpui_kit::*;
use sc_core::{Device, ListeningMode, Mode, Setting};

use crate::headset::{Connection, Headset};
use crate::{status, tray};

/// How long the command waits for the app to answer.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(5);

const USAGE: &str = "\
Usage: sound-connect [command]

With no command, opens Sound Connect, or its panel when it's already open.

Commands:
  mode next                Switch to the next listening mode
  mode <mode>              Switch to off, ambient, noise-cancelling,
                           noise-cancelling-low or wind-reduction, when the
                           headphones have it
  status                   Show the headphones' mode and battery
  status --json            The same as JSON, for scripts and status bars
  status [--json] --follow Show it again whenever it changes
  --version                Show the version";

/// Runs the `sound-connect` command with these arguments.
pub fn run(args: &[String]) -> ExitCode {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let command = match args[..] {
        ["mode", mode] => Some(format!("mode {mode}")),
        ["status", ref flags @ ..] => status_command(flags),
        ["help" | "--help" | "-h"] => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        _ => None,
    };
    let Some(command) = command else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };

    let mut code = ExitCode::SUCCESS;
    let sent = send(&command, |answer| match answer {
        Ok(text) => writeln!(io::stdout(), "{text}").is_ok(),
        Err(text) => {
            eprintln!("{text}");
            code = ExitCode::FAILURE;
            true
        }
    });
    match sent {
        Ok(()) => code,
        Err(_) => {
            eprintln!("Sound Connect isn't running.");
            ExitCode::FAILURE
        }
    }
}

/// `status` with `--json` and `--follow`, in any order, as the app reads it.
fn status_command(flags: &[&str]) -> Option<String> {
    let (json, follow) = (flags.contains(&"--json"), flags.contains(&"--follow"));
    let known = usize::from(json) + usize::from(follow) == flags.len();
    known.then(|| ["status", if json { " json" } else { "" }, if follow { " follow" } else { "" }].concat())
}

/// Whether this is the only copy of the app.
pub enum Claim {
    /// It is, and answers commands here (nowhere without `$XDG_RUNTIME_DIR`).
    First(Option<UnixListener>),
    /// Another copy is running, and has been asked to show its panel.
    Taken,
}

/// Claims the socket before the app starts, so two copies started at once
/// don't both run.
pub fn claim() -> Claim {
    let Some(path) = socket_path() else {
        eprintln!("not listening for commands: XDG_RUNTIME_DIR isn't set");
        return Claim::First(None);
    };
    let bound = UnixListener::bind(&path).or_else(|error| {
        if error.kind() != io::ErrorKind::AddrInUse {
            return Err(error);
        }
        match UnixStream::connect(&path) {
            Ok(mut running) => {
                let _ = writeln!(running, "show");
                Err(io::ErrorKind::AddrInUse.into())
            }
            // Left behind by a copy that has quit.
            Err(_) => std::fs::remove_file(&path).and_then(|()| UnixListener::bind(&path)),
        }
    });
    match bound {
        Ok(listener) => Claim::First(Some(listener)),
        Err(error) if error.kind() == io::ErrorKind::AddrInUse => Claim::Taken,
        Err(error) => {
            eprintln!("not listening for commands on {}: {error}", path.display());
            Claim::First(None)
        }
    }
}

/// Sends a command to the running app and passes on its answers until it's
/// done, or until `answer` returns false.
fn send(command: &str, mut answer: impl FnMut(Result<String, String>) -> bool) -> io::Result<()> {
    let mut stream = UnixStream::connect(socket_path().ok_or(io::ErrorKind::NotFound)?)?;
    // Following waits for changes, as long as they take.
    let follows = command.ends_with(" follow");
    stream.set_read_timeout((!follows).then_some(ANSWER_TIMEOUT))?;
    writeln!(stream, "{command}")?;
    for line in BufReader::new(stream).lines() {
        let line = line?;
        let parsed = match line.split_once(' ') {
            Some(("ok", text)) => Ok(text.to_owned()),
            Some((_, text)) => Err(text.to_owned()),
            None if line == "ok" => Ok(String::new()),
            None => Err(line),
        };
        if !answer(parsed) {
            break;
        }
    }
    Ok(())
}

fn socket_path() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|dir| PathBuf::from(dir).join("sound-connect.sock"))
}

type Answers = async_channel::Sender<Result<String, String>>;

/// A command from a client, and where its answers go.
struct Request {
    command: String,
    answers: Answers,
}

/// Clients following the status, and what each was last told.
struct Followers(Vec<Follower>);

impl Global for Followers {}

struct Follower {
    json: bool,
    answers: Answers,
    last: String,
}

/// Starts answering commands on the socket [`claim`] got.
pub fn serve(listener: UnixListener, headset: Entity<Headset>, cx: &mut App) {
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

    cx.set_global(Followers(Vec::new()));
    cx.observe(&headset, |headset, cx| tell_followers(&headset, cx)).detach();
    cx.spawn(async move |cx| {
        while let Ok(request) = received.recv().await {
            cx.update(|cx| handle(request, &headset, cx));
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
    let (answers, answered) = async_channel::unbounded();
    let request = Request { command: command.trim().to_owned(), answers };
    if requests.send_blocking(request).is_err() {
        return;
    }
    while let Ok(answer) = answered.recv_blocking() {
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

fn handle(request: Request, headset: &Entity<Headset>, cx: &mut App) {
    let words: Vec<&str> = request.command.split(' ').collect();
    let answer = match words[..] {
        ["show"] => {
            tray::show(headset, cx);
            Ok(String::new())
        }
        ["mode", mode] => headset.update(cx, |headset, cx| switch_mode(headset, mode, cx)),
        ["status", ref options @ ..] => {
            let json = options.contains(&"json");
            let line = if json { status::json(headset.read(cx)) } else { status::text(headset.read(cx)) };
            if options.contains(&"follow") {
                let follower = Follower { json, answers: request.answers.clone(), last: line.clone() };
                cx.global_mut::<Followers>().0.push(follower);
            }
            Ok(line)
        }
        _ => Err(format!("Unknown command \"{}\". Run sound-connect --help to see them all.", request.command)),
    };
    let _ = request.answers.try_send(answer);
}

/// Tells each follower the status, if it changed since they last heard.
fn tell_followers(headset: &Entity<Headset>, cx: &mut App) {
    let (json, text) = (status::json(headset.read(cx)), status::text(headset.read(cx)));
    cx.global_mut::<Followers>().0.retain_mut(|follower| {
        let line = if follower.json { &json } else { &text };
        if *line == follower.last {
            return !follower.answers.is_closed();
        }
        follower.last.clone_from(line);
        follower.answers.try_send(Ok(line.clone())).is_ok()
    });
}

/// Switches to `mode`, a mode's ID or "next", and says which mode that is.
fn switch_mode(headset: &mut Headset, mode: &str, cx: &mut Context<Headset>) -> Result<String, String> {
    let device = headset
        .device
        .as_ref()
        .filter(|_| headset.connection == Connection::Connected)
        .ok_or("No headphones are connected.")?;
    let target = mode_for(device, headset.mode, mode)?;
    let name = target.name.to_owned();
    headset.change(Setting::Mode(target.mode), cx);
    Ok(name)
}

/// The mode a `mode` command asks for: one of the device's by ID, or the
/// one after `current`.
fn mode_for<'a>(device: &'a Device, current: Option<ListeningMode>, asked: &str) -> Result<&'a Mode, String> {
    if device.modes.is_empty() {
        return Err("These headphones' modes aren't known yet.".into());
    }
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
    use super::{mode_for, status_command};

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

    #[test]
    fn reads_the_status_flags_in_any_order() {
        assert_eq!(status_command(&[]).as_deref(), Some("status"));
        assert_eq!(status_command(&["--follow", "--json"]).as_deref(), Some("status json follow"));
        assert_eq!(status_command(&["--follow"]).as_deref(), Some("status follow"));
        assert_eq!(status_command(&["--json", "--json"]), None);
        assert_eq!(status_command(&["--color"]), None);
    }
}
