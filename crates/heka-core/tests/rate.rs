use std::time::{Duration, Instant};

use heka_core::rate::RateMeter;

fn feed(meter: &mut RateMeter, start: Instant, every: Duration, reports: u32) {
    for report in 0..=reports {
        meter.tick(start + every * report);
    }
}

#[test]
fn a_meter_that_has_seen_nothing_has_no_rate() {
    let mut meter = RateMeter::default();

    assert_eq!(meter.per_second(), None);
    meter.tick(Instant::now());
    assert_eq!(meter.per_second(), None);
}

#[test]
fn a_steady_stream_gives_its_rate_once_a_second_has_passed() {
    let mut meter = RateMeter::default();

    feed(&mut meter, Instant::now(), Duration::from_millis(4), 300);

    let rate = meter.per_second().unwrap();
    assert!((248.0..=252.0).contains(&rate), "{rate}");
}

#[test]
fn a_slower_stream_gives_a_lower_rate() {
    let mut meter = RateMeter::default();

    feed(&mut meter, Instant::now(), Duration::from_millis(8), 200);

    let rate = meter.per_second().unwrap();
    assert!((123.0..=127.0).contains(&rate), "{rate}");
}

#[test]
fn the_rate_follows_a_change_in_the_stream() {
    let mut meter = RateMeter::default();
    let start = Instant::now();
    feed(&mut meter, start, Duration::from_millis(4), 300);
    let slow_start = start + Duration::from_millis(4) * 300;

    feed(&mut meter, slow_start, Duration::from_millis(10), 300);

    let rate = meter.per_second().unwrap();
    assert!((98.0..=102.0).contains(&rate), "{rate}");
}

#[test]
fn nothing_is_reported_before_the_first_second_is_over() {
    let mut meter = RateMeter::default();

    feed(&mut meter, Instant::now(), Duration::from_millis(4), 200);

    assert_eq!(meter.per_second(), None);
}

#[test]
fn resetting_forgets_the_rate() {
    let mut meter = RateMeter::default();
    feed(&mut meter, Instant::now(), Duration::from_millis(4), 300);

    meter.reset();

    assert_eq!(meter.per_second(), None);
}
