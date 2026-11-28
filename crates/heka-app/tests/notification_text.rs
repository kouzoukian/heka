use heka_app::text;
use heka_core::alerts::Alert;
use heka_core::mac::MacAddress;
use heka_core::supervisor::BridgeError;

const MAC: MacAddress = MacAddress([0x02, 0, 0, 0, 0, 1]);

fn alerts() -> [Alert; 4] {
    [
        Alert::LowBattery { percent: 15 },
        Alert::BridgeFailed(BridgeError::NoBus),
        Alert::Connected,
        Alert::Removed,
    ]
}

#[test]
fn each_notification_says_what_happened() {
    let bodies = alerts().map(|alert| text::notification_body(&alert));

    assert_eq!(
        bodies,
        [
            "Low battery, 15% left.",
            "Could not Bridge: ViGEmBus is not connected",
            "Connected.",
            "Removed.",
        ]
    );
}

#[test]
fn a_newer_notification_replaces_only_the_one_it_makes_stale() {
    let [battery, bridge, connected, removed] =
        alerts().map(|alert| text::notification_tag(MAC, &alert));
    let other = text::notification_tag(MacAddress([0x02, 0, 0, 0, 0, 2]), &Alert::Connected);

    assert_eq!(connected, removed);
    assert_ne!(battery, bridge);
    assert_ne!(bridge, connected);
    assert_ne!(connected, other);
    assert!(
        [battery, bridge, connected]
            .iter()
            .all(|tag| tag.len() <= 64)
    );
}
