use ab_glyph::{FontVec, PxScale};
use anyhow::{anyhow, Result};
use image::{Rgba, RgbaImage};
use imageproc::drawing::{draw_text_mut, text_size};
use std::path::{Path, PathBuf};

/// System font candidates per platform (loaded at runtime, nothing bundled).
const FONT_CANDIDATES: &[&str] = &[
    // Windows
    "C:\\Windows\\Fonts\\segoeuib.ttf",
    "C:\\Windows\\Fonts\\arialbd.ttf",
    "C:\\Windows\\Fonts\\arial.ttf",
    // macOS
    "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/System/Library/Fonts/Supplemental/Verdana Bold.ttf",
    // Linux
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
];

fn load_font() -> Result<FontVec> {
    for path in FONT_CANDIDATES {
        if let Ok(bytes) = std::fs::read(path) {
            if let Ok(font) = FontVec::try_from_vec(bytes) {
                return Ok(font);
            }
        }
    }
    Err(anyhow!("no usable system font found (tried Segoe UI/Arial/DejaVu)"))
}

fn parse_hex(color: &str) -> Rgba<u8> {
    let c = color.trim_start_matches('#');
    let parse = |i: usize| u8::from_str_radix(c.get(i..i + 2).unwrap_or("ff"), 16).unwrap_or(255);
    if c.len() >= 6 {
        Rgba([parse(0), parse(2), parse(4), if c.len() >= 8 { parse(6) } else { 255 }])
    } else {
        Rgba([255, 255, 255, 255])
    }
}

/// Render text (possibly multi-line) to a transparent PNG in `cache_dir`.
/// Returns (path, width, height).
pub fn render_text_png(
    cache_dir: &Path,
    id: &str,
    text: &str,
    font_size: f32,
    color: &str,
) -> Result<(PathBuf, u32, u32)> {
    let font = load_font()?;
    let scale = PxScale::from(font_size.clamp(8.0, 512.0));
    let rgba = parse_hex(color);
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return Err(anyhow!("text is empty"));
    }
    let line_h = (font_size * 1.25).ceil() as u32;
    let pad = (font_size * 0.25).ceil() as u32;
    let mut max_w = 1u32;
    for l in &lines {
        let (w, _) = text_size(scale, &font, l);
        max_w = max_w.max(w);
    }
    let width = max_w + pad * 2;
    let height = line_h * lines.len() as u32 + pad * 2;
    let mut img = RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 0]));
    // Cheap outline for legibility on any background.
    let outline = Rgba([0, 0, 0, (rgba[3] as f32 * 0.85) as u8]);
    for (i, l) in lines.iter().enumerate() {
        let (w, _) = text_size(scale, &font, l);
        let x = ((width - w) / 2) as i32;
        let y = (pad + i as u32 * line_h) as i32;
        for (dx, dy) in [(-2, 0), (2, 0), (0, -2), (0, 2)] {
            draw_text_mut(&mut img, outline, x + dx, y + dy, scale, &font, l);
        }
        draw_text_mut(&mut img, rgba, x, y, scale, &font, l);
    }
    std::fs::create_dir_all(cache_dir)?;
    let out = cache_dir.join(format!("text-{id}.png"));
    img.save(&out)?;
    Ok((out, width, height))
}
