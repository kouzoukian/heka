use std::mem::size_of;

use heka_core::game_rules::{Front, program_name};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, LPARAM};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindowThreadProcessId, IsWindowVisible,
};

const PATH_CAPACITY: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Foreground {
    Program(String),
    Own,
    Unreadable,
    Nothing,
}

impl Foreground {
    pub fn as_front(&self) -> Front<'_> {
        match self {
            Self::Program(program) => Front::Program(program),
            Self::Own => Front::Own,
            Self::Unreadable | Self::Nothing => Front::Nothing,
        }
    }

    pub fn is_unreadable(&self) -> bool {
        *self == Self::Unreadable
    }
}

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

pub fn foreground() -> Foreground {
    let window = unsafe { GetForegroundWindow() };
    if window.is_null() {
        return Foreground::Nothing;
    }
    let mut process = 0u32;
    unsafe { GetWindowThreadProcessId(window, &mut process) };
    if process == 0 {
        return Foreground::Nothing;
    }
    if process == unsafe { GetCurrentProcessId() } {
        return Foreground::Own;
    }
    match image_path(process) {
        Some(path) => Foreground::Program(program_name(&path)),
        None => Foreground::Unreadable,
    }
}

fn image_path(process: u32) -> Option<String> {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process) };
    if handle.is_null() {
        return None;
    }
    let handle = Handle(handle);
    let mut buffer = [0u16; PATH_CAPACITY];
    let mut length = PATH_CAPACITY as u32;
    let read = unsafe { QueryFullProcessImageNameW(handle.0, 0, buffer.as_mut_ptr(), &mut length) };
    (read != 0).then(|| String::from_utf16_lossy(&buffer[..length as usize]))
}

pub fn is_running(program: &str) -> bool {
    let wanted = program_name(program);
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return false;
    }
    let snapshot = Handle(snapshot);
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    let mut more = unsafe { Process32FirstW(snapshot.0, &mut entry) } != 0;
    while more {
        let end = entry
            .szExeFile
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(entry.szExeFile.len());
        if program_name(&String::from_utf16_lossy(&entry.szExeFile[..end])) == wanted {
            return true;
        }
        more = unsafe { Process32NextW(snapshot.0, &mut entry) } != 0;
    }
    false
}

pub fn running_programs() -> Vec<String> {
    let mut processes: Vec<u32> = Vec::new();
    unsafe {
        EnumWindows(
            Some(collect_visible),
            &mut processes as *mut Vec<u32> as LPARAM,
        );
    }
    let own = unsafe { GetCurrentProcessId() };
    let mut programs: Vec<String> = processes
        .into_iter()
        .filter(|&process| process != own)
        .filter_map(image_path)
        .map(|path| program_name(&path))
        .filter(|program| !program.is_empty())
        .collect();
    programs.sort();
    programs.dedup();
    programs
}

unsafe extern "system" fn collect_visible(window: *mut core::ffi::c_void, found: LPARAM) -> i32 {
    if unsafe { IsWindowVisible(window) } != 0 {
        let mut process = 0u32;
        unsafe { GetWindowThreadProcessId(window, &mut process) };
        if process != 0 {
            let processes = unsafe { &mut *(found as *mut Vec<u32>) };
            if !processes.contains(&process) {
                processes.push(process);
            }
        }
    }
    1
}
