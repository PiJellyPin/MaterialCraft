// SPDX-License-Identifier: MIT OR Apache-2.0
//! OpenEXR import/export through the `exr` crate, plus Radiance HDR (RGBE) export.
//! HDR values are kept in linear space, including values above 1.0 or below 0.0.
use crate::tex::Tex;
use std::io::Cursor;

/// Read a single flat OpenEXR layer at its largest resolution, returning linear RGBA.
/// RGB takes precedence over grayscale Y; absent alpha is opaque.
pub fn decode_exr(bytes: &[u8]) -> Result<Tex, String> {
    use ::exr::prelude::*;

    // Parse once with the crate and check our allocation limits before creating pixel storage.
    let chunks = ::exr::block::read(Cursor::new(bytes), true).map_err(|e| e.to_string())?;
    if chunks.headers().len() != 1 || chunks.meta_data().requirements.has_multiple_layers {
        return Err("multi-part OpenEXR files are not supported".into());
    }
    let header = &chunks.headers()[0];
    if header.deep || chunks.meta_data().requirements.has_deep_data {
        return Err("deep OpenEXR files are not supported".into());
    }
    let (w, h) = (header.layer_size.width(), header.layer_size.height());
    if w == 0 || h == 0 || w > 32768 || h > 32768 || w.checked_mul(h).is_none_or(|n| n > 1 << 28) {
        return Err("unsupported OpenEXR size".into());
    }
    let has = |name: &str| header.channels.list.iter().any(|c| c.name == *name);
    let rgb = has("R") && has("G") && has("B");
    if !rgb && !has("Y") {
        return Err("OpenEXR file has no R, G, B or Y channel".into());
    }

    // Read only color and alpha, avoiding full-image allocations for auxiliary channels.
    let image = read()
        .no_deep_data()
        .largest_resolution_level()
        .specific_channels()
        .optional("R", 0.0f32)
        .optional("G", 0.0f32)
        .optional("B", 0.0f32)
        .optional("A", 1.0f32)
        .optional("Y", 0.0f32)
        .collect_pixels(
            |size, _| Tex { w: size.width(), h: size.height(), px: vec![[0.0; 4]; size.area()] },
            move |tex, position, (r, g, b, a, y): (f32, f32, f32, f32, f32)| {
                tex.px[position.y() * tex.w + position.x()] = if rgb { [r, g, b, a] } else { [y, y, y, a] };
            },
        )
        .first_valid_layer()
        .all_attributes()
        .pedantic()
        .non_parallel()
        .from_chunks(chunks)
        .map_err(|e| e.to_string())?;
    Ok(image.layer_data.channel_data.pixels)
}

/// OpenEXR, single part, scanline, no compression. `float` selects 32-bit float channels, otherwise half.
pub fn encode_exr(t: &Tex, float: bool) -> Vec<u8> {
    if float {
        encode_pixels(t, |v| v)
    } else {
        encode_pixels(t, ::exr::prelude::f16::from_f32)
    }
}

fn encode_pixels<S: ::exr::prelude::IntoSample>(t: &Tex, convert: impl Fn(f32) -> S + Sync) -> Vec<u8> {
    use ::exr::prelude::*;
    let channels = SpecificChannels::rgba(|position: Vec2<usize>| {
        let p = t.px[position.y() * t.w + position.x()];
        (convert(p[0]), convert(p[1]), convert(p[2]), convert(p[3]))
    });
    let image = Image::from_layer(Layer::new((t.w, t.h), LayerAttributes::default(), Encoding::UNCOMPRESSED, channels));
    let mut out = Cursor::new(Vec::new());
    image.write().non_parallel().to_buffered(&mut out).expect("valid texture should encode to an in-memory EXR");
    out.into_inner()
}

/// f32 -> IEEE half, rounded to nearest even.
pub fn f32_to_half(f: f32) -> u16 {
    ::exr::prelude::f16::from_f32(f).to_bits()
}

/// IEEE half -> f32.
pub fn half_to_f32(h: u16) -> f32 {
    ::exr::prelude::f16::from_bits(h).to_f32()
}

/// Radiance RGBE (`.hdr`), uncompressed pixels, alpha is dropped.
pub fn encode_hdr(t: &Tex) -> Vec<u8> {
    let mut out = format!("#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y {} +X {}\n", t.h, t.w).into_bytes();
    for p in &t.px {
        let m = p[0].max(p[1]).max(p[2]);
        if !(m > 1e-32) || !m.is_finite() {
            out.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        // m = frac * 2^e with frac in [0.5, 1)
        let e = m.log2().floor() as i32 + 1;
        let scale = 256.0 / 2f32.powi(e);
        out.extend_from_slice(&[(p[0].max(0.0) * scale) as u8, (p[1].max(0.0) * scale) as u8, (p[2].max(0.0) * scale) as u8, (e + 128) as u8]);
    }
    out
}
