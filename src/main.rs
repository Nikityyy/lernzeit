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
use windows::core::w;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW};

fn main() -> eframe::Result {
    // One copy at a time; a second launch brings the first one forward.
    unsafe {
        let _instance = CreateMutexW(None, true, w!("Local\\Lernzeit"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            if let Ok(window) = FindWindowW(None, w!("Lernzeit")) {
                let _ = ShowWindow(window, SW_SHOW);
                let _ = ShowWindow(window, SW_RESTORE);
                let _ = SetForegroundWindow(window);
            }
            return Ok(());
        }
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
    eframe::run_native("Lernzeit", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, core)))))
}
