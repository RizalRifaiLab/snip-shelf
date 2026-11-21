//! Draws the app icon (Dock, Spotlight, Applications) and the small menu-bar icon in code.

fn sd_round_rect(px: f32, py: f32, cx: f32, cy: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let qx = (px - cx).abs() - hw + r;
    let qy = (py - cy).abs() - hh + r;
    let ox = qx.max(0.0);
    let oy = qy.max(0.0);
    (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) - r
}

fn cov(d: f32) -> f32 {
    (0.5 - d).clamp(0.0, 1.0)
}

fn blend(px: &mut [f32; 4], c: [f32; 3], a: f32) {
    let out_a = a + px[3] * (1.0 - a);
    if out_a <= 0.0 {
        return;
    }
    for i in 0..3 {
        px[i] = (c[i] * a + px[i] * px[3] * (1.0 - a)) / out_a;
    }
    px[3] = out_a;
}

fn to_rgba(buf: Vec<[f32; 4]>) -> Vec<u8> {
    buf.iter()
        .flat_map(|p| {
            [
                (p[0] * 255.0).round() as u8,
                (p[1] * 255.0).round() as u8,
                (p[2] * 255.0).round() as u8,
                (p[3] * 255.0).round() as u8,
            ]
        })
        .collect()
}

/// The colourful app icon: a purple tile with a stack of history cards.
pub fn app_icon(n: u32) -> Vec<u8> {
    let s = n as f32;
    let mut buf = vec![[0.0f32; 4]; (n * n) as usize];
    // macOS icons leave a margin around the tile.
    let tile = s * 0.805;
    let c = s / 2.0;
    for y in 0..n {
        for x in 0..n {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let p = &mut buf[(y * n + x) as usize];
            // Soft shadow.
            let ds = sd_round_rect(px, py - s * 0.012, c, c, tile / 2.0, tile / 2.0, tile * 0.225);
            blend(p, [0.0, 0.0, 0.0], (0.35 * (1.0 - (ds / (s * 0.02)).clamp(0.0, 1.0))).max(0.0) * if ds > 0.0 { 1.0 } else { 0.0 });
            // Tile with a vertical gradient.
            let d = sd_round_rect(px, py, c, c, tile / 2.0, tile / 2.0, tile * 0.225);
            let t = ((py - (c - tile / 2.0)) / tile).clamp(0.0, 1.0);
            let top = [0.56, 0.49, 1.0];
            let bot = [0.33, 0.25, 0.86];
            let col = [
                top[0] + (bot[0] - top[0]) * t,
                top[1] + (bot[1] - top[1]) * t,
                top[2] + (bot[2] - top[2]) * t,
            ];
            blend(p, col, cov(d * s / s));
            // Back card.
            let cw = tile * 0.27;
            let ch = tile * 0.19;
            let r = tile * 0.06;
            let d2 = sd_round_rect(px, py, c + tile * 0.07, c - tile * 0.13, cw, ch, r);
            blend(p, [1.0, 1.0, 1.0], cov(d2) * 0.45);
            // Front card.
            let fx = c - tile * 0.03;
            let fy = c + tile * 0.04;
            let d3 = sd_round_rect(px, py, fx, fy, cw, ch, r);
            blend(p, [1.0, 1.0, 1.0], cov(d3));
            // Lines of "text" on the front card.
            for (i, len) in [0.30f32, 0.22, 0.26].iter().enumerate() {
                let ly = fy - ch * 0.45 + i as f32 * ch * 0.45;
                let lx0 = fx - cw * 0.7;
                let hw = tile * len / 2.0;
                let dl = sd_round_rect(px, py, lx0 + hw, ly, hw, tile * 0.018, tile * 0.018);
                blend(p, [0.42, 0.36, 0.92], cov(dl));
            }
            // Green "snip" corner marks around the front card.
            let mx0 = fx - cw - tile * 0.06;
            let my1 = fy + ch + tile * 0.06;
            let arm = tile * 0.11;
            let w = tile * 0.025;
            let d4 = sd_round_rect(px, py, mx0 + arm / 2.0, my1, arm / 2.0, w, w);
            let d5 = sd_round_rect(px, py, mx0, my1 - arm / 2.0, w, arm / 2.0, w);
            blend(p, [0.24, 0.84, 0.55], cov(d4.min(d5)));
        }
    }
    to_rgba(buf)
}

/// A black-and-transparent menu-bar icon (macOS tints it for light and dark menu bars).
pub fn tray_icon(n: u32) -> Vec<u8> {
    let s = n as f32;
    let mut buf = vec![[0.0f32; 4]; (n * n) as usize];
    let stroke = s * 0.085;
    for y in 0..n {
        for x in 0..n {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let p = &mut buf[(y * n + x) as usize];
            // Back card outline (only the part not hidden by the front card).
            let back = sd_round_rect(px, py, s * 0.58, s * 0.38, s * 0.3, s * 0.21, s * 0.08);
            let front = sd_round_rect(px, py, s * 0.42, s * 0.6, s * 0.3, s * 0.21, s * 0.08);
            let back_ring = back.abs() - stroke / 2.0;
            let a_back = if front < stroke { 0.0 } else { cov(back_ring) };
            blend(p, [0.0, 0.0, 0.0], a_back);
            blend(p, [0.0, 0.0, 0.0], cov(front.abs() - stroke / 2.0));
            // Two lines on the front card.
            for (i, len) in [0.32f32, 0.22].iter().enumerate() {
                let ly = s * 0.555 + i as f32 * s * 0.12;
                let hw = s * len / 2.0;
                let dl = sd_round_rect(px, py, s * 0.25 + hw, ly, hw, s * 0.035, s * 0.035);
                blend(p, [0.0, 0.0, 0.0], cov(dl));
            }
        }
    }
    to_rgba(buf)
}
