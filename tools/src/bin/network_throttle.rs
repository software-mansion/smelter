//! Interactive bad network simulation for local testing. Applies `tc netem` profiles to UDP
//! traffic on a device, optionally only from/to a single port (e.g. MoQ server port).
//!
//! Requires `tc` (iproute2). Commands run through `sudo` unless started as root. The qdisc is
//! removed on exit.
//!
//! Profile values apply per direction. On `lo` both directions are throttled, so the RTT is
//! twice the configured delay.
//!
//! Profiles without a prefix approximate typical networks (periodic events are more frequent than
//! in reality to make testing faster). `Stress:` profiles are harsher than typical networks.
//!
//! After a profile is applied, its delay, jitter, loss and rate can be adjusted live. Ramp mode
//! degrades one value step by step until any key is pressed, to find a value at which things
//! break.

use std::{
    fmt::{self, Write as _},
    fs,
    io::{self, Write},
    process::Command,
    sync::{
        Mutex,
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread::{self, ScopedJoinHandle},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    queue,
    terminal::{self, ClearType},
};
use inquire::{InquireError, Select, Text};

const REFRESH_INTERVAL: Duration = Duration::from_millis(250);
const RAMP_INTERVAL: Duration = Duration::from_secs(5);
const SUDO_REFRESH_INTERVAL: Duration = Duration::from_secs(60);
const DELAY_STEP_MS: i32 = 1;
const JITTER_STEP_MS: i32 = 1;
const LOSS_STEP_PERCENT: f64 = 0.01;
const MIN_RATE_KBIT: u32 = 10;
const MAX_RATE_KBIT: u32 = 1_000_000;
/// Step multiplier when shift is held.
const BIG_STEP: i32 = 10;

struct Profile {
    name: &'static str,
    netem: Netem,
    outage: Option<Outage>,
}

/// Periodically switches to a degraded netem config.
struct Outage {
    /// Netem options without delay, delay stays the same as outside of the outage.
    netem: &'static str,
    every: Duration,
    duration: Duration,
}

#[derive(Clone, Copy, PartialEq)]
struct Netem {
    delay_ms: u32,
    jitter_ms: u32,
    /// Jitter distribution, uniform if not set.
    distribution: Option<&'static str>,
    loss: Loss,
    /// Unlimited if not set.
    rate_kbit: Option<u32>,
    /// Remaining netem options, passed as is.
    rest: &'static str,
}

#[derive(Clone, Copy, PartialEq)]
enum Loss {
    /// Independent loss, in percent.
    Random(f64),
    /// Gilbert-Elliott model, chances to start and to end a loss burst, in percent.
    Gemodel(f64, f64),
}

const NO_IMPAIRMENT: Netem = Netem {
    delay_ms: 0,
    jitter_ms: 0,
    distribution: None,
    loss: Loss::Random(0.0),
    rate_kbit: None,
    rest: "",
};

const MOBILE: Netem = Netem {
    delay_ms: 35,
    jitter_ms: 10,
    distribution: Some("normal"),
    loss: Loss::Random(0.1),
    rate_kbit: Some(MAX_RATE_KBIT),
    rest: "",
};

// 1 Gbit/s rate does not limit anything, it keeps netem from reordering jittered packets.
const PROFILES: &[Profile] = &[
    Profile {
        name: "No impairment",
        netem: NO_IMPAIRMENT,
        outage: None,
    },
    Profile {
        name: "Wired, same region (RTT ~20ms)",
        netem: Netem {
            delay_ms: 10,
            jitter_ms: 1,
            distribution: Some("normal"),
            loss: Loss::Random(0.01),
            rate_kbit: Some(MAX_RATE_KBIT),
            rest: "",
        },
        outage: None,
    },
    Profile {
        name: "Internet, same continent (RTT ~60ms)",
        netem: Netem {
            delay_ms: 30,
            jitter_ms: 3,
            distribution: Some("normal"),
            loss: Loss::Random(0.05),
            rate_kbit: Some(MAX_RATE_KBIT),
            rest: "",
        },
        outage: None,
    },
    Profile {
        name: "Internet, intercontinental (RTT ~180ms)",
        netem: Netem {
            delay_ms: 90,
            jitter_ms: 5,
            distribution: Some("normal"),
            loss: Loss::Random(0.1),
            rate_kbit: Some(MAX_RATE_KBIT),
            rest: "",
        },
        outage: None,
    },
    Profile {
        name: "Home Wi-Fi",
        netem: Netem {
            delay_ms: 25,
            jitter_ms: 8,
            distribution: Some("normal"),
            loss: Loss::Gemodel(0.1, 30.0),
            rate_kbit: Some(MAX_RATE_KBIT),
            rest: "",
        },
        outage: None,
    },
    Profile {
        name: "Congested Wi-Fi",
        netem: Netem {
            delay_ms: 30,
            jitter_ms: 20,
            distribution: Some("normal"),
            loss: Loss::Gemodel(0.5, 25.0),
            rate_kbit: Some(MAX_RATE_KBIT),
            rest: "",
        },
        outage: None,
    },
    Profile {
        name: "Mobile (LTE)",
        netem: MOBILE,
        outage: None,
    },
    Profile {
        name: "Uplink 3 Mbit/s, 500ms queue",
        netem: Netem {
            delay_ms: 25,
            jitter_ms: 0,
            distribution: None,
            loss: Loss::Random(0.0),
            rate_kbit: Some(3_000),
            rest: "limit 125",
        },
        outage: None,
    },
    Profile {
        name: "Uplink 1 Mbit/s, 500ms queue",
        netem: Netem {
            delay_ms: 25,
            jitter_ms: 0,
            distribution: None,
            loss: Loss::Random(0.0),
            rate_kbit: Some(1_000),
            rest: "limit 40",
        },
        outage: None,
    },
    Profile {
        name: "Mobile handover (50ms outage every 30s)",
        netem: MOBILE,
        outage: Some(Outage {
            netem: "loss 100%",
            every: Duration::from_secs(30),
            duration: Duration::from_millis(50),
        }),
    },
    Profile {
        name: "Mobile bandwidth dip (1 Mbit/s for 5s every 30s)",
        netem: MOBILE,
        outage: Some(Outage {
            netem: "rate 1mbit limit 40",
            every: Duration::from_secs(30),
            duration: Duration::from_secs(5),
        }),
    },
    Profile {
        name: "Mobile coverage gap (3s outage every 2min)",
        netem: MOBILE,
        outage: Some(Outage {
            netem: "loss 100%",
            every: Duration::from_secs(120),
            duration: Duration::from_secs(3),
        }),
    },
    Profile {
        name: "Stress: 1% random loss (RTT 100ms)",
        netem: Netem {
            delay_ms: 50,
            jitter_ms: 0,
            distribution: None,
            loss: Loss::Random(1.0),
            rate_kbit: Some(MAX_RATE_KBIT),
            rest: "",
        },
        outage: None,
    },
    Profile {
        name: "Stress: 5% random loss (RTT 100ms)",
        netem: Netem {
            delay_ms: 50,
            jitter_ms: 0,
            distribution: None,
            loss: Loss::Random(5.0),
            rate_kbit: Some(MAX_RATE_KBIT),
            rest: "",
        },
        outage: None,
    },
    Profile {
        name: "Stress: heavy reordering",
        netem: Netem {
            delay_ms: 50,
            jitter_ms: 20,
            distribution: Some("normal"),
            loss: Loss::Random(0.0),
            rate_kbit: None,
            rest: "",
        },
        outage: None,
    },
    Profile {
        name: "Stress: 1% loss + heavy reordering",
        netem: Netem {
            delay_ms: 100,
            jitter_ms: 50,
            distribution: None,
            loss: Loss::Random(1.0),
            rate_kbit: None,
            rest: "",
        },
        outage: None,
    },
    Profile {
        name: "Stress: batched delivery (20-40ms slots)",
        netem: Netem {
            delay_ms: 30,
            jitter_ms: 0,
            distribution: None,
            loss: Loss::Random(0.0),
            rate_kbit: None,
            rest: "slot 20ms 40ms",
        },
        outage: None,
    },
    Profile {
        name: "Stress: 3s outage every 40s",
        netem: Netem {
            delay_ms: 50,
            jitter_ms: 0,
            distribution: None,
            loss: Loss::Random(0.0),
            rate_kbit: None,
            rest: "",
        },
        outage: Some(Outage {
            netem: "loss 100%",
            every: Duration::from_secs(40),
            duration: Duration::from_secs(3),
        }),
    },
    Profile {
        name: "Stress: 10s outage every 40s",
        netem: Netem {
            delay_ms: 50,
            jitter_ms: 0,
            distribution: None,
            loss: Loss::Random(0.0),
            rate_kbit: None,
            rest: "",
        },
        outage: Some(Outage {
            netem: "loss 100%",
            every: Duration::from_secs(40),
            duration: Duration::from_secs(10),
        }),
    },
];

impl Netem {
    fn step(&mut self, field: Field, steps: i32) {
        match field {
            Field::Delay => {
                self.delay_ms = self.delay_ms.saturating_add_signed(steps * DELAY_STEP_MS)
            }
            Field::Jitter => {
                self.jitter_ms = self.jitter_ms.saturating_add_signed(steps * JITTER_STEP_MS)
            }
            Field::Loss => {
                let (Loss::Random(loss) | Loss::Gemodel(loss, _)) = &mut self.loss;
                // rounding drops float error accumulated over many steps
                let stepped = ((*loss + f64::from(steps) * LOSS_STEP_PERCENT) * 100.0).round();
                *loss = f64::clamp(stepped / 100.0, 0.0, 100.0);
            }
            Field::Rate => {
                let Some(rate) = &mut self.rate_kbit else {
                    if steps < 0 {
                        self.rate_kbit = Some(MAX_RATE_KBIT);
                    }
                    return;
                };
                // keeps two significant digits, so a step is 1-10% of the value
                for _ in 0..steps.unsigned_abs() {
                    let stepped = match steps > 0 {
                        true => *rate + 10u32.pow(rate.ilog10().saturating_sub(1)),
                        false => *rate - 10u32.pow((*rate - 1).ilog10().saturating_sub(1)),
                    };
                    *rate = u32::clamp(stepped, MIN_RATE_KBIT, MAX_RATE_KBIT);
                }
            }
        }
    }

    fn value(&self, field: Field) -> String {
        match field {
            Field::Delay => format!("{}ms", self.delay_ms),
            Field::Jitter => format!("{}ms", self.jitter_ms),
            Field::Loss => match self.loss {
                Loss::Random(loss) => format!("{loss}%"),
                Loss::Gemodel(start, end) => format!("gemodel {start}% {end}%"),
            },
            Field::Rate => match self.rate_kbit {
                Some(rate) => format!("{}mbit", f64::from(rate) / 1000.0),
                None => "unlimited".to_string(),
            },
        }
    }
}

impl fmt::Display for Netem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "delay {}ms", self.delay_ms)?;
        if self.jitter_ms > 0 {
            write!(f, " {}ms", self.jitter_ms)?;
        }
        // tc rejects a distribution unless both delay and jitter are set
        if let Some(distribution) = self.distribution
            && self.delay_ms > 0
            && self.jitter_ms > 0
        {
            write!(f, " distribution {distribution}")?;
        }
        match self.loss {
            Loss::Random(loss) if loss > 0.0 => write!(f, " loss {loss}%")?,
            Loss::Random(_) => {}
            Loss::Gemodel(start, end) => write!(f, " loss gemodel {start}% {end}%")?,
        }
        if let Some(rate) = self.rate_kbit {
            write!(f, " rate {}mbit", f64::from(rate) / 1000.0)?;
        }
        if !self.rest.is_empty() {
            write!(f, " {}", self.rest)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Field {
    Delay,
    Jitter,
    Loss,
    Rate,
}

const FIELDS: [Field; 4] = [Field::Delay, Field::Jitter, Field::Loss, Field::Rate];

impl Field {
    /// Step direction that makes the network worse.
    fn degrade_step(self) -> i32 {
        match self {
            Field::Delay | Field::Jitter | Field::Loss => 1,
            Field::Rate => -1,
        }
    }
}

impl fmt::Display for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(match self {
            Field::Delay => "delay",
            Field::Jitter => "jitter",
            Field::Loss => "loss",
            Field::Rate => "rate",
        })
    }
}

enum Choice {
    Profile(&'static Profile),
    Exit,
}

impl fmt::Display for Choice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let profile = match self {
            Choice::Profile(profile) => profile,
            Choice::Exit => return write!(f, "Exit"),
        };
        write!(f, "{:<50} {}", profile.name, profile.netem)?;
        if let Some(outage) = &profile.outage {
            write!(
                f,
                " | {:?} every {:?}: {}",
                outage.duration, outage.every, outage.netem
            )?;
        }
        Ok(())
    }
}

fn main() -> Result<()> {
    let mut devices: Vec<String> = fs::read_dir("/sys/class/net")
        .context("Failed to list network devices")?
        .filter_map(|entry| Some(entry.ok()?.file_name().to_string_lossy().into_owned()))
        .collect();
    devices.sort();
    let lo_index = devices
        .iter()
        .position(|device| device == "lo")
        .unwrap_or(0);
    let device = Select::new("Network device:", devices)
        .with_starting_cursor(lo_index)
        .with_help_message(
            "Only outgoing traffic is throttled. On lo that covers both directions, on other \
             devices incoming traffic is not affected",
        )
        .prompt()?;
    let port = Text::new("UDP port to throttle (outgoing):")
        .with_help_message("Leave empty to throttle all UDP traffic on the device")
        .prompt()?;
    let port = match port.trim() {
        "" => None,
        port => Some(port.parse::<u16>().context("Invalid port")?),
    };

    let throttle = Throttle::new(device, port)?;
    let throttle = &throttle;
    thread::scope(|scope| -> Result<()> {
        // dropping the sender stops the loop
        let (_sudo_stop_sender, sudo_stop_receiver) = mpsc::channel::<()>();
        scope.spawn(move || throttle.keep_sudo_alive(sudo_stop_receiver));
        let mut outage_loop: Option<(Sender<()>, ScopedJoinHandle<()>)> = None;
        loop {
            let choices = PROFILES
                .iter()
                .map(Choice::Profile)
                .chain([Choice::Exit])
                .collect();
            let profile = match Select::new("Network profile:", choices)
                .with_page_size(PROFILES.len() + 1)
                .prompt()
            {
                Ok(Choice::Profile(profile)) => profile,
                Ok(Choice::Exit)
                | Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
                    return Ok(());
                }
                Err(err) => return Err(err.into()),
            };

            // dropping the sender stops the loop
            if let Some((stop_sender, handle)) = outage_loop.take() {
                drop(stop_sender);
                _ = handle.join();
            }

            throttle.refresh_sudo()?;
            throttle.update(|shaping| {
                *shaping = Shaping {
                    base: profile.netem,
                    outage: None,
                }
            })?;
            outage_loop = profile.outage.as_ref().map(|outage| {
                let (stop_sender, stop_receiver) = mpsc::channel::<()>();
                let handle = scope.spawn(move || run_outage_loop(throttle, outage, stop_receiver));
                (stop_sender, handle)
            });
            match tune(throttle, profile)? {
                TuneExit::Profiles => {}
                TuneExit::Quit => return Ok(()),
            }
        }
    })
}

fn run_outage_loop(throttle: &Throttle, outage: &'static Outage, stop_receiver: Receiver<()>) {
    let wait = |duration| {
        matches!(
            stop_receiver.recv_timeout(duration),
            Err(RecvTimeoutError::Timeout)
        )
    };
    loop {
        if !wait(outage.every.saturating_sub(outage.duration)) {
            return;
        }
        if let Err(err) = throttle.update(|shaping| shaping.outage = Some(outage.netem)) {
            eprintln!("Failed to start outage: {err:#}");
            return;
        }
        if !wait(outage.duration) {
            return;
        }
        if let Err(err) = throttle.update(|shaping| shaping.outage = None) {
            eprintln!("Failed to end outage: {err:#}");
            return;
        }
    }
}

enum TuneExit {
    Profiles,
    Quit,
}

struct Ramp {
    field: Field,
    started: Instant,
    next_step: Instant,
}

/// Live adjustment of delay, jitter, loss and rate on top of the profile, applied on every change.
fn tune(throttle: &Throttle, profile: &Profile) -> Result<TuneExit> {
    let _raw_mode = RawMode::enable()?;
    let mut screen = Screen::default();
    let mut netem = profile.netem;
    let mut selected = 0;
    let mut ramp: Option<Ramp> = None;
    let mut status = String::new();
    loop {
        let mut frame = String::new();
        writeln!(frame, "{}", profile.name)?;
        for (index, field) in FIELDS.into_iter().enumerate() {
            let marker = if index == selected { '>' } else { ' ' };
            let value = netem.value(field);
            let preset = profile.netem.value(field);
            writeln!(frame, "{marker} {field:<7} {value:<22} preset {preset}")?;
        }
        if profile.outage.is_some() {
            let outage = if throttle.outage_active() {
                "active"
            } else {
                "waiting"
            };
            writeln!(frame, "outage: {outage}")?;
        }
        writeln!(frame, "netem: {netem}")?;
        match &ramp {
            Some(ramp) => writeln!(
                frame,
                "Ramping {} every {}s, any key stops ({}s elapsed)",
                ramp.field,
                RAMP_INTERVAL.as_secs(),
                ramp.started.elapsed().as_secs()
            )?,
            None => writeln!(frame, "{status}")?,
        }
        writeln!(
            frame,
            "↑/↓ select  ←/→ adjust  shift ×{BIG_STEP}  a ramp  r reset  p profiles  q quit"
        )?;
        screen.draw(&frame)?;

        let previous = netem;
        if let Some(ramp) = &mut ramp
            && ramp.next_step <= Instant::now()
        {
            netem.step(ramp.field, ramp.field.degrade_step());
            ramp.next_step += RAMP_INTERVAL;
        } else if event::poll(REFRESH_INTERVAL)?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            if let Some(ramp) = ramp.take() {
                status = format!(
                    "Ramp stopped at {} {} after {}s",
                    ramp.field,
                    netem.value(ramp.field),
                    ramp.started.elapsed().as_secs()
                );
                continue;
            }
            let steps = match key.modifiers.contains(KeyModifiers::SHIFT) {
                true => BIG_STEP,
                false => 1,
            };
            match key.code {
                KeyCode::Up => selected = selected.saturating_sub(1),
                KeyCode::Down => selected = usize::min(selected + 1, FIELDS.len() - 1),
                KeyCode::Left => netem.step(FIELDS[selected], -steps),
                KeyCode::Right => netem.step(FIELDS[selected], steps),
                KeyCode::Char('a') => {
                    let now = Instant::now();
                    ramp = Some(Ramp {
                        field: FIELDS[selected],
                        started: now,
                        next_step: now + RAMP_INTERVAL,
                    });
                }
                KeyCode::Char('r') => netem = profile.netem,
                KeyCode::Char('p') => return Ok(TuneExit::Profiles),
                KeyCode::Char('q') => return Ok(TuneExit::Quit),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(TuneExit::Quit);
                }
                _ => {}
            }
        }
        if netem != previous {
            status = match throttle.update(|shaping| shaping.base = netem) {
                Ok(()) => String::new(),
                Err(err) => format!("{err:#}"),
            };
        }
    }
}

struct RawMode;

impl RawMode {
    fn enable() -> Result<Self> {
        terminal::enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        _ = terminal::disable_raw_mode();
    }
}

/// Redraws a frame in place of the previous one.
#[derive(Default)]
struct Screen {
    lines: u16,
}

impl Screen {
    fn draw(&mut self, frame: &str) -> Result<()> {
        let mut stdout = io::stdout();
        if self.lines > 0 {
            queue!(stdout, cursor::MoveUp(self.lines))?;
        }
        queue!(
            stdout,
            cursor::MoveToColumn(0),
            terminal::Clear(ClearType::FromCursorDown)
        )?;
        self.lines = 0;
        // raw mode does not move to the line start on `\n`
        for line in frame.lines() {
            write!(stdout, "{line}\r\n")?;
            self.lines += 1;
        }
        stdout.flush()?;
        Ok(())
    }
}

/// What is applied to the qdisc.
struct Shaping {
    /// Profile netem with live adjustments.
    base: Netem,
    /// Netem options of an ongoing outage.
    outage: Option<&'static str>,
}

impl fmt::Display for Shaping {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.outage {
            None => write!(f, "{}", self.base),
            Some(outage) => write!(f, "delay {}ms {outage}", self.base.delay_ms),
        }
    }
}

/// Netem qdisc on a device, attached to a prio band that only the UDP filters lead to; removed on
/// drop.
struct Throttle {
    device: String,
    use_sudo: bool,
    shaping: Mutex<Shaping>,
}

impl Throttle {
    fn new(device: String, port: Option<u16>) -> Result<Self> {
        let is_root = Command::new("id")
            .arg("-u")
            .output()
            .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).trim() == "0");
        let throttle = Self {
            device,
            use_sudo: !is_root,
            shaping: Mutex::new(Shaping {
                base: NO_IMPAIRMENT,
                outage: None,
            }),
        };
        throttle.refresh_sudo()?;

        // leftovers from a previous run, fails if there is nothing to remove
        _ = throttle.tc(&["qdisc", "del", "dev", &throttle.device, "root"]);

        let device = throttle.device.as_str();
        throttle.tc(&[
            "qdisc", "add", "dev", device, "root", "handle", "1:", "prio", "bands", "4",
        ])?;
        throttle.tc(&[
            "qdisc", "add", "dev", device, "parent", "1:4", "handle", "40:", "netem",
        ])?;

        let add_udp_filter = |protocol: &str, prio: &str, l3: &str, port_match: &[&str]| {
            let args: Vec<&str> = [
                "filter", "add", "dev", device, "parent", "1:", "protocol", protocol, "prio", prio,
                "u32", "match", l3, "protocol", "17", "0xff",
            ]
            .into_iter()
            .chain(port_match.iter().copied())
            .chain(["flowid", "1:4"])
            .collect();
            throttle.tc(&args)
        };
        let port = port.map(|port| port.to_string());
        for (protocol, prio, l3) in [("ip", "1", "ip"), ("ipv6", "2", "ip6")] {
            match &port {
                Some(port) => {
                    for direction in ["sport", "dport"] {
                        add_udp_filter(
                            protocol,
                            prio,
                            l3,
                            &["match", l3, direction, port, "0xffff"],
                        )?;
                    }
                }
                None => add_udp_filter(protocol, prio, l3, &[])?,
            }
        }
        Ok(throttle)
    }

    /// Changes the shaping and applies the result. Lock is held during `tc` call, so concurrent
    /// updates are applied in order.
    fn update(&self, change: impl FnOnce(&mut Shaping)) -> Result<()> {
        let mut shaping = self.shaping.lock().unwrap();
        change(&mut shaping);
        self.set_netem(&shaping.to_string())
    }

    fn outage_active(&self) -> bool {
        self.shaping.lock().unwrap().outage.is_some()
    }

    fn set_netem(&self, netem: &str) -> Result<()> {
        let args: Vec<&str> = [
            "qdisc",
            "replace",
            "dev",
            &self.device,
            "parent",
            "1:4",
            "handle",
            "40:",
            "netem",
        ]
        .into_iter()
        .chain(netem.split_whitespace())
        .collect();
        self.tc(&args)
    }

    /// Asks for the sudo password if needed, so `tc` calls never block on a hidden prompt.
    fn refresh_sudo(&self) -> Result<()> {
        if !self.use_sudo {
            return Ok(());
        }
        let status = Command::new("sudo").arg("-v").status()?;
        if !status.success() {
            bail!("sudo authentication failed");
        }
        Ok(())
    }

    /// Refreshes the sudo credentials until `stop_receiver` disconnects, so they do not expire
    /// while the tool runs.
    fn keep_sudo_alive(&self, stop_receiver: Receiver<()>) {
        if !self.use_sudo {
            return;
        }
        while let Err(RecvTimeoutError::Timeout) = stop_receiver.recv_timeout(SUDO_REFRESH_INTERVAL)
        {
            _ = Command::new("sudo").args(["-n", "-v"]).output();
        }
    }

    fn tc(&self, args: &[&str]) -> Result<()> {
        let mut command = match self.use_sudo {
            true => {
                let mut command = Command::new("sudo");
                command.args(["-n", "tc"]);
                command
            }
            false => Command::new("tc"),
        };
        let output = command.args(args).output().context("Failed to run tc")?;
        if !output.status.success() {
            bail!(
                "tc {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(())
    }
}

impl Drop for Throttle {
    fn drop(&mut self) {
        match self.tc(&["qdisc", "del", "dev", &self.device, "root"]) {
            Ok(()) => println!("Removed throttling from {}", self.device),
            Err(err) => eprintln!("Failed to remove throttling: {err:#}"),
        }
    }
}
