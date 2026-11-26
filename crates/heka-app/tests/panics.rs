use std::thread;

use heka_app::log::{self, Log};
use heka_core::clock::Timestamp;

#[test]
fn a_panic_is_logged_with_its_thread_and_message_before_the_thread_ends() {
    log::install(Log::start(None, Timestamp::default));
    log::catch_panics();

    let panicking = thread::Builder::new()
        .name("heka-doomed".into())
        .spawn(|| panic!("it all went wrong"))
        .unwrap();
    assert!(panicking.join().is_err());

    let recent = log::recent();
    let message = &recent.back().expect("the panic was logged").message;
    assert!(
        message.starts_with("Panic on thread heka-doomed:"),
        "{message}"
    );
    assert!(message.contains("it all went wrong"), "{message}");
    assert!(message.contains("panics.rs"), "{message}");
}
