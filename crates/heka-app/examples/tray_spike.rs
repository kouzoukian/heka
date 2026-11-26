//! Proves that a `tray-icon` tray works inside GPUI's event loop on Windows.
//!
//! Touches no HID, ViGEmBus, HidHide or registry code. It runs by itself and ends
//! within a few seconds:
//!
//! 1. opens a window with `QuitMode::Explicit` and adds a tray icon with a menu,
//! 2. closes the window and checks the app is still running,
//! 3. posts the tray's own left-click message to the tray icon's hidden window,
//!    which GPUI's message loop dispatches to `tray-icon`,
//! 4. reopens the window when the click event arrives, then quits.
//!
//! `cargo run -p heka-app --example tray_spike`

use std::time::{Duration, Instant};

use gpui::{
    App, AppContext, Context, IntoElement, ParentElement, QuitMode, Render, Styled, Window,
    WindowOptions, div,
};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

struct Hello;

impl Render for Hello {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child("tray spike")
    }
}

fn open_window(cx: &mut App) {
    cx.open_window(WindowOptions::default(), |_, cx| cx.new(|_| Hello))
        .expect("window");
}

/// `tray-icon` 0.25.1 `src/platform_impl/windows/mod.rs`: the class of its hidden
/// window and the callback message it registers with `Shell_NotifyIconW`.
const TRAY_CLASS: &str = "tray_icon_app";
const WM_USER_TRAYICON: u32 = 6002;

/// Finds the tray's hidden window among this thread's windows and posts it the
/// message the shell sends on a left button release.
fn post_tray_left_click() -> bool {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumThreadWindows, GetClassNameW, PostMessageW, WM_LBUTTONUP,
    };

    unsafe extern "system" fn find(hwnd: HWND, found: LPARAM) -> i32 {
        let mut name = [0u16; 64];
        let len = unsafe { GetClassNameW(hwnd, name.as_mut_ptr(), name.len() as i32) };
        if String::from_utf16_lossy(&name[..len as usize]) == TRAY_CLASS {
            unsafe { *(found as *mut HWND) = hwnd };
            return 0;
        }
        1
    }

    let mut hwnd: HWND = std::ptr::null_mut();
    unsafe {
        EnumThreadWindows(
            GetCurrentThreadId(),
            Some(find),
            &mut hwnd as *mut HWND as LPARAM,
        );
        !hwnd.is_null() && PostMessageW(hwnd, WM_USER_TRAYICON, 0, WM_LBUTTONUP as LPARAM) != 0
    }
}

fn icon() -> Icon {
    let size = 32u32;
    let rgba = (0..size * size)
        .flat_map(|_| [0x4f, 0x8c, 0xff, 0xff])
        .collect();
    Icon::from_rgba(rgba, size, size).expect("icon")
}

fn main() {
    gpui_platform::application()
        .with_quit_mode(QuitMode::Explicit)
        .run(|cx: &mut App| {
            cx.on_app_quit(|_| {
                eprintln!("spike: on_app_quit ran");
                async {}
            })
            .detach();

            open_window(cx);

            let open = MenuItem::new("Open heka", true, None);
            let quit = MenuItem::new("Quit", true, None);
            let menu = Menu::new();
            menu.append_items(&[&open, &quit]).expect("menu");
            let tray = TrayIconBuilder::new()
                .with_icon(icon())
                .with_tooltip("heka tray spike")
                .with_menu(Box::new(menu))
                .with_menu_on_left_click(false)
                .build()
                .expect("tray icon");

            let (open_id, quit_id) = (open.id().clone(), quit.id().clone());
            cx.spawn(async move |cx| {
                let _tray = tray;
                let started = Instant::now();
                let mut closed = false;
                let mut clicked = false;
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                    let elapsed = started.elapsed();

                    while let Ok(event) = TrayIconEvent::receiver().try_recv() {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            eprintln!("spike: tray click reached GPUI after {elapsed:?}");
                            cx.update(open_window);
                        }
                    }
                    while let Ok(event) = MenuEvent::receiver().try_recv() {
                        if event.id == open_id {
                            cx.update(open_window);
                        } else if event.id == quit_id {
                            cx.update(|cx| cx.quit());
                        }
                    }

                    let windows = cx.update(|cx| cx.windows().len());
                    if !closed && elapsed > Duration::from_secs(1) {
                        closed = true;
                        cx.update(|cx| {
                            for window in cx.windows() {
                                let _ = window.update(cx, |_, window, _| window.remove_window());
                            }
                        });
                    } else if closed && !clicked && elapsed > Duration::from_secs(2) {
                        eprintln!("spike: windows after close = {windows}, app still running");
                        clicked = true;
                        eprintln!("spike: posted tray click = {}", post_tray_left_click());
                    } else if clicked && windows == 1 {
                        eprintln!("spike: PASS, window reopened from the tray");
                        cx.update(|cx| cx.quit());
                    } else if elapsed > Duration::from_secs(10) {
                        eprintln!("spike: FAIL, timed out with {windows} windows");
                        cx.update(|cx| cx.quit());
                    }
                }
            })
            .detach();
        });
}
