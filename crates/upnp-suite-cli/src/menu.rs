//! The interactive top-level menu shown when `upnp-discover` is run with no
//! subcommand.

use std::fmt;
use std::time::Duration;

const DEFAULT_WAIT_SECS: u64 = 3;

#[derive(Clone, Copy)]
enum Choice {
    Discover,
    Capabilities,
    Gena,
    Dashboard,
    Exit,
}

impl fmt::Display for Choice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Choice::Discover => "List devices (SSDP)",
            Choice::Capabilities => "Device capabilities report",
            Choice::Gena => "Subscribe to GENA events",
            Choice::Dashboard => "Device dashboard",
            Choice::Exit => "Exit",
        };
        write!(f, "{label}")
    }
}

pub async fn run() {
    loop {
        let options = vec![
            Choice::Discover,
            Choice::Capabilities,
            Choice::Gena,
            Choice::Dashboard,
            Choice::Exit,
        ];
        let Ok(choice) = inquire::Select::new("What do you want to do?", options).prompt() else {
            return;
        };

        match choice {
            Choice::Discover => {
                let devices = match crate::discover::scan(Duration::from_secs(DEFAULT_WAIT_SECS)).await {
                    Ok(devices) => devices,
                    Err(err) => {
                        eprintln!("SSDP discovery failed: {err}");
                        continue;
                    }
                };
                if devices.is_empty() {
                    println!("No UPnP devices responded.");
                } else {
                    let show_vars = inquire::Confirm::new("Show allowed state variable values too?")
                        .with_default(false)
                        .prompt()
                        .unwrap_or(false);
                    crate::discover::print_report(&devices, show_vars);
                }
            }
            Choice::Capabilities => {
                crate::capabilities::run(Duration::from_secs(DEFAULT_WAIT_SECS)).await
            }
            Choice::Gena => crate::gena::run(Duration::from_secs(DEFAULT_WAIT_SECS)).await,
            Choice::Dashboard => crate::dashboard::run(Duration::from_secs(DEFAULT_WAIT_SECS)).await,
            Choice::Exit => return,
        }

        println!();
    }
}
