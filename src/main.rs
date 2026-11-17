#![windows_subsystem = "windows"]

mod app;
mod autostart;
mod clock;
mod core;
mod icon;
mod spotify;
mod stats;
mod store;
mod theme;
mod tray;

use std::sync::{Arc, Mutex};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Threading::{CreateEventW, CreateMutexW, SetEvent};

const INSTANCE_MUTEX: PCWSTR = w!("Local\\Lernzeit");
const RESTORE_EVENT: PCWSTR = w!("Local\\Lernzeit.Restore");

fn main() -> eframe::Result {
    // The event asks the existing egui instance to restore its own viewport, which
    // clears any pending close request as well as showing the native window.
    let restore_event = unsafe { CreateEventW(None, false, false, RESTORE_EVENT) }.expect("restore event");
    // Keep this handle alive for the full eframe run so later launches find this instance.
    let instance = unsafe { CreateMutexW(None, true, INSTANCE_MUTEX) }.expect("instance mutex");
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe {
            let _ = SetEvent(restore_event);
            let _ = CloseHandle(restore_event);
            let _ = CloseHandle(instance);
        }
        return Ok(());
    }

    let mut store = store::Store::open();
    if !store.settings.clock_imported {
        match clock::import() {
            clock::Import::Found(studies) => {
                for study in studies {
                    store.add_study(study);
                }
                store.settings.clock_imported = true;
            }
            clock::Import::Nothing => store.settings.clock_imported = true,
            clock::Import::Busy => {}
        }
        store.save_settings();
    }
    autostart::refresh();
    let core = Arc::new(Mutex::new(core::Core::new(store)));

    let hidden = std::env::args().any(|a| a == "--hidden");
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Lernzeit")
            .with_inner_size([400.0, 720.0])
            .with_min_inner_size([360.0, 600.0])
            .with_icon(Arc::new(eframe::egui::IconData { rgba: icon::rgba(64), width: 64, height: 64 }))
            .with_visible(!hidden),
        ..Default::default()
    };
    let result = eframe::run_native("Lernzeit", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, core, restore_event)))));
    unsafe {
        let _ = CloseHandle(restore_event);
        let _ = CloseHandle(instance);
    }
    result
}
