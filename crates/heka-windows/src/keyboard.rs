use std::cell::RefCell;
use std::io;
use std::marker::PhantomData;
use std::ptr;

use heka_core::capture::{KeyListener, KeyStroke};
use heka_core::desktop::Key;
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardLayout, MAPVK_VK_TO_CHAR, MAPVK_VSC_TO_VK_EX, MapVirtualKeyExW, VkKeyScanExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, HC_ACTION, HHOOK, SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD,
};

const SCAN_SHIFT: u32 = 16;
const EXTENDED_BIT: u32 = 1 << 24;
const WAS_DOWN_BIT: u32 = 1 << 30;
const RELEASED_BIT: u32 = 1 << 31;
const DEAD_KEY_BIT: u32 = 1 << 31;
const NEEDS_CTRL_AND_ALT: i16 = 0x0600;
const NO_SUCH_CHARACTER: i16 = -1;
const EURO_SIGN: u16 = 0x20AC;

const FAKE_SHIFTS: [Key; 2] = [Key::extended(0x2A), Key::extended(0x36)];

thread_local! {
    static HEARD: RefCell<Vec<KeyStroke>> = const { RefCell::new(Vec::new()) };
}

pub struct KeyHook {
    hook: HHOOK,
    alt_gr: bool,
    this_thread: PhantomData<*const ()>,
}

impl KeyHook {
    pub fn install() -> io::Result<Self> {
        HEARD.with_borrow_mut(Vec::clear);
        let hook = unsafe {
            SetWindowsHookExW(
                WH_KEYBOARD,
                Some(hear),
                ptr::null_mut(),
                GetCurrentThreadId(),
            )
        };
        if hook.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            hook,
            alt_gr: layout_has_alt_gr(),
            this_thread: PhantomData,
        })
    }
}

impl KeyListener for KeyHook {
    fn heard(&mut self) -> Vec<KeyStroke> {
        HEARD.with_borrow_mut(std::mem::take)
    }

    fn has_alt_gr(&self) -> bool {
        self.alt_gr
    }
}

impl Drop for KeyHook {
    fn drop(&mut self) {
        unsafe { UnhookWindowsHookEx(self.hook) };
    }
}

unsafe extern "system" fn hear(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32
        && let Some((stroke, repeated)) = stroke_of(lparam as u32)
    {
        if !repeated && !FAKE_SHIFTS.contains(&stroke.key) {
            HEARD.with_borrow_mut(|heard| heard.push(stroke));
        }
        return 1;
    }
    unsafe { CallNextHookEx(ptr::null_mut(), code, wparam, lparam) }
}

fn stroke_of(flags: u32) -> Option<(KeyStroke, bool)> {
    let code = (flags >> SCAN_SHIFT) as u8;
    if code == 0 {
        return None;
    }
    let key = if flags & EXTENDED_BIT != 0 {
        Key::extended(code)
    } else {
        Key::new(code)
    };
    let down = flags & RELEASED_BIT == 0;
    let repeated = down && flags & WAS_DOWN_BIT != 0;
    Some((KeyStroke { key, down }, repeated))
}

fn layout_has_alt_gr() -> bool {
    let layout = unsafe { GetKeyboardLayout(0) };
    (0x20..0x250).chain([EURO_SIGN]).any(|character| {
        let keys = unsafe { VkKeyScanExW(character, layout) };
        keys != NO_SUCH_CHARACTER && keys & NEEDS_CTRL_AND_ALT == NEEDS_CTRL_AND_ALT
    })
}

pub fn key_label(key: Key) -> Option<String> {
    let character = unsafe {
        let layout = GetKeyboardLayout(0);
        let virtual_key = MapVirtualKeyExW(u32::from(key.scan()), MAPVK_VSC_TO_VK_EX, layout);
        if virtual_key == 0 {
            return None;
        }
        MapVirtualKeyExW(virtual_key, MAPVK_VK_TO_CHAR, layout)
    };
    char::from_u32(character & !DEAD_KEY_BIT)
        .filter(|character| !character.is_control() && !character.is_whitespace())
        .map(|character| character.to_uppercase().collect())
}
