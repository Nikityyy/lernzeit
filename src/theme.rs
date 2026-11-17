//! Colours, type and small drawn widgets, modelled on Apple's system design: a grouped grey
//! background, white cards with 12 px corners, one accent colour.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2,
};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct Theme {
    pub background: Color32,
    pub card: Color32,
    pub label: Color32,
    pub secondary: Color32,
    pub tertiary: Color32,
    pub separator: Color32,
    pub fill: Color32,
    pub fill_hover: Color32,
    pub selected: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub dark: bool,
}

impl Theme {
    pub fn for_mode(dark: bool) -> Self {
        if dark {
            Self {
                background: hex(0x000000),
                card: hex(0x1C1C1E),
                label: hex(0xFFFFFF),
                secondary: hex(0x98989F),
                tertiary: hex(0x5A5A5F),
                separator: hex(0x38383A),
                fill: hex(0x2C2C2E),
                fill_hover: hex(0x3A3A3C),
                selected: hex(0x636366),
                accent: hex(0xFC3C5A),
                accent_hover: hex(0xFF5470),
                dark,
            }
        } else {
            Self {
                background: hex(0xF2F2F7),
                card: hex(0xFFFFFF),
                label: hex(0x000000),
                secondary: hex(0x6E6E73),
                tertiary: hex(0xC7C7CC),
                separator: hex(0xE5E5EA),
                fill: hex(0xE8E8ED),
                fill_hover: hex(0xDCDCE1),
                selected: hex(0xFFFFFF),
                accent: hex(0xFA2D48),
                accent_hover: hex(0xE0223C),
                dark,
            }
        }
    }
}

pub fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

// Type scale (Segoe UI standing in for SF Pro): 34 / 20 / 15 / 13 / 11.
pub fn regular(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}

pub fn light(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("light".into()))
}

/// Uses Windows' own UI font. Falls back to egui's font if a file is missing.
pub fn install_fonts(ctx: &egui::Context) {
    let dir = std::env::var_os("WINDIR").map(std::path::PathBuf::from).unwrap_or_default().join("Fonts");
    ctx.set_fonts(font_definitions(&dir));
}

fn font_definitions(dir: &std::path::Path) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::empty();
    let fallback = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(
        include_bytes!("../assets/fonts/Ubuntu-Light.ttf.zlib"), 512 * 1024,
    ).expect("The bundled fallback font is invalid");
    fonts.font_data.insert("Ubuntu-Light".into(), Arc::new(egui::FontData::from_owned(fallback)));
    fonts.font_data.insert("egui-icons".into(), Arc::new(
        egui::FontData::from_static(epaint_default_fonts::EGUI_ICONS).tweak(egui::FontTweak {
            scale: 0.90, ..Default::default()
        }),
    ));
    let fallback = vec!["Ubuntu-Light".to_owned(), "egui-icons".to_owned()];
    fonts.families.insert(FontFamily::Proportional, fallback.clone());
    fonts.families.insert(FontFamily::Monospace, fallback);
    let mut load = |name: &str, file: &str| -> Option<String> {
        let bytes = std::fs::read(dir.join(file)).ok()?;
        fonts.font_data.insert(name.to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
        Some(name.to_owned())
    };
    let regular = load("segoe", "segoeui.ttf");
    let semibold = load("segoe-semibold", "seguisb.ttf");
    let light = load("segoe-light", "segoeuil.ttf");
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let family = |first: Option<String>| first.into_iter().chain(fallback.iter().cloned()).collect::<Vec<_>>();
    fonts.families.insert(FontFamily::Proportional, family(regular.clone()));
    fonts.families.insert(FontFamily::Name("semibold".into()), family(semibold.or(regular.clone())));
    fonts.families.insert(FontFamily::Name("light".into()), family(light.or(regular)));
    fonts
}

pub fn card(ui: &mut Ui, t: &Theme, add: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(t.card)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

/// Row metrics of an inset grouped list, as in iOS Settings.
pub const ROW_HEIGHT: f32 = 44.0;
pub const ROW_INSET: f32 = 16.0;

/// A card of tappable rows with no padding of its own: each row is exactly
/// `ROW_HEIGHT` tall, and the hairlines between rows run from the text to the card's edge.
pub fn list(ui: &mut Ui, t: &Theme, add: impl FnOnce(&mut Ui)) {
    egui::Frame::new().fill(t.card).corner_radius(CornerRadius::same(12)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        add(ui);
    });
}

/// Row `index` of `count` in a `list`. Returns the inset content rect and the response.
/// A pressed row darkens, rounded to match the card at the top and bottom.
pub fn row(ui: &mut Ui, t: &Theme, index: usize, count: usize, sense: Sense) -> (Rect, Response) {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_HEIGHT), sense);
    if sense.senses_click() && response.is_pointer_button_down_on() {
        let (top, bottom) = (if index == 0 { 12 } else { 0 }, if index + 1 == count { 12 } else { 0 });
        ui.painter().rect_filled(rect, CornerRadius { nw: top, ne: top, sw: bottom, se: bottom }, t.fill);
    }
    if index > 0 {
        let y = rect.top() + 0.5;
        ui.painter().line_segment([Pos2::new(rect.left() + ROW_INSET, y), Pos2::new(rect.right(), y)], Stroke::new(1.0, t.separator));
    }
    let response = if sense.senses_click() { response.on_hover_cursor(egui::CursorIcon::PointingHand) } else { response };
    (rect.shrink2(Vec2::new(ROW_INSET, 0.0)), response)
}

/// The grey chevron at the end of a row that opens something.
pub fn chevron(ui: &Ui, t: &Theme, right_center: Pos2) {
    let c = right_center - Vec2::new(1.0, 0.0);
    let stroke = Stroke::new(2.0, t.tertiary);
    ui.painter().line_segment([c + Vec2::new(-5.0, -6.0), c], stroke);
    ui.painter().line_segment([c, c + Vec2::new(-5.0, 6.0)], stroke);
}

/// A small grey heading above a card.
pub fn section(ui: &mut Ui, t: &Theme, text: &str) {
    // 24 above, 6 below: the heading belongs to the card under it.
    ui.add_space(24.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 16.0), Sense::hover());
    ui.painter().text(rect.left_center() + Vec2::new(ROW_INSET, 0.0), Align2::LEFT_CENTER, text, regular(13.0), t.secondary);
    ui.add_space(6.0);
}

/// Apple's segmented control. Returns the new selection.
pub fn segmented(ui: &mut Ui, t: &Theme, labels: &[&str], selected: usize, width: f32) -> usize {
    // One widget for the whole control; the pointer position picks the segment.
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 32.0), Sense::click());
    let w = rect.width() / labels.len() as f32;
    let segment_at = |p: Pos2| (((p.x - rect.left()) / w).max(0.0) as usize).min(labels.len() - 1);
    let chosen = match response.interact_pointer_pos() {
        Some(p) if response.clicked() => segment_at(p),
        _ => selected,
    };
    let hovered = response.hover_pos().map(segment_at);
    ui.painter().rect_filled(rect, 9, t.fill);
    for (i, label) in labels.iter().enumerate() {
        let seg = Rect::from_min_size(rect.min + Vec2::new(w * i as f32, 0.0), Vec2::new(w, rect.height()));
        if i == chosen {
            let inner = seg.shrink(2.0);
            shadow(ui, inner, 7, t);
            ui.painter().rect_filled(inner, 7, t.selected);
        }
        let font = if i == chosen { semibold(13.0) } else { regular(13.0) };
        let color = if i != chosen && hovered == Some(i) { t.secondary } else { t.label };
        ui.painter().text(seg.center(), Align2::CENTER_CENTER, *label, font, color);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand);
    chosen
}

/// A soft shadow under a raised element, as on iOS segmented controls.
pub fn shadow(ui: &Ui, rect: Rect, radius: u8, t: &Theme) {
    if t.dark {
        return;
    }
    for (spread, alpha) in [(0.5, 18u8), (1.5, 8)] {
        ui.painter().rect_filled(rect.expand(spread).translate(Vec2::new(0.0, 1.0)), radius + 1, Color32::from_black_alpha(alpha));
    }
}

/// Apple's switch: 51 x 31, white knob, in the accent colour when on.
pub fn switch(ui: &mut Ui, t: &Theme, on: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(51.0, 31.0), Sense::click());
    let how_on = ui.ctx().animate_bool_with_time(response.id, on, 0.18);
    let track = lerp(if t.dark { hex(0x39393D) } else { hex(0xE9E9EA) }, t.accent, how_on);
    ui.painter().rect_filled(rect, 15.5, track);
    let x = egui::lerp(rect.left() + 15.5..=rect.right() - 15.5, how_on);
    let knob = Pos2::new(x, rect.center().y);
    ui.painter().circle_filled(knob + Vec2::new(0.0, 1.5), 13.5, Color32::from_black_alpha(25));
    ui.painter().circle_filled(knob, 13.5, Color32::WHITE);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A filled circle with a play or pause glyph.
pub fn play_button(ui: &mut Ui, diameter: f32, fill: Color32, hover: Color32, glyph: Color32, playing: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(diameter), Sense::click());
    let pressed = response.is_pointer_button_down_on();
    let scale = ui.ctx().animate_value_with_time(response.id, if pressed { 0.94 } else { 1.0 }, 0.08);
    let r = diameter / 2.0 * scale;
    let c = rect.center();
    ui.painter().circle_filled(c, r, if response.hovered() { hover } else { fill });
    let s = r * 0.36;
    if playing {
        // Two rounded bars.
        let (w, h, gap) = (s * 0.52, s * 1.7, s * 0.42);
        for dx in [-(gap / 2.0 + w / 2.0), gap / 2.0 + w / 2.0] {
            let bar = Rect::from_center_size(c + Vec2::new(dx, 0.0), Vec2::new(w, h));
            ui.painter().rect_filled(bar, (w / 2.5) as u8, glyph);
        }
    } else {
        // A triangle, nudged right so its visual centre sits on the circle's centre.
        let nudge = s * 0.18;
        let points = vec![
            c + Vec2::new(-s * 0.72 + nudge, -s * 0.95),
            c + Vec2::new(s * 0.95 + nudge, 0.0),
            c + Vec2::new(-s * 0.72 + nudge, s * 0.95),
        ];
        ui.painter().add(Shape::convex_polygon(points, glyph, Stroke::new(s * 0.18, glyph)));
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A gear, drawn: a ring with eight teeth.
pub fn gear_button(ui: &mut Ui, t: &Theme) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::click());
    if response.hovered() {
        ui.painter().circle_filled(rect.center(), 16.0, t.fill);
    }
    let c = rect.center();
    let color = t.secondary;
    for i in 0..8 {
        let angle = i as f32 * std::f32::consts::TAU / 8.0;
        let dir = Vec2::angled(angle);
        ui.painter().line_segment([c + dir * 6.5, c + dir * 9.0], Stroke::new(3.0, color));
    }
    ui.painter().circle_stroke(c, 5.8, Stroke::new(2.4, color));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// "‹ Zurück", in the accent colour like an iOS back button.
pub fn back_button(ui: &mut Ui, t: &Theme) -> Response {
    let text = ui.painter().layout_no_wrap("Zurück".into(), regular(15.0), t.accent);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(text.size().x + 16.0, 32.0), Sense::click());
    let color = if response.hovered() { t.accent_hover } else { t.accent };
    let y = rect.center().y;
    let x = rect.left() + 2.0;
    ui.painter().line_segment([Pos2::new(x + 6.0, y - 6.0), Pos2::new(x, y)], Stroke::new(2.2, color));
    ui.painter().line_segment([Pos2::new(x, y), Pos2::new(x + 6.0, y + 6.0)], Stroke::new(2.2, color));
    ui.painter().text(Pos2::new(x + 14.0, y), Align2::LEFT_CENTER, "Zurück", regular(15.0), color);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A thin line between rows of a card.
pub fn separator(ui: &mut Ui, t: &Theme, indent: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().line_segment(
        [Pos2::new(rect.left() + indent, rect.center().y), Pos2::new(rect.right(), rect.center().y)],
        Stroke::new(1.0, t.separator),
    );
}

/// A cover image with rounded corners, or a grey square with a note when there is none.
pub fn cover(ui: &mut Ui, t: &Theme, texture: Option<&egui::TextureHandle>, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let radius = (size / 7.0) as u8;
    match texture {
        Some(tex) => {
            let shape = egui::epaint::RectShape::filled(rect, radius, Color32::WHITE)
                .with_texture(tex.id(), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)));
            ui.painter().add(shape);
        }
        None => {
            ui.painter().rect_filled(rect, radius, t.fill);
            // A music note.
            let c = rect.center();
            let s = size / 40.0;
            ui.painter().circle_filled(c + Vec2::new(-3.0, 5.0) * s, 3.4 * s, t.tertiary);
            ui.painter().line_segment([c + Vec2::new(0.2, 5.0) * s, c + Vec2::new(0.2, -7.0) * s], Stroke::new(1.8 * s, t.tertiary));
            ui.painter().line_segment([c + Vec2::new(0.2, -7.0) * s, c + Vec2::new(5.5, -5.0) * s], Stroke::new(1.8 * s, t.tertiary));
        }
    }
}

/// Text cut with "…" so it fits in `width`.
pub fn fit(ui: &Ui, text: &str, font: FontId, width: f32) -> String {
    let measure = |s: &str| ui.painter().layout_no_wrap(s.to_owned(), font.clone(), Color32::WHITE).size().x;
    if measure(text) <= width {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let candidate: String = chars[..mid].iter().collect::<String>() + "…";
        if measure(&candidate) <= width { lo = mid } else { hi = mid - 1 }
    }
    chars[..lo].iter().collect::<String>().trim_end().to_owned() + "…"
}


#[cfg(test)]
mod font_tests {
    use super::*;

    #[test]
    fn bundled_fallback_renders_without_windows_fonts() {
        // A machine without the Segoe files must still have readable controls.
        let fonts = font_definitions(std::path::Path::new(""));
        let context = egui::Context::default();
        context.set_fonts(fonts);
        let mut output = context.run_pass(Default::default(), |ctx| {
            ctx.fonts_mut(|fonts| {
                for font in [regular(15.0), semibold(15.0), light(34.0), FontId::monospace(13.0)] {
                    let text = "Lernzeit · Zurück · ÄÖÜ äöü ß · 12:34".to_owned();
                    assert!(fonts.has_glyphs(&font, &text), "Missing glyphs in {font:?}");
                    let galley = fonts.layout_no_wrap(text.clone(), font, Color32::WHITE);
                    assert_eq!(galley.text(), text);
                    assert!(galley.size().x > 100.0 && galley.size().y > 10.0);
                }
            });
        });
        // This headless check has no GPU to consume the generated font texture.
        output.textures_delta.clear();
    }
}
