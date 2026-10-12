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

use std::{
    fmt, fs,
    process::Command,
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread::{self, ScopedJoinHandle},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use inquire::{InquireError, Select, Text};

struct Profile {
    name: &'static str,
    netem: &'static str,
    outage: Option<Outage>,
}

/// Periodically switches to a degraded netem config.
struct Outage {
    netem: &'static str,
    every: Duration,
    duration: Duration,
}

// `rate 1gbit` does not limit anything, it keeps netem from reordering jittered packets.
const PROFILES: &[Profile] = &[
    Profile {
        name: "No impairment",
        netem: "",
        outage: None,
    },
    Profile {
        name: "Wired, same region (RTT ~20ms)",
        netem: "delay 10ms 1ms distribution normal rate 1gbit loss 0.01%",
        outage: None,
    },
    Profile {
        name: "Internet, same continent (RTT ~60ms)",
        netem: "delay 30ms 3ms distribution normal rate 1gbit loss 0.05%",
        outage: None,
    },
    Profile {
        name: "Internet, intercontinental (RTT ~180ms)",
        netem: "delay 90ms 5ms distribution normal rate 1gbit loss 0.1%",
        outage: None,
    },
    Profile {
        name: "Home Wi-Fi",
        netem: "delay 25ms 8ms distribution normal rate 1gbit loss gemodel 0.1% 30%",
        outage: None,
    },
    Profile {
        name: "Congested Wi-Fi",
        netem: "delay 30ms 20ms distribution normal rate 1gbit loss gemodel 0.5% 25%",
        outage: None,
    },
    Profile {
        name: "Mobile (LTE)",
        netem: "delay 35ms 10ms distribution normal rate 1gbit loss 0.1%",
        outage: None,
    },
    Profile {
        name: "Uplink 3 Mbit/s, 500ms queue",
        netem: "delay 25ms rate 3mbit limit 125",
        outage: None,
    },
    Profile {
        name: "Uplink 1 Mbit/s, 500ms queue",
        netem: "delay 25ms rate 1mbit limit 40",
        outage: None,
    },
    Profile {
        name: "Mobile handover (50ms outage every 30s)",
        netem: "delay 35ms 10ms distribution normal rate 1gbit loss 0.1%",
        outage: Some(Outage {
            netem: "delay 35ms loss 100%",
            every: Duration::from_secs(30),
            duration: Duration::from_millis(50),
        }),
    },
    Profile {
        name: "Mobile bandwidth dip (1 Mbit/s for 5s every 30s)",
        netem: "delay 35ms 10ms distribution normal rate 1gbit loss 0.1%",
        outage: Some(Outage {
            netem: "delay 35ms rate 1mbit limit 40",
            every: Duration::from_secs(30),
            duration: Duration::from_secs(5),
        }),
    },
    Profile {
        name: "Mobile coverage gap (3s outage every 2min)",
        netem: "delay 35ms 10ms distribution normal rate 1gbit loss 0.1%",
        outage: Some(Outage {
            netem: "delay 35ms loss 100%",
            every: Duration::from_secs(120),
            duration: Duration::from_secs(3),
        }),
    },
    Profile {
        name: "Stress: 1% random loss (RTT 100ms)",
        netem: "delay 50ms rate 1gbit loss 1%",
        outage: None,
    },
    Profile {
        name: "Stress: 5% random loss (RTT 100ms)",
        netem: "delay 50ms rate 1gbit loss 5%",
        outage: None,
    },
    Profile {
        name: "Stress: heavy reordering",
        netem: "delay 50ms 20ms distribution normal",
        outage: None,
    },
    Profile {
        name: "Stress: 1% loss + heavy reordering",
        netem: "delay 100ms 50ms loss 1%",
        outage: None,
    },
    Profile {
        name: "Stress: batched delivery (20-40ms slots)",
        netem: "delay 30ms slot 20ms 40ms",
        outage: None,
    },
    Profile {
        name: "Stress: 3s outage every 40s",
        netem: "delay 50ms",
        outage: Some(Outage {
            netem: "delay 50ms loss 100%",
            every: Duration::from_secs(40),
            duration: Duration::from_secs(3),
        }),
    },
    Profile {
        name: "Stress: 10s outage every 40s",
        netem: "delay 50ms",
        outage: Some(Outage {
            netem: "delay 50ms loss 100%",
            every: Duration::from_secs(40),
            duration: Duration::from_secs(10),
        }),
    },
];

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
        .prompt()?;
    let port = Text::new("UDP port to throttle:")
        .with_help_message("Leave empty to throttle all UDP traffic on the device")
        .prompt()?;
    let port = match port.trim() {
        "" => None,
        port => Some(port.parse::<u16>().context("Invalid port")?),
    };

    let throttle = Throttle::new(device, port)?;
    let throttle = &throttle;
    thread::scope(|scope| -> Result<()> {
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
            throttle.set_netem(profile.netem)?;
            outage_loop = profile.outage.as_ref().map(|outage| {
                let (stop_sender, stop_receiver) = mpsc::channel::<()>();
                let handle = scope
                    .spawn(move || run_outage_loop(throttle, profile.netem, outage, stop_receiver));
                (stop_sender, handle)
            });
            println!("Applied: {}", profile.name);
        }
    })
}

fn run_outage_loop(throttle: &Throttle, netem: &str, outage: &Outage, stop_receiver: Receiver<()>) {
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
        if let Err(err) = throttle.set_netem(outage.netem) {
            eprintln!("Failed to start outage: {err:#}");
            return;
        }
        if !wait(outage.duration) {
            return;
        }
        if let Err(err) = throttle.set_netem(netem) {
            eprintln!("Failed to end outage: {err:#}");
            return;
        }
    }
}

/// Netem qdisc on a device, attached to a prio band that only the UDP filters lead to; removed on
/// drop.
struct Throttle {
    device: String,
    use_sudo: bool,
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
