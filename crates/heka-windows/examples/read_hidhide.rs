use heka_core::hiding::{HidHide, HidHideStatus};
use heka_windows::hid::dualsense_usb_devices;
use heka_windows::hidhide::{HidHideDriver, device_instance_path, journal_path, own_image_name};
use hidapi::HidApi;

fn main() {
    println!("status: {:?}", HidHideStatus::detect(&HidHideDriver));
    match HidHideDriver.read() {
        Ok(config) => {
            println!("cloaking on: {}", config.active);
            println!("inverted application list: {}", config.inverse);
            println!("hidden devices:");
            for device in &config.hidden_devices {
                println!("  {device}");
            }
            println!("allowed applications:");
            for app in &config.allowed_apps {
                println!("  {app}");
            }
        }
        Err(error) => println!("cannot read HidHide: {error:?}"),
    }

    match own_image_name() {
        Ok(name) => println!("this program as HidHide would list it: {name}"),
        Err(error) => println!("cannot read the image name: {error}"),
    }
    println!("journal: {:?}", journal_path());

    let api = HidApi::new().expect("hidapi");
    for info in dualsense_usb_devices(&api) {
        match device_instance_path(info.path()) {
            Ok(path) => println!("DualSense on USB would be hidden as: {path}"),
            Err(error) => println!("cannot resolve {:?}: {error:?}", info.path()),
        }
    }
}
