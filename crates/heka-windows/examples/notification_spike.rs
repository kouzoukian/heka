use std::thread;
use std::time::Duration;

use heka_windows::notification::{self, Delivery};
use windows::UI::Notifications::ToastNotificationManager;
use windows::core::HSTRING;

const APP_ID: &str = "Kouzoukian.Heka.Spike";

fn setting() -> String {
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_ID))
        .and_then(|notifier| notifier.Setting())
        .map_or_else(
            |error| format!("error {error}"),
            |setting| format!("{setting:?}"),
        )
}

fn main() {
    eprintln!("spike: setting before registering = {}", setting());
    if let Err(error) = notification::register(APP_ID, "heka notification spike", None) {
        eprintln!("spike: FAIL, cannot register: {error}");
        return;
    }
    eprintln!("spike: setting after registering = {}", setting());
    let shown = notification::show(
        APP_ID,
        "spike",
        "heka notification spike",
        "An unpackaged program showed this <notification> & nothing else.",
    );
    eprintln!("spike: show = {shown:?}");
    match shown {
        Ok(Delivery::Shown) => {
            thread::sleep(Duration::from_secs(6));
            let kept = notification::in_notification_centre(APP_ID);
            eprintln!("spike: in the notification centre = {kept:?}");
            match kept {
                Ok(count) if count > 0 => eprintln!("spike: PASS, Windows took the notification"),
                _ => eprintln!("spike: FAIL, Windows holds no notification"),
            }
            eprintln!("spike: clear = {:?}", notification::clear(APP_ID));
        }
        Ok(Delivery::TurnedOffInWindows) => {
            eprintln!("spike: notifications are off in Windows; turn them on and run this again");
        }
        Err(_) => eprintln!("spike: FAIL"),
    }
    eprintln!(
        "spike: unregister = {:?}",
        notification::unregister(APP_ID).map_err(|error| error.to_string())
    );
}
