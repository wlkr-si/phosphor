//! Bake a font into a glyph sheet, holding every printable ASCII character.
//!
//! cargo run -p phosphor-foundry --bin bake -- <font.ttf> <px> <out.sheet> [--preview out.ppm "TEXT"]
//!
//! The preview draws TEXT from the baked sheet, soft-edged, light on dark, as a PPM picture
//! with one pixel per dot.

use std::process::ExitCode;

use phosphor_foundry::sheet::{bake, Edge, GlyphSheet};
use phosphor_kernel::Canvas;

const USAGE: &str = "usage: bake <font.ttf> <px> <out.sheet> [--preview out.ppm \"TEXT\"]";
const PAPER: u32 = 0x101010;
const INK: u32 = 0xe8e8e8;
const MARGIN: usize = 4;

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("bake: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let (font_path, px, out, preview) = match args.as_slice() {
        [f, p, o] => (f, p, o, None),
        [f, p, o, flag, ppm, text] if flag == "--preview" => (f, p, o, Some((ppm, text))),
        _ => return Err(USAGE.to_string()),
    };
    let px: f32 = px.parse().map_err(|_| format!("{px:?} is not a size"))?;
    let font = std::fs::read(font_path).map_err(|e| format!("{font_path}: {e}"))?;
    let chars: String = (' '..='~').collect();
    let sheet = bake(&font, px, &chars)?;
    let bytes = sheet.to_bytes();
    std::fs::write(out, &bytes).map_err(|e| format!("{out}: {e}"))?;
    println!(
        "{}: {} glyphs at {} px, line height {}, {} bytes",
        sheet.name,
        sheet.glyphs.len(),
        sheet.px,
        sheet.line_height,
        bytes.len()
    );
    if let Some((ppm, text)) = preview {
        let canvas = preview_canvas(&sheet, text);
        std::fs::write(ppm, to_ppm(&canvas)).map_err(|e| format!("{ppm}: {e}"))?;
        println!("preview: {} x {} dots", canvas.w, canvas.h);
    }
    Ok(())
}

fn preview_canvas(sheet: &GlyphSheet, text: &str) -> Canvas {
    let (w, h) = sheet.measure(text);
    let mut canvas = Canvas::new(w as usize + 2 * MARGIN, h as usize + 2 * MARGIN, PAPER);
    sheet.draw(
        &mut canvas,
        MARGIN as i64,
        MARGIN as i64,
        text,
        INK,
        Edge::Soft,
    );
    canvas
}

fn to_ppm(canvas: &Canvas) -> Vec<u8> {
    let mut out = format!("P6\n{} {}\n255\n", canvas.w, canvas.h).into_bytes();
    for &d in &canvas.dots {
        out.extend_from_slice(&[(d >> 16) as u8, (d >> 8) as u8, d as u8]);
    }
    out
}
