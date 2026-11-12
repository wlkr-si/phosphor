//! Pictures: loading an image file into a canvas and scaling it to fit.

use phosphor_kernel::{Canvas, Rgb};
use png::{ColorType, Decoder, Transformations};

/// Load a PNG into a canvas. See-through parts are blended over `matte`.
///
/// Handles 8-bit grey, grey + alpha, RGB, RGBA and palette images (a palette's own
/// transparency is honoured). 16-bit images are reduced to 8.
pub fn load_png(bytes: &[u8], matte: Rgb) -> Result<Canvas, String> {
    let mut decoder = Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(Transformations::EXPAND | Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| format!("not a PNG: {e}"))?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| "picture is too large".to_string())?;
    let mut buf = vec![0u8; size];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("bad PNG: {e}"))?;
    let (w, h) = (info.width as usize, info.height as usize);
    if w == 0 || h == 0 {
        return Err("picture has no dots".to_string());
    }
    if info.bit_depth != png::BitDepth::Eight {
        return Err(format!("unexpected bit depth {:?}", info.bit_depth));
    }
    let channels = match info.color_type {
        ColorType::Grayscale => 1,
        ColorType::GrayscaleAlpha => 2,
        ColorType::Rgb => 3,
        ColorType::Rgba => 4,
        ColorType::Indexed => return Err("palette was not expanded".to_string()),
    };
    let data = buf
        .get(..w * h * channels)
        .ok_or_else(|| "PNG data is short".to_string())?;
    let m = [matte >> 16 & 255, matte >> 8 & 255, matte & 255];
    let mut dots = Vec::with_capacity(w * h);
    for px in data.chunks_exact(channels) {
        let (rgb, alpha) = match channels {
            1 => ([px[0] as u32; 3], 255),
            2 => ([px[0] as u32; 3], px[1] as u32),
            3 => ([px[0] as u32, px[1] as u32, px[2] as u32], 255),
            _ => ([px[0] as u32, px[1] as u32, px[2] as u32], px[3] as u32),
        };
        let blend = |c: usize| (rgb[c] * alpha + m[c] * (255 - alpha) + 127) / 255;
        dots.push(blend(0) << 16 | blend(1) << 8 | blend(2));
    }
    Ok(Canvas { w, h, dots })
}

/// Scale a canvas to fit inside `max_w` × `max_h` dots without changing its shape.
///
/// `dot_aspect` is a dot's height divided by its width, so the result looks the same
/// shape on screen as the source. Each output dot is the average of the source dots
/// under it. The result is at least 1 × 1.
pub fn scale_to_fit(src: &Canvas, max_w: usize, max_h: usize, dot_aspect: f64) -> Canvas {
    if src.w == 0 || src.h == 0 {
        return Canvas::new(1, 1, 0);
    }
    let aspect = if dot_aspect.is_finite() && dot_aspect > 0.0 {
        dot_aspect
    } else {
        1.0
    };
    let (w, h) = (src.w, src.h);
    let scale = (max_w as f64 / w as f64).min(max_h as f64 * aspect / h as f64);
    let ow = ((w as f64 * scale).round() as usize).clamp(1, max_w.max(1));
    let oh = ((h as f64 * scale / aspect).round() as usize).clamp(1, max_h.max(1));
    let mut out = Canvas::new(ow, oh, 0);
    for y in 0..oh {
        let y0 = y * h / oh;
        let y1 = ((y + 1) * h / oh).max(y0 + 1).min(h);
        for x in 0..ow {
            let x0 = x * w / ow;
            let x1 = ((x + 1) * w / ow).max(x0 + 1).min(w);
            let (mut acc, mut n) = ([0u64; 3], 0u64);
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let c = src.dots[sy * w + sx];
                    acc[0] += (c >> 16 & 255) as u64;
                    acc[1] += (c >> 8 & 255) as u64;
                    acc[2] += (c & 255) as u64;
                    n += 1;
                }
            }
            let avg = |a: u64| ((a + n / 2) / n) as u32;
            out.dots[y * ow + x] = avg(acc[0]) << 16 | avg(acc[1]) << 8 | avg(acc[2]);
        }
    }
    out
}

/// A canvas as a binary PPM (P6), for previews.
pub fn to_ppm(canvas: &Canvas) -> Vec<u8> {
    let mut out = format!("P6\n{} {}\n255\n", canvas.w, canvas.h).into_bytes();
    for &c in &canvas.dots {
        out.extend_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(
        w: u32,
        h: u32,
        color: ColorType,
        depth: png::BitDepth,
        data: &[u8],
        palette: Option<(&[u8], &[u8])>,
    ) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(color);
        enc.set_depth(depth);
        if let Some((p, t)) = palette {
            enc.set_palette(p.to_vec());
            enc.set_trns(t.to_vec());
        }
        let mut wr = enc.write_header().unwrap();
        wr.write_image_data(data).unwrap();
        wr.finish().unwrap();
        out
    }

    const MATTE: Rgb = 0x102030;

    #[test]
    fn rgba_loads_with_matte_under_see_through_dots() {
        let mut data = Vec::new();
        for i in 0..16u8 {
            match i % 4 {
                0 => data.extend_from_slice(&[255, 0, 0, 255]),
                1 => data.extend_from_slice(&[0, 255, 0, 255]),
                2 => data.extend_from_slice(&[9, 9, 9, 0]),
                _ => data.extend_from_slice(&[0, 0, 255, 255]),
            }
        }
        let png = encode(4, 4, ColorType::Rgba, png::BitDepth::Eight, &data, None);
        let c = load_png(&png, MATTE).unwrap();
        assert_eq!((c.w, c.h), (4, 4));
        for i in 0..16 {
            let want = [0xff0000, 0x00ff00, MATTE, 0x0000ff][i % 4];
            assert_eq!(c.dots[i], want, "dot {i}");
        }
    }

    #[test]
    fn half_see_through_blends() {
        let png = encode(
            1,
            1,
            ColorType::Rgba,
            png::BitDepth::Eight,
            &[255, 255, 255, 128],
            None,
        );
        let c = load_png(&png, 0).unwrap();
        let v = c.dots[0] & 255;
        assert!((127..=129).contains(&v));
    }

    #[test]
    fn grey_loads() {
        let png = encode(
            2,
            1,
            ColorType::Grayscale,
            png::BitDepth::Eight,
            &[0, 200],
            None,
        );
        let c = load_png(&png, MATTE).unwrap();
        assert_eq!(c.dots, vec![0x000000, 0xc8c8c8]);
    }

    #[test]
    fn grey_alpha_loads() {
        let png = encode(
            2,
            1,
            ColorType::GrayscaleAlpha,
            png::BitDepth::Eight,
            &[200, 255, 5, 0],
            None,
        );
        let c = load_png(&png, MATTE).unwrap();
        assert_eq!(c.dots, vec![0xc8c8c8, MATTE]);
    }

    #[test]
    fn rgb_loads() {
        let png = encode(1, 1, ColorType::Rgb, png::BitDepth::Eight, &[1, 2, 3], None);
        assert_eq!(load_png(&png, MATTE).unwrap().dots, vec![0x010203]);
    }

    #[test]
    fn palette_loads() {
        let pal = [255, 0, 0, 0, 0, 255, 0, 255, 0];
        let trns = [255, 255, 0];
        let png = encode(
            3,
            1,
            ColorType::Indexed,
            png::BitDepth::Eight,
            &[0, 1, 2],
            Some((&pal, &trns)),
        );
        let c = load_png(&png, MATTE).unwrap();
        assert_eq!(c.dots, vec![0xff0000, 0x0000ff, MATTE]);
    }

    #[test]
    fn sixteen_bit_is_reduced_to_eight() {
        let png = encode(
            1,
            1,
            ColorType::Rgb,
            png::BitDepth::Sixteen,
            &[0xff, 0xff, 0x80, 0x00, 0x00, 0x00],
            None,
        );
        assert_eq!(load_png(&png, MATTE).unwrap().dots, vec![0xff8000]);
    }

    #[test]
    fn bad_bytes_are_errors_not_panics() {
        assert!(load_png(&[], 0).is_err());
        assert!(load_png(b"definitely not a picture", 0).is_err());
        let png = encode(4, 4, ColorType::Rgb, png::BitDepth::Eight, &[7; 48], None);
        for cut in [8, 20, png.len() / 2, png.len() - 13] {
            assert!(load_png(&png[..cut], 0).is_err(), "cut at {cut}");
        }
    }

    #[test]
    fn scaling_keeps_shape() {
        let src = Canvas::new(100, 50, 0x123456);
        let a = scale_to_fit(&src, 40, 40, 1.0);
        assert_eq!((a.w, a.h), (40, 20));
        let b = scale_to_fit(&src, 40, 40, 2.0);
        assert_eq!((b.w, b.h), (40, 10));
        let tall = scale_to_fit(&Canvas::new(50, 100, 0), 40, 40, 1.0);
        assert_eq!((tall.w, tall.h), (20, 40));
    }

    #[test]
    fn flat_colour_stays_exact() {
        for colour in [0x000000, 0xffffff, 0x123456, 0xfedcba] {
            for (w, h) in [(100, 50), (37, 91), (3, 3)] {
                let out = scale_to_fit(&Canvas::new(w, h, colour), 40, 40, 2.0);
                assert!(out.dots.iter().all(|&d| d == colour));
            }
        }
    }

    #[test]
    fn scaling_averages() {
        let src = Canvas {
            w: 2,
            h: 1,
            dots: vec![0x000000, 0xfefefe],
        };
        let out = scale_to_fit(&src, 1, 1, 1.0);
        assert_eq!(out.dots, vec![0x7f7f7f]);
    }

    #[test]
    fn scaling_never_empty() {
        let out = scale_to_fit(&Canvas::new(1000, 1, 5), 4, 4, 1.0);
        assert!(out.w >= 1 && out.h >= 1);
        let out = scale_to_fit(&Canvas::new(3, 3, 5), 0, 0, 1.0);
        assert_eq!((out.w, out.h), (1, 1));
    }

    #[test]
    fn ppm_has_header_and_bytes() {
        let c = Canvas {
            w: 2,
            h: 1,
            dots: vec![0x010203, 0x040506],
        };
        assert_eq!(
            to_ppm(&c),
            b"P6\n2 1\n255\n\x01\x02\x03\x04\x05\x06".to_vec()
        );
    }
}
