//! The tray icon. A click opens the window; the menu opens, pauses or quits.

use crate::core::Core;
use crate::spotify::{now, Remote};
use eframe::egui::{self, ViewportCommand};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub struct Tray {
    icon: Option<TrayIcon>,
}

impl Tray {
    pub fn new(ctx: egui::Context, core: Arc<Mutex<Core>>, remote: Remote, quitting: Arc<AtomicBool>) -> Self {
        let open = MenuItem::new("Lernzeit öffnen", true, None);
        let toggle = MenuItem::new("Start / Pause", true, None);
        let quit = MenuItem::new("Beenden", true, None);
        let menu = Menu::new();
        let _ = menu.append_items(&[&open, &toggle, &PredefinedMenuItem::separator(), &quit]);

        let show = {
            let ctx = ctx.clone();
            move || restore_window(&ctx)
        };
        let on_click = show.clone();
        TrayIconEvent::set_event_handler(Some(move |event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                on_click();
            }
        }));
        let (open_id, toggle_id, quit_id) = (open.id().clone(), toggle.id().clone(), quit.id().clone());
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if event.id == open_id {
                show();
            } else if event.id == toggle_id {
                if let Some(command) = core.lock().unwrap().toggle(now()) {
                    remote.send(command);
                }
                ctx.request_repaint();
            } else if event.id == quit_id {
                quitting.store(true, Ordering::Relaxed);
                ctx.send_viewport_cmd(ViewportCommand::Close);
                ctx.request_repaint();
            }
        }));

        let icon = TrayIconBuilder::new()
            .with_icon(Icon::from_rgba(crate::icon::rgba(32), 32, 32).expect("icon"))
            .with_tooltip("Lernzeit")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()
            .ok();
        Self { icon }
    }

    pub fn set_tooltip(&self, text: &str) {
        if let Some(icon) = &self.icon {
            let _ = icon.set_tooltip(Some(text));
        }
    }
}

pub fn restore_window(ctx: &egui::Context) {
    // Clear any close request left by the previous hide-to-tray cycle.
    ctx.send_viewport_cmd(ViewportCommand::CancelClose);
    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(ViewportCommand::Focus);
    ctx.request_repaint();
}
