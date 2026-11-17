//! The app icon, drawn in code: a red-pink disc with white clock hands.

/// RGBA pixels of the icon at `size` x `size`.
pub fn rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let c = s / 2.0;
    let (r, g, b) = (0xFA, 0x2D, 0x48);
    // Two hands from the centre: towards 12 and towards 4 o'clock.
    let hands = [((c, c), (c, c - s * 0.30)), ((c, c), (c + s * 0.22, c + s * 0.13))];
    let width = s * 0.085;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let p = (x as f32 + 0.5, y as f32 + 0.5);
            let disc = (c - 0.5 - ((p.0 - c).powi(2) + (p.1 - c).powi(2)).sqrt()).clamp(0.0, 1.0);
            let hand = hands.iter().map(|&(a, b)| distance(p, a, b)).fold(f32::MAX, f32::min);
            let mark = (width / 2.0 - hand + 0.5).clamp(0.0, 1.0);
            let mix = |v: u8| (v as f32 + (255.0 - v as f32) * mark) as u8;
            out.extend([mix(r), mix(g), mix(b), (disc * 255.0) as u8]);
        }
    }
    out
}

fn distance(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    ((p.0 - a.0 - t * dx).powi(2) + (p.1 - a.1 - t * dy).powi(2)).sqrt()
}
