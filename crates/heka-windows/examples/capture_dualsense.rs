use std::error::Error;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use heka_core::dualsense::{BT_INPUT_REPORT_ID, USB_INPUT_REPORT_ID};
use heka_core::link::{LinkKind, LinkRead, LinkReader};
use heka_core::mac::MacAddress;
use heka_windows::hid::{
    ControllerLink, dualsense_devices, read_calibration_report, read_firmware_info,
};
use hidapi::HidApi;

const SETTLE: Duration = Duration::from_millis(500);
const STEP_TIMEOUT: Duration = Duration::from_secs(5);
const REPORTS_PER_STEP: usize = 5;

const STEPS: [(&str, &str); 29] = [
    ("rest", "Leave the Controller untouched"),
    ("cross", "Hold Cross"),
    ("circle", "Hold Circle"),
    ("square", "Hold Square"),
    ("triangle", "Hold Triangle"),
    ("dpad_up", "Hold Up on the d-pad"),
    ("dpad_right", "Hold Right on the d-pad"),
    ("dpad_down", "Hold Down on the d-pad"),
    ("dpad_left", "Hold Left on the d-pad"),
    ("l1", "Hold L1"),
    ("r1", "Hold R1"),
    ("l2", "Pull L2 all the way"),
    ("r2", "Pull R2 all the way"),
    ("l3", "Press the left stick in (L3)"),
    ("r3", "Press the right stick in (R3)"),
    ("create", "Hold Create"),
    ("options", "Hold Options"),
    ("ps", "Hold PS"),
    (
        "touchpad",
        "With one finger on the left half, click the touchpad and hold it down",
    ),
    ("mute", "Hold the mute button"),
    ("left_stick_up", "Push the left stick all the way up"),
    ("left_stick_right", "Push the left stick all the way right"),
    ("left_stick_down", "Push the left stick all the way down"),
    ("left_stick_left", "Push the left stick all the way left"),
    ("right_stick_up", "Push the right stick all the way up"),
    (
        "right_stick_right",
        "Push the right stick all the way right",
    ),
    ("right_stick_down", "Push the right stick all the way down"),
    ("right_stick_left", "Push the right stick all the way left"),
    (
        "two_fingers",
        "Without clicking, rest one finger near the touchpad's top left corner, \
         then a second near its bottom right corner",
    ),
];

struct Args {
    kind: LinkKind,
    guided: Option<PathBuf>,
    count: usize,
}

fn args() -> Result<Args, Box<dyn Error>> {
    let mut parsed = Args {
        kind: LinkKind::Usb,
        guided: None,
        count: 50,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bluetooth" => parsed.kind = LinkKind::Bluetooth,
            "--guided" => {
                parsed.guided = Some(args.next().ok_or("--guided needs a file")?.into());
            }
            count => parsed.count = count.parse()?,
        }
    }
    Ok(parsed)
}

fn main() -> Result<(), Box<dyn Error>> {
    let Args {
        kind,
        guided,
        count,
    } = args()?;

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

    let Some(info) = dualsense_devices(&api, kind).next() else {
        return Err(format!("no DualSense on {kind:?}").into());
    };
    let (mut link, mac, _) = ControllerLink::open_path(&api, info.path())
        .map_err(|error| format!("cannot open the DualSense: {error:?}"))?;
    eprintln!("MAC address {mac}");
    let device = api.open_path(link.hid_path())?;
    match read_firmware_info(&device) {
        Some(info) => eprintln!("firmware info (feature report 0x20): {}", hex(&info)),
        None => eprintln!("firmware info (feature report 0x20): not available"),
    }
    if let Some(writer) = link.feedback_writer() {
        eprintln!("Feedback format: {:?}", writer.format());
    }
    match read_calibration_report(&device) {
        Some(report) => {
            eprintln!("calibration (feature report 0x05): {}", hex(&report));
            eprintln!(
                "calibration parsed: {:?}",
                heka_core::dualsense::parse_calibration(&report)
            );
        }
        None => eprintln!("calibration (feature report 0x05): not available"),
    }
    drop(device);

    if let Some(path) = guided {
        return record_steps(&mut link, kind, mac, &path);
    }

    let mut out = std::io::stdout().lock();
    let mut buf = [0u8; 128];
    let mut captured = 0;
    let deadline = Instant::now() + Duration::from_secs(30);
    while captured < count && Instant::now() < deadline {
        match link.read(&mut buf) {
            LinkRead::Report(len) => {
                writeln!(out, "{}", hex(&buf[..len]))?;
                captured += 1;
            }
            LinkRead::Timeout => {}
            LinkRead::Lost => return Err("the DualSense was lost".into()),
        }
    }
    eprintln!("captured {captured} report(s)");
    Ok(())
}

fn record_steps(
    link: &mut ControllerLink,
    kind: LinkKind,
    mac: MacAddress,
    path: &Path,
) -> Result<(), Box<dyn Error>> {
    let mut recorded: Vec<Vec<Vec<u8>>> = Vec::new();
    let mut entered = std::io::stdin().lock().lines();
    while recorded.len() < STEPS.len() {
        let (_, instruction) = STEPS[recorded.len()];
        eprintln!(
            "\n{}/{}: {instruction}, then press Enter (r and Enter redoes the step before)",
            recorded.len() + 1,
            STEPS.len()
        );
        let line = entered.next().ok_or("the input was closed")??;
        if line.trim() == "r" {
            recorded.pop();
            continue;
        }
        recorded.push(record(link, kind, mac)?);
        eprintln!("recorded");
    }

    let mut out = std::fs::File::create(path)?;
    for ((name, _), reports) in STEPS.iter().zip(recorded) {
        for report in reports {
            writeln!(out, "{name}: {}", hex(&report))?;
        }
    }
    eprintln!("\nwrote {}", path.display());
    Ok(())
}

fn record(
    link: &mut ControllerLink,
    kind: LinkKind,
    mac: MacAddress,
) -> Result<Vec<Vec<u8>>, Box<dyn Error>> {
    let id = match kind {
        LinkKind::Usb => USB_INPUT_REPORT_ID,
        LinkKind::Bluetooth => BT_INPUT_REPORT_ID,
    };
    let mut reversed = mac.0;
    reversed.reverse();
    let mut buf = [0u8; 128];
    let settled = Instant::now() + SETTLE;
    let mut reports = Vec::new();
    while reports.len() < REPORTS_PER_STEP {
        if Instant::now() > settled + STEP_TIMEOUT {
            return Err("the DualSense sent too few reports".into());
        }
        match link.read(&mut buf) {
            LinkRead::Report(len) if Instant::now() >= settled => {
                let report = &buf[..len];
                if report.first() != Some(&id) {
                    return Err(format!("unexpected report: {}", hex(report)).into());
                }
                if report
                    .windows(mac.0.len())
                    .any(|bytes| bytes == mac.0 || bytes == reversed)
                {
                    return Err("a report holds the Controller's address".into());
                }
                reports.push(report.to_vec());
            }
            LinkRead::Report(_) | LinkRead::Timeout => {}
            LinkRead::Lost => return Err("the DualSense was lost".into()),
        }
    }
    Ok(reports)
}

fn hex(bytes: &[u8]) -> String {
    let bytes: Vec<String> = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    bytes.join(" ")
}
