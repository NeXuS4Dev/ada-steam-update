// The application icon, rendered procedurally with anti-aliased signed distance
// fields. Resolution-independent, so the same art is used for the window icon,
// the tray icon, toast notifications and (from build.rs) the Windows .ico.
//
// This file is dependency-free on purpose: build.rs `include!`s it.

/// Render the icon as straight (unpremultiplied) RGBA8, `size`×`size` pixels.
pub fn render_icon(size: u32) -> Vec<u8> {
    render_icon_with(size, false)
}

/// Monochrome variant for small tray sizes / template images.
pub fn render_icon_with(size: u32, badge: bool) -> Vec<u8> {
    let n = size as usize;
    let mut out = vec![0u8; n * n * 4];
    let s = size as f32;
    // 3×3 supersampling on top of the analytic AA keeps tiny sizes crisp.
    const SS: [f32; 3] = [-1.0 / 3.0, 0.0, 1.0 / 3.0];

    for y in 0..n {
        for x in 0..n {
            let mut acc = [0f32; 4];
            for oy in SS {
                for ox in SS {
                    let px = (x as f32 + 0.5 + ox) / s;
                    let py = (y as f32 + 0.5 + oy) / s;
                    let c = shade(px, py, s, badge);
                    // accumulate premultiplied
                    acc[0] += c[0] * c[3];
                    acc[1] += c[1] * c[3];
                    acc[2] += c[2] * c[3];
                    acc[3] += c[3];
                }
            }
            let a = acc[3] / 9.0;
            let i = (y * n + x) * 4;
            if a > 0.0 {
                out[i] = (acc[0] / acc[3] * 255.0).round().clamp(0.0, 255.0) as u8;
                out[i + 1] = (acc[1] / acc[3] * 255.0).round().clamp(0.0, 255.0) as u8;
                out[i + 2] = (acc[2] / acc[3] * 255.0).round().clamp(0.0, 255.0) as u8;
                out[i + 3] = (a * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

fn shade(px: f32, py: f32, size: f32, badge: bool) -> [f32; 4] {
    let aa = 0.75 / size;
    let cover = |d: f32| (0.5 - d / aa * 0.5).clamp(0.0, 1.0);

    // Background: rounded square with a diagonal blue → violet gradient.
    let bg_d = sd_round_rect(px - 0.5, py - 0.5, 0.44, 0.44, 0.2);
    let bg_a = cover(bg_d);
    if bg_a <= 0.0 {
        return [0.0; 4];
    }
    let t = ((px + py) * 0.5).clamp(0.0, 1.0);
    let top = [0.33, 0.55, 1.0];
    let bot = [0.49, 0.30, 0.98];
    let mut col = [top[0] + (bot[0] - top[0]) * t, top[1] + (bot[1] - top[1]) * t, top[2] + (bot[2] - top[2]) * t];
    // Subtle top highlight for depth.
    let hl = (1.0 - py * 2.2).clamp(0.0, 1.0) * 0.10;
    for c in &mut col {
        *c = (*c + hl).min(1.0);
    }

    // Glyph: a download arrow landing in a tray.
    let w = 0.052;
    let mut d = sd_segment(px, py, 0.5, 0.25, 0.5, 0.56) - w;
    d = d.min(sd_segment(px, py, 0.34, 0.43, 0.5, 0.59) - w);
    d = d.min(sd_segment(px, py, 0.66, 0.43, 0.5, 0.59) - w);
    d = d.min(sd_segment(px, py, 0.27, 0.6, 0.27, 0.72) - w);
    d = d.min(sd_segment(px, py, 0.73, 0.6, 0.73, 0.72) - w);
    d = d.min(sd_segment(px, py, 0.27, 0.72, 0.73, 0.72) - w);
    let g = cover(d);
    col = mix(col, [1.0, 1.0, 1.0], g);

    // Notification dot with a ring cut-out.
    let (cx, cy) = (0.765, 0.235);
    let dot = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
    let ring = cover(dot - 0.135);
    let fill = cover(dot - 0.095);
    let dot_col = if badge { [1.0, 0.36, 0.36] } else { [0.24, 0.84, 0.55] };
    col = mix(col, [1.0, 1.0, 1.0], (ring - fill).max(0.0));
    col = mix(col, dot_col, fill);
    let alpha = bg_a.max(ring);

    [col[0], col[1], col[2], alpha]
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn sd_round_rect(x: f32, y: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let qx = x.abs() - hw + r;
    let qy = y.abs() - hh + r;
    let ox = qx.max(0.0);
    let oy = qy.max(0.0);
    (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) - r
}

fn sd_segment(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (pax, pay) = (px - ax, py - ay);
    let (bax, bay) = (bx - ax, by - ay);
    let h = ((pax * bax + pay * bay) / (bax * bax + bay * bay)).clamp(0.0, 1.0);
    let (dx, dy) = (pax - bax * h, pay - bay * h);
    (dx * dx + dy * dy).sqrt()
}
