//! Command-line surface: a subcommand runs that one action directly; with
//! none given, `main` falls back to the interactive menu (see
//! [`crate::menu`]).

use clap::{Parser, Subcommand};

const DEFAULT_WAIT_SECS: u64 = 3;

#[derive(Parser)]
#[command(name = "upnp-discover", version, about = "UPnP control-point CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Send an SSDP M-SEARCH and report every discovered device's services
    /// (the original `upnp-discover` behavior).
    Discover {
        /// How long to listen for search responses.
        #[arg(default_value_t = DEFAULT_WAIT_SECS)]
        wait_secs: u64,
        /// Also report each state variable's allowed values/ranges.
        #[arg(short, long)]
        vars: bool,
    },
    /// Discover devices, pick one, and print its full service/action/
    /// evented-variable/allowed-values capabilities report.
    Capabilities {
        #[arg(default_value_t = DEFAULT_WAIT_SECS)]
        wait_secs: u64,
    },
    /// Discover devices, pick a GENA-eventable service, subscribe, and
    /// print NOTIFYs (or poll fallbacks) to the terminal as they arrive.
    Gena {
        #[arg(default_value_t = DEFAULT_WAIT_SECS)]
        wait_secs: u64,
    },
    /// Discover devices, pick one, and show a live-updating transport/
    /// position/volume dashboard for it.
    Dashboard {
        #[arg(default_value_t = DEFAULT_WAIT_SECS)]
        wait_secs: u64,
    },
}
