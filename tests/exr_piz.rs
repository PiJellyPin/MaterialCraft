// SPDX-License-Identifier: MIT OR Apache-2.0
//! Synthetic fixtures encoded with OpenEXR 3.5.2 using PIZ compression.
//! Original test assets are MIT OR Apache-2.0; expected pixel values are defined below.
//! The RGB fixture uses origin (-7, -19) and decreasing scanline order.
//! The mixed fixture stores R/Z as UINT, G/A as HALF, and B as FLOAT; Z = x + y * 71.
use materialcraft::{hdri, imgio, tex::Tex};

fn check(bytes: &[u8], w: usize, h: usize, pixel: impl Fn(usize, usize) -> [f32; 4]) {
    let tex = hdri::decode_exr(bytes).expect("reference PIZ file should decode");
    assert_eq!((tex.w, tex.h), (w, h));
    for y in 0..h {
        for x in 0..w {
            assert_eq!(tex.px[y * w + x].map(f32::to_bits), pixel(x, y).map(f32::to_bits), "pixel ({x}, {y})");
        }
    }
}

#[test]
fn piz_half_rgba_odd_dimensions_and_partial_block() {
    check(include_bytes!("assets/piz_half_rgba.exr"), 65, 35, |x, y| {
        [(x as i32 % 17 - 8) as f32 * 0.5, y as f32 * 0.25, ((x * 3 + y * 5) % 31) as f32 * 2.0, ((x + y) % 5) as f32 * 0.25]
    });
}

#[test]
fn piz_float_rgb_offset_window_and_decreasing_scanlines() {
    check(include_bytes!("assets/piz_float_rgb.exr"), 67, 67, |x, y| {
        [x as f32 * 1000.25 + y as f32 / 64.0, (y as f32 - 33.0) / 32.0, x as f32 * 4.0 + y as f32 * 0.125, 1.0]
    });
}

#[test]
fn piz_mixed_half_float_uint_and_extra_channel() {
    check(include_bytes!("assets/piz_mixed.exr"), 71, 33, |x, y| {
        [(x * 31 + y) as f32, (y as f32 - 16.0) / 8.0, x as f32 * 1000.25 + y as f32 / 64.0, ((x + y) % 5) as f32 * 0.25]
    });
}

#[test]
fn piz_full_16_bit_wavelet_range() {
    check(include_bytes!("assets/piz_wide_lut.exr"), 1025, 32, |x, y| [f32::from_bits(0x3f800000 | ((y * 1025 + x) % 32768) as u32), 0.5, 2.0, 1.0]);
}

#[test]
fn piz_empty_bitmap_and_one_dimensional_images() {
    check(include_bytes!("assets/piz_zero_y.exr"), 97, 33, |_, _| [0.0, 0.0, 0.0, 1.0]);
    check(include_bytes!("assets/piz_single_row.exr"), 257, 1, |_, _| [0.5, 0.5, 0.5, 1.0]);
    check(include_bytes!("assets/piz_single_column.exr"), 1, 65, |_, _| [0.5, 0.5, 0.5, 1.0]);
}

fn attribute(bytes: &[u8], wanted: &str) -> (usize, usize) {
    let mut p = 8;
    while bytes[p] != 0 {
        let end = p + bytes[p..].iter().position(|&b| b == 0).unwrap();
        let name = std::str::from_utf8(&bytes[p..end]).unwrap();
        p = end + 1;
        p += bytes[p..].iter().position(|&b| b == 0).unwrap() + 1;
        let size = u32::from_le_bytes(bytes[p..p + 4].try_into().unwrap()) as usize;
        p += 4;
        if name == wanted {
            return (p, size);
        }
        p += size;
    }
    (p + 1, 0) // first offset-table entry
}

fn first_chunk(bytes: &[u8]) -> usize {
    let offsets = attribute(bytes, "").0;
    u64::from_le_bytes(bytes[offsets..offsets + 8].try_into().unwrap()) as usize
}

#[test]
fn compressed_formats_accept_raw_fallback_blocks() {
    let tex = Tex::from_fn(9, 1, |u, _| [u * 100.0, -u, 0.125, 1.0]);
    let original = materialcraft::exr::encode_exr(&tex, true);
    let compression = attribute(&original, "compression").0;
    for method in [1, 2, 3, 4] {
        let mut bytes = original.clone();
        bytes[compression] = method;
        let decoded = hdri::decode_exr(&bytes).unwrap();
        assert_eq!(decoded.px, tex.px, "raw fallback for compression {method}");
    }
}

#[test]
fn piz_is_available_to_bitmap_and_environment_loaders() {
    let bytes = include_bytes!("assets/piz_half_rgba.exr");
    let bitmap = imgio::decode_full("material.exr", bytes).unwrap();
    let environment = hdri::load_bytes("lighting.exr", bytes).unwrap();
    assert!(bitmap.float);
    assert_eq!(bitmap.tex.px, environment.px);
}

#[test]
fn malformed_piz_data_returns_errors() {
    let original = include_bytes!("assets/piz_half_rgba.exr");
    let chunk = first_chunk(original);
    let data = chunk + 8;
    let low = u16::from_le_bytes(original[data..data + 2].try_into().unwrap()) as usize;
    let high = u16::from_le_bytes(original[data + 2..data + 4].try_into().unwrap()) as usize;
    let bitmap_size = if low <= high { high - low + 1 } else { 0 };
    let size_field = data + 4 + bitmap_size;
    let huffman = size_field + 4;
    for length in [0, 7, attribute(original, "").0, chunk, data + 3, size_field + 2, huffman + 19, original.len() - 1] {
        assert!(hdri::decode_exr(&original[..length]).is_err(), "truncation at {length}");
    }
    // The exr crate derives the Huffman table size from its code lengths, ignoring this redundant field.
    // Accepting it must not change the decoded pixels.
    let mut redundant_length = original.to_vec();
    redundant_length[huffman + 8..huffman + 12].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(hdri::decode_exr(&redundant_length).unwrap().px, hdri::decode_exr(original).unwrap().px);
    let mutations: Vec<(usize, Vec<u8>)> = vec![
        (data + 2, 8192u16.to_le_bytes().to_vec()),
        (size_field, u32::MAX.to_le_bytes().to_vec()),
        (huffman, 65537u32.to_le_bytes().to_vec()),
        (huffman + 12, u32::MAX.to_le_bytes().to_vec()),
        (huffman + 20, vec![0xff; 16]), // impossible zero runs in the code table
        (attribute(original, "").0, u64::MAX.to_le_bytes().to_vec()),
        (chunk, i32::MIN.to_le_bytes().to_vec()),
        (chunk + 4, u32::MAX.to_le_bytes().to_vec()),
    ];
    for (at, replacement) in mutations {
        let mut bytes = original.to_vec();
        bytes[at..at + replacement.len()].copy_from_slice(&replacement);
        assert!(hdri::decode_exr(&bytes).is_err(), "corruption at {at}");
    }
}

#[test]
fn inconsistent_storage_flags_are_rejected() {
    for flag in [0x200u32, 0x800, 0x1000] {
        let mut bytes = include_bytes!("assets/piz_half_rgba.exr").to_vec();
        bytes[4..8].copy_from_slice(&(2 | flag).to_le_bytes());
        assert!(hdri::decode_exr(&bytes).is_err(), "inconsistent EXR flag {flag:x}");
    }
}

#[test]
fn crate_adapter_reads_lossless_scanlines_and_tiles() {
    use ::exr::prelude::*;
    use std::io::Cursor;
    for compression in [Compression::Uncompressed, Compression::RLE, Compression::ZIP1, Compression::ZIP16, Compression::PIZ] {
        for blocks in [Blocks::ScanLines, Blocks::Tiles(Vec2(32, 32))] {
            let encoding = Encoding { compression, blocks, line_order: LineOrder::Increasing };
            let pixels = SpecificChannels::rgba(|p: Vec2<usize>| (p.x() as f32 * 17.25, p.y() as f32 / 8.0, -0.5f32, 0.25f32));
            let image = Image::from_layer(Layer::new((65, 35), LayerAttributes::default(), encoding, pixels));
            let mut bytes = Cursor::new(Vec::new());
            image.write().non_parallel().to_buffered(&mut bytes).unwrap();
            check(bytes.get_ref(), 65, 35, |x, y| [x as f32 * 17.25, y as f32 / 8.0, -0.5, 0.25]);
        }
    }
}

#[test]
fn crate_adapter_preserves_luminance_fallback_alpha_and_rgb_precedence() {
    use ::exr::prelude::*;
    use std::io::Cursor;
    let gray =
        SpecificChannels::build().with_channel("Y").with_channel("A").with_channel("R").with_pixel_fn(|p: Vec2<usize>| (p.x() as f32 * 0.25, 0.5f32, 99.0f32));
    let mut bytes = Cursor::new(Vec::new());
    Image::from_channels((9, 3), gray).write().non_parallel().to_buffered(&mut bytes).unwrap();
    check(bytes.get_ref(), 9, 3, |x, _| [x as f32 * 0.25, x as f32 * 0.25, x as f32 * 0.25, 0.5]);

    let rgb = SpecificChannels::build()
        .with_channel("R")
        .with_channel("G")
        .with_channel("B")
        .with_channel("Y")
        .with_pixel_fn(|p: Vec2<usize>| (p.x() as f32, 2.0f32, 3.0f32, 99.0f32));
    let mut bytes = Cursor::new(Vec::new());
    Image::from_channels((9, 3), rgb).write().non_parallel().to_buffered(&mut bytes).unwrap();
    check(bytes.get_ref(), 9, 3, |x, _| [x as f32, 2.0, 3.0, 1.0]);
}

#[test]
fn crate_adapter_checks_dimensions_and_color_channels_before_allocating() {
    let original = include_bytes!("assets/piz_half_rgba.exr");
    let window = attribute(original, "dataWindow").0;
    for (max_x, max_y) in [(32768i32, 34i32), (32767, 32767)] {
        let mut bytes = original.to_vec();
        bytes[window + 8..window + 12].copy_from_slice(&max_x.to_le_bytes());
        bytes[window + 12..window + 16].copy_from_slice(&max_y.to_le_bytes());
        let error = hdri::decode_exr(&bytes).unwrap_err();
        assert!(error.contains("unsupported OpenEXR size"), "{error}");
    }
    let mut bytes = include_bytes!("assets/piz_zero_y.exr").to_vec();
    let channels = attribute(&bytes, "channels").0;
    bytes[channels] = b'Z';
    assert!(hdri::decode_exr(&bytes).unwrap_err().contains("no R, G, B or Y channel"));
}
