//! The window: the timer, the statistics and the settings.

use crate::core::{Command, Core, PausedBy, Run};
use crate::spotify::{now, Remote};
use crate::stats::{self, clock, hours_minutes, Entry};
use crate::store::{Ms, SpaceMode};
use crate::theme::{self, regular, semibold, Theme};
use crate::{autostart, tray};
use chrono::{Datelike, Duration, NaiveDate};
use eframe::egui::{self, Align2, Color32, Key, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{OpenEventW, SetEvent, WaitForSingleObject, INFINITE, SYNCHRONIZATION_SYNCHRONIZE};

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Timer,
    Stats,
    Settings,
}

#[derive(Clone, Copy, PartialEq)]
enum Range {
    Week,
    Month,
    Year,
}

pub struct App {
    core: Arc<Mutex<Core>>,
    remote: Remote,
    page: Page,
    back_to: Page,
    range: Range,
    covers: HashMap<String, egui::TextureHandle>,
    quitting: Arc<AtomicBool>,
    tray: tray::Tray,
    tooltip: String,
    /// Heights of the timer and of the cards under it, measured in the previous frame.
    timer_height: f32,
    bottom_height: f32,
    pending_music: Option<(bool, std::time::Instant)>,
    restore_event: HANDLE,
    restore_listener: Option<std::thread::JoinHandle<()>>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext, core: Arc<Mutex<Core>>, restore_event: HANDLE) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let remote = crate::spotify::start(core.clone(), move || ctx.request_repaint());
        let quitting = Arc::new(AtomicBool::new(false));
        let tray = tray::Tray::new(cc.egui_ctx.clone(), core.clone(), remote.clone(), quitting.clone());
        let restore_context = cc.egui_ctx.clone();
        let restore_quitting = quitting.clone();
        let restore_listener = std::thread::spawn(move || {
            let Ok(event) = (unsafe { OpenEventW(SYNCHRONIZATION_SYNCHRONIZE, false, crate::RESTORE_EVENT) }) else {
                return;
            };
            loop {
                if unsafe { WaitForSingleObject(event, INFINITE) } != WAIT_OBJECT_0 {
                    break;
                }
                if restore_quitting.load(Ordering::Relaxed) {
                    break;
                }
                tray::restore_window(&restore_context);
            }
            unsafe { let _ = CloseHandle(event); }
        });
        Self {
            core,
            remote,
            page: Page::Timer,
            back_to: Page::Timer,
            range: Range::Week,
            covers: HashMap::new(),
            quitting,
            tray,
            tooltip: String::new(),
            timer_height: 230.0,
            bottom_height: 220.0,
            pending_music: None,
            restore_event,
            restore_listener: Some(restore_listener),
        }
    }

    fn send(&mut self, command: Option<Command>) {
        if let Some(command) = command {
            self.pending_music = Some((command == Command::Play, std::time::Instant::now() + std::time::Duration::from_secs(4)));
            self.remote.send(command);
        }
    }

    /// The cover art for an artist or show and album, once Spotify has delivered it.
    fn cover(&mut self, ctx: &egui::Context, author: &str, album: &str) -> Option<egui::TextureHandle> {
        let key = crate::core::cover_key(author, album);
        if let Some(texture) = self.covers.get(&key) {
            return Some(texture.clone());
        }
        let path = self.core.lock().unwrap().store.cover_path(&key);
        let image = image::load_from_memory(&std::fs::read(path).ok()?).ok()?.to_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        let texture = ctx.load_texture(&key, egui::ColorImage::from_rgba_unmultiplied(size, &image), egui::TextureOptions::LINEAR);
        self.covers.insert(key, texture.clone());
        Some(texture)
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let t = Theme::for_mode(ui.visuals().dark_mode);
        let now = now();

        // Closing the window keeps Lernzeit running in the tray.
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting.load(Ordering::Relaxed) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        let actual_playing = self.core.lock().unwrap().playing;
        if self.pending_music.is_some_and(|(wanted, until)| actual_playing == wanted || std::time::Instant::now() >= until) {
            self.pending_music = None;
        }
        if self.page != Page::Settings && ctx.input(|i| i.key_pressed(Key::Space)) {
            let command = self.core.lock().unwrap().toggle(now);
            self.send(command);
        }

        let (running, session) = {
            let mut core = self.core.lock().unwrap();
            core.tick(now);
            (core.running_since().is_some(), core.session(now))
        };
        if running {
            ctx.request_repaint_after(std::time::Duration::from_millis((1000 - session % 1000) as u64 + 5));
        }
        let tooltip = if running { format!("Lernzeit · {}", clock(session)) } else { "Lernzeit".to_owned() };
        if tooltip != self.tooltip {
            self.tray.set_tooltip(&tooltip);
            self.tooltip = tooltip;
        }

        egui::Frame::new().fill(t.background).inner_margin(egui::Margin::symmetric(16, 12)).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            self.top_bar(ui, &t);
            ui.add_space(12.0);
            match self.page {
                Page::Timer => self.timer_page(ui, &t, now),
                Page::Stats => self.stats_page(ui, &t, now),
                Page::Settings => self.settings_page(ui, &t),
            }
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.quitting.store(true, Ordering::Relaxed);
        unsafe { let _ = SetEvent(self.restore_event); }
        if let Some(listener) = self.restore_listener.take() {
            let _ = listener.join();
        }
        self.core.lock().unwrap().shutdown(now());
    }
}

impl App {
    fn top_bar(&mut self, ui: &mut Ui, t: &Theme) {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), Sense::hover());
        let mut bar = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
        if self.page == Page::Settings {
            if theme::back_button(&mut bar, t).clicked() {
                self.page = self.back_to;
            }
            bar.painter().text(rect.center(), Align2::CENTER_CENTER, "Einstellungen", semibold(15.0), t.label);
            return;
        }
        let width = 200.0;
        let mut middle = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_center_size(rect.center(), Vec2::new(width, 32.0))));
        let current = if self.page == Page::Timer { 0 } else { 1 };
        let chosen = theme::segmented(&mut middle, t, &["Timer", "Statistik"], current, width);
        self.page = if chosen == 0 { Page::Timer } else { Page::Stats };
        let mut right = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(rect.right_top() - Vec2::new(32.0, 0.0), rect.max)));
        if theme::gear_button(&mut right, t).on_hover_text("Einstellungen").clicked() {
            self.back_to = self.page;
            self.page = Page::Settings;
        }
    }

    // ---------------------------------------------------------------- Timer

    fn timer_page(&mut self, ui: &mut Ui, t: &Theme, now: Ms) {
        let core = self.core.lock().unwrap();
        let run = core.run;
        let session = core.session(now);
        let open = core.running_since().map(|since| (since, now));
        let days = stats::per_day(&core.store.study, open);
        let space = core.store.settings.space;
        let spotify = core.spotify_found;
        let playing = core.playing;
        let track = core.track.clone();
        drop(core);

        let playing = self.pending_music.map_or(playing, |(wanted, _)| wanted);

        let today = stats::study_day(now);
        let week: Ms = (0..7).map(|i| days.get(&(stats::monday(today) + Duration::days(i))).copied().unwrap_or(0)).sum();

        // The cards sit at the bottom; the timer is centred in the space above them.
        // Both heights are measured in the previous frame.
        let full = ui.available_rect_before_wrap();
        let bottom = Rect::from_min_max(Pos2::new(full.left(), full.bottom() - self.bottom_height), full.max);
        let top = Rect::from_min_max(full.min, Pos2::new(full.right(), bottom.top()));
        ui.allocate_rect(full, Sense::hover());

        let mut timer = ui.new_child(egui::UiBuilder::new().max_rect(top).layout(egui::Layout::top_down(egui::Align::Center)));
        timer.add_space(((top.height() - self.timer_height) / 2.0 - 16.0).max(0.0));
        let timer_top = timer.cursor().top();
        let status = match run {
            Run::Stopped => "Bereit",
            Run::Running { .. } => "Läuft",
            Run::Paused { by: PausedBy::You } => "Pausiert",
            Run::Paused { by: PausedBy::Spotify } => "Pausiert · Musik aus",
        };
        timer.label(egui::RichText::new(status).font(regular(15.0)).color(t.secondary));
        big_clock(&mut timer, t, &clock(session), matches!(run, Run::Running { .. }));
        timer.add_space(20.0);
        let is_running = matches!(run, Run::Running { .. });
        let label = if is_running { "Pausieren (Leertaste)" } else { "Starten (Leertaste)" };
        if theme::play_button(&mut timer, 76.0, t.accent, t.accent_hover, Color32::WHITE, is_running).on_hover_text(label).clicked() {
            let command = self.core.lock().unwrap().toggle(now);
            self.send(command);
        }
        timer.add_space(8.0);
        // Always takes its space, so the layout stays still when it appears.
        let (rect, response) = timer.allocate_exact_size(Vec2::new(120.0, 28.0), Sense::click());
        if run != Run::Stopped {
            let color = if response.hovered() { t.label } else { t.secondary };
            timer.painter().text(rect.center(), Align2::CENTER_CENTER, "Beenden", regular(15.0), color);
            if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.core.lock().unwrap().stop(now);
            }
        }
        let timer_height = timer.cursor().top() - timer_top;

        let mut cards = ui.new_child(egui::UiBuilder::new().max_rect(bottom));
        theme::card(&mut cards, t, |ui| {
            ui.columns(2, |cols| {
                for (col, (label, ms)) in cols.iter_mut().zip([("Heute", days.get(&today).copied().unwrap_or(0)), ("Diese Woche", week)]) {
                    col.label(egui::RichText::new(label).font(regular(13.0)).color(t.secondary));
                    col.label(egui::RichText::new(hours_minutes(ms)).font(semibold(20.0)).color(t.label));
                }
            });
        });
        cards.add_space(12.0);
        theme::card(&mut cards, t, |ui| {
            ui.horizontal(|ui| {
                let texture = track.as_ref().and_then(|tr| self.cover(ui.ctx(), tr.author(), &tr.album));
                theme::cover(ui, t, texture.as_ref(), 52.0);
                ui.add_space(12.0);
                let text_width = ui.available_width() - 36.0 - 12.0;
                let (title, sub) = match (&track, spotify) {
                    (Some(tr), _) => (tr.title.clone(), tr.author().to_owned()),
                    (None, true) => ("Spotify".to_owned(), "Nichts ausgewählt".to_owned()),
                    (None, false) => ("Spotify ist nicht geöffnet".to_owned(), "Starte die Spotify-App".to_owned()),
                };
                ui.vertical(|ui| {
                    ui.add_space(7.0);
                    ui.label(egui::RichText::new(theme::fit(ui, &title, semibold(15.0), text_width)).font(semibold(15.0)).color(t.label));
                    ui.label(egui::RichText::new(theme::fit(ui, &sub, regular(13.0), text_width)).font(regular(13.0)).color(t.secondary));
                });
                if spotify {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let tip = if playing { "Nur Musik pausieren" } else { "Nur Musik abspielen" };
                        if theme::play_button(ui, 36.0, t.fill, t.fill_hover, t.label, playing).on_hover_text(tip).clicked() {
                            let command = self.core.lock().unwrap().toggle_music(now);
                            self.send(command);
                        }
                    });
                }
            });
        });
        cards.add_space(10.0);
        let hint = match space {
            SpaceMode::Both => "Leertaste pausiert Timer und Musik",
            SpaceMode::TimerOnly => "Leertaste pausiert nur den Timer",
            SpaceMode::MusicOnly => "Leertaste pausiert nur die Musik",
        };
        cards.vertical_centered(|ui| ui.label(egui::RichText::new(hint).font(regular(12.0)).color(t.secondary)));
        let bottom_height = cards.min_rect().height();

        if (bottom_height - self.bottom_height).abs() > 0.5 || (timer_height - self.timer_height).abs() > 0.5 {
            self.bottom_height = bottom_height;
            self.timer_height = timer_height;
            ui.ctx().request_discard("timer page layout measured");
        }
    }

    // ---------------------------------------------------------------- Stats

    fn stats_page(&mut self, ui: &mut Ui, t: &Theme, now: Ms) {
        let (days, today, hours_from_to, top, year_days);
        let range = self.range;
        {
            let core = self.core.lock().unwrap();
            let open = core.running_since().map(|since| (since, now));
            days = stats::per_day(&core.store.study, open);
            today = stats::study_day(now);
            let (from, to) = bounds(range, today);
            hours_from_to = stats::per_hour(&core.store.study, open, from, to);
            top = stats::top(&core.store.listening, from, to, 5);
            year_days = (from, to);
        }
        let (from, to) = year_days;

        let width = ui.available_width();
        let chosen = theme::segmented(ui, t, &["Woche", "Monat", "Jahr"], range as usize, width);
        self.range = [Range::Week, Range::Month, Range::Year][chosen];
        ui.add_space(12.0);

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let total: Ms = days.range(from..to).map(|(_, ms)| ms).sum();
            let elapsed = ((today.min(to - Duration::days(1)) - from).num_days() + 1).max(1);
            theme::card(ui, t, |ui| {
                let title = match range {
                    Range::Week => "Diese Woche",
                    Range::Month => "Dieser Monat",
                    Range::Year => "Dieses Jahr",
                };
                ui.label(egui::RichText::new(title).font(regular(13.0)).color(t.secondary));
                ui.label(egui::RichText::new(hours_minutes(total)).font(semibold(28.0)).color(t.label));
                ui.label(egui::RichText::new(format!("Ø {} pro Tag", hours_minutes(total / elapsed))).font(regular(13.0)).color(t.secondary));
                ui.add_space(14.0);
                match range {
                    Range::Week => {
                        let this: Vec<Ms> = (0..7).map(|i| days.get(&(from + Duration::days(i))).copied().unwrap_or(0)).collect();
                        let last: Vec<Ms> = (0..7).map(|i| days.get(&(from - Duration::days(7 - i))).copied().unwrap_or(0)).collect();
                        let labels = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];
                        let today_index = (today - from).num_days() as usize;
                        bars(ui, t, &this, Some(&last), &|i| Some(labels[i].to_owned()), Some(today_index));
                        ui.add_space(8.0);
                        legend(ui, t);
                    }
                    Range::Month => {
                        let n = (to - from).num_days() as usize;
                        let values: Vec<Ms> = (0..n).map(|i| days.get(&(from + Duration::days(i as i64))).copied().unwrap_or(0)).collect();
                        let today_index = (today - from).num_days() as usize;
                        bars(ui, t, &values, None, &|i| [0, 7, 14, 21, 28].contains(&i).then(|| (i + 1).to_string()), Some(today_index));
                    }
                    Range::Year => {
                        // One bar per month, like the year view in Apple Health.
                        let months: Vec<Ms> = (1..=12)
                            .map(|m| {
                                let start = NaiveDate::from_ymd_opt(from.year(), m, 1).unwrap_or(from);
                                days.range(start..stats::next_month(start)).map(|(_, ms)| ms).sum()
                            })
                            .collect();
                        let initials = ["J", "F", "M", "A", "M", "J", "J", "A", "S", "O", "N", "D"];
                        bars(ui, t, &months, None, &|i| Some(initials[i].to_owned()), Some(today.month0() as usize));
                    }
                }
            });

            theme::section(ui, t, "TAGESZEIT");
            theme::card(ui, t, |ui| {
                let total: Ms = hours_from_to.iter().sum();
                if total == 0 {
                    empty(ui, t, "Noch keine Lernzeit in diesem Zeitraum.");
                } else {
                    let peak = (0..24).max_by_key(|&h| hours_from_to[h] + hours_from_to[(h + 1) % 24]).unwrap_or(0);
                    ui.label(egui::RichText::new(format!("Am meisten zwischen {} und {} Uhr", peak, (peak + 2) % 24)).font(regular(15.0)).color(t.label));
                    ui.add_space(12.0);
                    bars(ui, t, &hours_from_to, None, &|i| (i % 6 == 0).then(|| format!("{i}")), None);
                }
            });

            for (title, entries, empty_text) in [
                ("SONGS", &top.songs, "Noch keine Songs beim Lernen."),
                ("KÜNSTLER", &top.artists, "Noch keine Künstler beim Lernen."),
                ("PODCASTS", &top.podcasts, "Noch keine Podcasts beim Lernen."),
            ] {
                theme::section(ui, t, title);
                theme::card(ui, t, |ui| {
                    if entries.is_empty() {
                        empty(ui, t, empty_text);
                    }
                    for (i, entry) in entries.iter().enumerate() {
                        if i > 0 {
                            ui.add_space(8.0);
                            theme::separator(ui, t, 52.0);
                            ui.add_space(8.0);
                        }
                        self.entry_row(ui, t, entry);
                    }
                });
            }
            ui.add_space(16.0);
        });
    }

    fn entry_row(&mut self, ui: &mut Ui, t: &Theme, entry: &Entry) {
        ui.horizontal(|ui| {
            let texture = self.cover(ui.ctx(), &entry.author, &entry.album);
            theme::cover(ui, t, texture.as_ref(), 40.0);
            ui.add_space(12.0);
            let minutes = hours_minutes(entry.ms);
            let width = ui.available_width() - ui.painter().layout_no_wrap(minutes.clone(), regular(13.0), t.secondary).size().x - 12.0;
            ui.vertical(|ui| {
                if entry.subtitle.is_empty() {
                    ui.add_space(10.0);
                }
                ui.label(egui::RichText::new(theme::fit(ui, &entry.title, regular(15.0), width)).font(regular(15.0)).color(t.label));
                if !entry.subtitle.is_empty() {
                    ui.label(egui::RichText::new(theme::fit(ui, &entry.subtitle, regular(13.0), width)).font(regular(13.0)).color(t.secondary));
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(minutes).font(regular(13.0)).color(t.secondary));
            });
        });
    }

    // ---------------------------------------------------------------- Settings

    fn settings_page(&mut self, ui: &mut Ui, t: &Theme) {
        let mut core = self.core.lock().unwrap();
        theme::section(ui, t, "LEERTASTE PAUSIERT");
        theme::list(ui, t, |ui| {
            let options = [
                (SpaceMode::Both, "Timer und Musik"),
                (SpaceMode::TimerOnly, "Nur den Timer"),
                (SpaceMode::MusicOnly, "Nur die Musik"),
            ];
            for (i, (mode, label)) in options.into_iter().enumerate() {
                let (rect, response) = theme::row(ui, t, i, options.len(), Sense::click());
                ui.painter().text(rect.left_center(), Align2::LEFT_CENTER, label, regular(15.0), t.label);
                if core.store.settings.space == mode {
                    check(ui, rect.right_center(), t.accent);
                }
                if response.clicked() {
                    core.store.settings.space = mode;
                    core.store.save_settings();
                }
            }
        });
        let dir = core.store.dir().to_path_buf();
        drop(core);

        theme::section(ui, t, "ALLGEMEIN");
        theme::list(ui, t, |ui| {
            let (rect, _) = theme::row(ui, t, 0, 1, Sense::hover());
            ui.painter().text(rect.left_center(), Align2::LEFT_CENTER, "Mit Windows starten", regular(15.0), t.label);
            let knob = Rect::from_min_max(rect.right_center() - Vec2::new(51.0, 15.5), rect.right_center() + Vec2::new(0.0, 15.5));
            let mut slot = ui.new_child(egui::UiBuilder::new().max_rect(knob));
            let on = autostart::enabled();
            if theme::switch(&mut slot, t, on).clicked() {
                autostart::set(!on);
            }
        });

        theme::section(ui, t, "DATEN");
        theme::list(ui, t, |ui| {
            let (rect, response) = theme::row(ui, t, 0, 1, Sense::click());
            ui.painter().text(rect.left_center(), Align2::LEFT_CENTER, "Datenordner öffnen", regular(15.0), t.label);
            theme::chevron(ui, t, rect.right_center());
            if response.clicked() {
                let _ = std::process::Command::new("explorer").arg(&dir).spawn();
            }
        });
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new(concat!("Lernzeit ", env!("CARGO_PKG_VERSION"))).font(regular(12.0)).color(t.secondary));
        });
    }
}

/// The days the range covers: `from` inclusive, `to` exclusive.
fn bounds(range: Range, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    match range {
        Range::Week => {
            let monday = stats::monday(today);
            (monday, monday + Duration::days(7))
        }
        Range::Month => (stats::first_of_month(today), stats::next_month(today)),
        Range::Year => {
            let first = NaiveDate::from_ymd_opt(today.year(), 1, 1).unwrap_or(today);
            (first, NaiveDate::from_ymd_opt(today.year() + 1, 1, 1).unwrap_or(today))
        }
    }
}

/// The session time in thin, large digits. Every digit gets the same width, so the
/// numbers do not shift as they change.
fn big_clock(ui: &mut Ui, t: &Theme, text: &str, running: bool) {
    let font = theme::light(68.0);
    let painter = ui.painter();
    let digit = (0..10)
        .map(|d| painter.layout_no_wrap(d.to_string(), font.clone(), t.label).size().x)
        .fold(0.0, f32::max);
    let colon = painter.layout_no_wrap(":".into(), font.clone(), t.label).size().x;
    let width: f32 = text.chars().map(|c| if c == ':' { colon } else { digit }).sum();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 80.0), Sense::hover());
    let color = if running { t.label } else { t.secondary };
    let mut x = rect.left();
    for c in text.chars() {
        let w = if c == ':' { colon } else { digit };
        ui.painter().text(Pos2::new(x + w / 2.0, rect.center().y), Align2::CENTER_CENTER, c, font.clone(), color);
        x += w;
    }
}

/// A bar chart. `ghost` draws faint bars behind, for the week before. `label` names
/// some bars below the chart; `highlight` marks today.
fn bars(ui: &mut Ui, t: &Theme, values: &[Ms], ghost: Option<&[Ms]>, label: &dyn Fn(usize) -> Option<String>, highlight: Option<usize>) {
    let height = 140.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height + 22.0), Sense::hover());
    let chart = Rect::from_min_size(rect.min, Vec2::new(rect.width(), height));
    let max = values.iter().chain(ghost.into_iter().flatten()).copied().max().unwrap_or(0);
    // The scale rounds up to whole hours (or 15 minutes for short charts).
    let step = if max <= 3_600_000 { 900_000 } else { 3_600_000 };
    let top = ((max + step - 1) / step).max(1) * step;
    let painter = ui.painter();
    painter.line_segment([chart.left_top(), chart.right_top()], Stroke::new(1.0, t.separator));
    painter.line_segment([chart.left_bottom(), chart.right_bottom()], Stroke::new(1.0, t.separator));
    painter.text(chart.right_top() + Vec2::new(0.0, 3.0), Align2::RIGHT_TOP, hours_minutes(top), regular(11.0), t.secondary);

    let n = values.len();
    let slot = chart.width() / n as f32;
    let bar_width = (slot * 0.56).clamp(2.0, 28.0);
    let radius = (bar_width / 4.0).min(5.0) as u8;
    let bar = |i: usize, ms: Ms| {
        let h = (ms as f32 / top as f32 * (height - 22.0)).max(if ms > 0 { 2.0 } else { 0.0 });
        let x = chart.left() + slot * (i as f32 + 0.5);
        Rect::from_min_max(Pos2::new(x - bar_width / 2.0, chart.bottom() - h), Pos2::new(x + bar_width / 2.0, chart.bottom()))
    };
    let mut hovered = None;
    for i in 0..n {
        if let Some(ghost) = ghost {
            painter.rect_filled(bar(i, ghost[i]), radius, t.fill);
        }
        let color = if highlight.is_some_and(|h| i > h) { t.tertiary } else { t.accent };
        painter.rect_filled(bar(i, values[i]), radius, color);
        let x = chart.left() + slot * (i as f32 + 0.5);
        if let Some(text) = label(i) {
            let strong = highlight == Some(i);
            let font = if strong { semibold(11.0) } else { regular(11.0) };
            painter.text(Pos2::new(x, rect.bottom()), Align2::CENTER_BOTTOM, text, font, if strong { t.label } else { t.secondary });
        }
        if let Some(pointer) = response.hover_pos() {
            if (pointer.x - x).abs() <= slot / 2.0 {
                hovered = Some(i);
            }
        }
    }
    if let Some(i) = hovered {
        let mut text = hours_minutes(values[i]);
        if let Some(ghost) = ghost {
            text += &format!("\nVorwoche: {}", hours_minutes(ghost[i]));
        }
        response.on_hover_text(text);
    }
}

/// "■ Diese Woche  ■ Vorwoche" under the week chart.
fn legend(ui: &mut Ui, t: &Theme) {
    ui.horizontal(|ui| {
        for (color, text) in [(t.accent, "Diese Woche"), (t.fill, "Vorwoche")] {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
            ui.painter().rect_filled(rect, 3, color);
            ui.label(egui::RichText::new(text).font(regular(12.0)).color(t.secondary));
            ui.add_space(10.0);
        }
    });
}

fn empty(ui: &mut Ui, t: &Theme, text: &str) {
    ui.label(egui::RichText::new(text).font(regular(15.0)).color(t.secondary));
}

fn check(ui: &Ui, at: Pos2, color: Color32) {
    let stroke = Stroke::new(2.2, color);
    ui.painter().line_segment([at + Vec2::new(-12.0, 0.0), at + Vec2::new(-7.5, 4.5)], stroke);
    ui.painter().line_segment([at + Vec2::new(-7.5, 4.5), at + Vec2::new(0.0, -5.5)], stroke);
}
