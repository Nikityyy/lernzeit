//! The "Mit Windows starten" setting: a value under HKCU\...\Run that starts Lernzeit
//! hidden in the tray.

use windows::core::{w, PCWSTR};
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_ANY,
};

const RUN: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");

pub fn enabled() -> bool {
    unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN, w!("Lernzeit"), RRF_RT_ANY, None, None, None).is_ok() }
}

pub fn set(on: bool) {
    unsafe {
        if on {
            let Ok(exe) = std::env::current_exe() else { return };
            let command: Vec<u16> = format!("\"{}\" --hidden", exe.display()).encode_utf16().chain([0]).collect();
            let _ = RegSetKeyValueW(
                HKEY_CURRENT_USER, RUN, w!("Lernzeit"), REG_SZ.0,
                Some(command.as_ptr().cast()), (command.len() * 2) as u32,
            );
        } else {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN, w!("Lernzeit"));
        }
    }
}

/// Keeps the startup entry pointing at this exe if it was moved.
pub fn refresh() {
    if enabled() {
        set(true);
    }
}
