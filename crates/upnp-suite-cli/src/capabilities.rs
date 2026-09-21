//! Per-device capabilities report: scan, pick one device, and print both
//! what its SCPD *advertises* it supports (the same formatting
//! [`discover::print_report`](crate::discover::print_report) uses, just
//! focused on a single device and always with the allowed-values detail
//! shown) and what it *actually reports right now* by connecting and
//! calling `GetDeviceCapabilities`/`GetCurrentTransportActions`/
//! `GetTransportSettings`/`GetMediaInfo` (AVTransport) and
//! `GetVolumeDBRange`/`ListPresets` (RenderingControl) — instance state and
//! capability queries the CLI never issued before.

use std::time::Duration;

use upnp_suite_client::{
    AVTransportClient, ClientError, RendererClient, RenderingControlClient,
};

use crate::discover::{self, Palette};

pub async fn run(wait: Duration) {
    let devices = match discover::scan(wait).await {
        Ok(devices) => devices,
        Err(err) => {
            eprintln!("SSDP discovery failed: {err}");
            return;
        }
    };

    if devices.is_empty() {
        println!("No UPnP devices responded.");
        return;
    }

    let Ok(device) = inquire::Select::new("Capabilities for:", devices).prompt() else {
        return;
    };

    discover::print_report(std::slice::from_ref(&device), true);
    print_live_capabilities(&device.report.location).await;
}

async fn print_live_capabilities(description_url: &str) {
    let palette = Palette::detect();
    println!(
        "\n{bold}Live capabilities{reset} {dim}(queried just now over SOAP){reset}",
        bold = palette.bold,
        dim = palette.dim,
        reset = palette.reset,
    );

    let client = match RendererClient::connect(description_url).await {
        Ok(client) => client,
        Err(err) => {
            println!("  Failed to connect: {err}");
            return;
        }
    };

    match client.av_transport() {
        Ok(av) => print_av_capabilities(&palette, &av).await,
        Err(_) => println!("  AVTransport: not present on this device"),
    }

    match client.rendering_control() {
        Ok(rc) => print_rendering_capabilities(&palette, &rc).await,
        Err(_) => println!("  RenderingControl: not present on this device"),
    }
}

async fn print_av_capabilities(palette: &Palette, av: &AVTransportClient<'_>) {
    println!("  {bold}AVTransport{reset}", bold = palette.bold, reset = palette.reset);

    match av.get_device_capabilities().await {
        Ok(caps) => {
            println!("    Play media: {}", caps.play_media);
            println!("    Record media: {}", caps.rec_media);
            println!("    Record quality modes: {}", caps.rec_quality_modes);
        }
        Err(err) => print_unavailable("GetDeviceCapabilities", &err),
    }

    match av.get_current_transport_actions().await {
        Ok(actions) => println!("    Currently valid actions: {}", actions.join(", ")),
        Err(err) => print_unavailable("GetCurrentTransportActions", &err),
    }

    match av.get_transport_settings().await {
        Ok(settings) => {
            println!("    Play mode: {}", settings.play_mode);
            println!("    Record quality mode: {}", settings.rec_quality_mode);
        }
        Err(err) => print_unavailable("GetTransportSettings", &err),
    }

    match av.get_media_info().await {
        Ok(media) => {
            println!(
                "    Media: {} track(s), duration {}, play medium {}",
                media.nr_tracks, media.media_duration, media.play_medium
            );
            if !media.current_uri.is_empty() {
                println!("    Current URI: {}", media.current_uri);
            }
        }
        Err(err) => print_unavailable("GetMediaInfo", &err),
    }
}

async fn print_rendering_capabilities(palette: &Palette, rc: &RenderingControlClient<'_>) {
    println!(
        "  {bold}RenderingControl{reset}",
        bold = palette.bold,
        reset = palette.reset
    );

    match rc.get_volume_db_range().await {
        Ok(range) => println!("    Volume range: {} to {} (1/100 dB)", range.min, range.max),
        Err(err) => print_unavailable("GetVolumeDBRange", &err),
    }

    match rc.list_presets().await {
        Ok(presets) => println!("    Presets: {}", presets.join(", ")),
        Err(err) => print_unavailable("ListPresets", &err),
    }
}

fn print_unavailable(action: &str, err: &ClientError) {
    println!("    {action}: not supported ({err})");
}
