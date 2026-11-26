//! Records raw DualSense USB input reports, read-only, as fixture lines.
//!
//! ```text
//! cargo run -p heka-windows --example capture_dualsense -- [COUNT] > reports.hex
//! ```
//!
//! Prints every DualSense HID collection it finds on stderr, then COUNT reports
//! (default 50) as hex lines on stdout, in the format `heka-core`'s fixtures use.
//! It also prints the firmware info feature report, the Feedback format with the
//! rumble mode chosen from it and the motion calibration feature report on
//! stderr. Hold the inputs you want recorded while it runs. Nothing is sent
//! to the Controller: it opens the device and reads, nothing else.

use std::io::Write;
use std::time::{Duration, Instant};

use heka_core::link::{LinkRead, LinkReader};
use heka_windows::hid::{
    ControllerLink, dualsense_usb_devices, dualsense_usb_paths, read_calibration_report,
    read_firmware_info,
};
use hidapi::HidApi;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let count: usize = match std::env::args().nth(1) {
        Some(arg) => arg.parse()?,
        None => 50,
    };

    let api = HidApi::new()?;
    for info in api
        .device_list()
        .filter(|info| heka_core::dualsense::is_dualsense(info.vendor_id(), info.product_id()))
    {
        eprintln!(
            "found {:04x}:{:04x} bus={:?} interface={} usage_page={:#06x} usage={:#06x}",
            info.vendor_id(),
            info.product_id(),
            info.bus_type(),
            info.interface_number(),
            info.usage_page(),
            info.usage(),
        );
    }
    eprintln!(
        "{} usable DualSense USB gamepad collection(s)",
        dualsense_usb_devices(&api).count()
    );
    for path in dualsense_usb_paths(&api) {
        match ControllerLink::open_path(&api, &path) {
            Ok((_, mac)) => eprintln!("MAC address {mac}"),
            Err(error) => eprintln!("cannot open {path:?}: {error}"),
        }
    }

    let Some(path) = dualsense_usb_paths(&api).into_iter().next() else {
        return Err("no DualSense on USB".into());
    };
    let (mut link, _) = ControllerLink::open_path(&api, &path)?;
    let device = api.open_path(link.hid_path())?;
    match read_firmware_info(&device) {
        Some(info) => {
            let hex: Vec<String> = info.iter().map(|b| format!("{b:02x}")).collect();
            eprintln!("firmware info (feature report 0x20): {}", hex.join(" "));
        }
        None => eprintln!("firmware info (feature report 0x20): not available"),
    }
    if let Some(writer) = link.feedback_writer() {
        // Only names the rumble mode: nothing is written.
        eprintln!("Feedback format: {:?}", writer.format());
    }
    match read_calibration_report(&device) {
        Some(report) => {
            let hex: Vec<String> = report.iter().map(|b| format!("{b:02x}")).collect();
            eprintln!("calibration (feature report 0x05): {}", hex.join(" "));
            eprintln!(
                "calibration parsed: {:?}",
                heka_core::dualsense::parse_calibration(&report)
            );
        }
        None => eprintln!("calibration (feature report 0x05): not available"),
    }
    drop(device);

    let mut out = std::io::stdout().lock();
    let mut buf = [0u8; 128];
    let mut captured = 0;
    let deadline = Instant::now() + Duration::from_secs(30);
    while captured < count && Instant::now() < deadline {
        match link.read(&mut buf) {
            LinkRead::Report(len) => {
                let hex: Vec<String> = buf[..len].iter().map(|b| format!("{b:02x}")).collect();
                writeln!(out, "{}", hex.join(" "))?;
                captured += 1;
            }
            LinkRead::Timeout => {}
            LinkRead::Lost => return Err("the DualSense was unplugged".into()),
        }
    }
    eprintln!("captured {captured} report(s)");
    Ok(())
}
