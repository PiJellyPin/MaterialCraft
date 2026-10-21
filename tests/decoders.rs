//! Decoder checks against reference pictures produced by independent encoders/decoders (tests/assets/*.ref.png).
use materialcraft::{dec_basic, dec_dds, dec_jpeg, dec_tiff, dec_webp, png};
use std::path::PathBuf;

fn data(n: &str) -> Vec<u8> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/assets").join(n);
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// returns (max abs diff, mean abs diff) in 0..255 units over RGBA
fn diff(name: &str, got: &materialcraft::tex::Tex) -> (f32, f32) {
    let r = png::decode(&data(&format!("{name}.ref.png"))).unwrap();
    assert_eq!((got.w, got.h), (r.w, r.h), "{name} size");
    let (mut mx, mut sum) = (0f32, 0f32);
    for (a, b) in got.px.iter().zip(&r.px) {
        for k in 0..4 {
            let d = ((a[k] - b[k]) * 255.0).abs();
            mx = mx.max(d);
            sum += d;
        }
    }
    (mx, sum / (got.px.len() * 4) as f32)
}

fn check(name: &str, got: Result<materialcraft::tex::Tex, String>, max: f32, mean: f32) {
    let t = got.unwrap_or_else(|e| panic!("{name}: {e}"));
    let (m, a) = diff(name, &t);
    eprintln!("{name}: max {m:.1} mean {a:.3}");
    assert!(m <= max && a <= mean, "{name}: max {m} mean {a}");
}

#[test]
fn webp() {
    for n in ["lossless.webp", "lossless_a.webp", "pal.webp"] {
        check(n, dec_webp::decode(&data(n)), 0.6, 0.01);
    }
    for n in ["lossy30.webp", "lossy90.webp", "lossy_a.webp"] {
        check(n, dec_webp::decode(&data(n)), 1.1, 0.05);
    }
}

#[test]
fn jpeg() {
    for n in ["base.jpg", "prog.jpg", "s444.jpg", "gray.jpg"] {
        check(n, dec_jpeg::decode(&data(n)), 3.0, 0.5);
    }
}

#[test]
fn tiff() {
    for n in ["tiff_lzw.tif", "packbits.tif", "tiff_adobe_deflate.tif", "raw.tif", "rgba_lzw.tif"] {
        check(n, dec_tiff::decode(&data(n)), 0.6, 0.01);
    }
}

#[test]
fn basic() {
    check("t.bmp", dec_basic::decode_bmp(&data("t.bmp")), 0.6, 0.01);
    check("t.tga", dec_basic::decode_tga(&data("t.tga")), 0.6, 0.01);
}

#[test]
fn dds() {
    for n in ["d1.dds", "d5.dds", "d0.dds"] {
        check(n, dec_dds::decode(&data(n)).map(|d| d.tex), 1.1, 0.05);
    }
}

#[test]
fn bc7_random_blocks() {
    // random block data decoded by an independent decoder (stored as BGRA bytes)
    let d = dec_dds::decode(&data("bc7.dds")).unwrap();
    let r = data("bc7.ref.bgra");
    assert_eq!(r.len(), d.tex.px.len() * 4);
    let mut bad = 0;
    for (i, p) in d.tex.px.iter().enumerate() {
        let e = [r[i * 4 + 2], r[i * 4 + 1], r[i * 4], r[i * 4 + 3]];
        for k in 0..4 {
            if ((p[k] * 255.0).round() - e[k] as f32).abs() > 1.0 {
                bad += 1;
            }
        }
    }
    assert_eq!(bad, 0, "BC7 mismatching channel values");
}

#[test]
fn bc6h_unsigned_random_blocks() {
    // independent decoder clamps/tonemaps to 8 bit, so allow a small number of extreme-value disagreements
    let d = dec_dds::decode(&data("bc6u.dds")).unwrap();
    let r = data("bc6u.ref.bgra");
    let mut big = 0;
    for (i, p) in d.tex.px.iter().enumerate() {
        let e = [r[i * 4 + 2], r[i * 4 + 1], r[i * 4]];
        for k in 0..3 {
            if (p[k].clamp(0.0, 1.0) * 255.0 - e[k] as f32).abs() > 8.0 {
                big += 1;
            }
        }
    }
    assert!(big < 12, "{big} BC6H channel values differ");
    // signed variant must at least decode without error
    dec_dds::decode(&data("bc6s.dds")).unwrap();
}

#[test]
fn corrupt_input_never_panics() {
    type F = fn(&[u8]) -> bool;
    assert!(png::decode(b"not a png").is_err());
    assert!(png::decode(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0]).is_err());
    let files: [(&str, F); 13] = [
        ("t.png", |b| png::decode(b).is_ok()),
        ("lossy30.webp", |b| dec_webp::decode(b).is_ok()),
        ("lossless_a.webp", |b| dec_webp::decode(b).is_ok()),
        ("pal.webp", |b| dec_webp::decode(b).is_ok()),
        ("lossy_a.webp", |b| dec_webp::decode(b).is_ok()),
        ("base.jpg", |b| dec_jpeg::decode(b).is_ok()),
        ("prog.jpg", |b| dec_jpeg::decode(b).is_ok()),
        ("tiff_lzw.tif", |b| dec_tiff::decode(b).is_ok()),
        ("packbits.tif", |b| dec_tiff::decode(b).is_ok()),
        ("t.bmp", |b| dec_basic::decode_bmp(b).is_ok()),
        ("t.tga", |b| dec_basic::decode_tga(b).is_ok()),
        ("d5.dds", |b| dec_dds::decode(b).is_ok()),
        ("bc7.dds", |b| dec_dds::decode(b).is_ok()),
    ];
    let mut seed = 12345u32;
    let mut rnd = || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 8) as usize
    };
    for (name, f) in files {
        let orig = data(name);
        for cut in (0..orig.len()).step_by((orig.len() / 40).max(1)) {
            let _ = f(&orig[..cut]);
        }
        for _ in 0..300 {
            let mut b = orig.clone();
            for _ in 0..1 + rnd() % 4 {
                let i = rnd() % b.len();
                b[i] = rnd() as u8;
            }
            let _ = f(&b);
        }
    }
}

#[test]
fn tiff_variants() {
    // tiled + predictor, planar, 16 bit + predictor, float, big endian (written by an independent library)
    for n in ["tiled_pred.tif", "planar.tif", "rgb16_pred.tif", "grey32f.tif", "be.tif"] {
        check(n, dec_tiff::decode(&data(n)), 0.6, 0.01);
    }
}

#[test]
fn imgio_recognises_by_content() {
    use materialcraft::imgio::{decode_full, sniff, Format};
    let cases = [
        ("t.png", Format::Png),
        ("base.jpg", Format::Jpeg),
        ("tiff_lzw.tif", Format::Tiff),
        ("lossy30.webp", Format::WebP),
        ("d5.dds", Format::Dds),
        ("t.bmp", Format::Bmp),
        ("t.tga", Format::Tga),
    ];
    for (n, f) in cases {
        let b = data(n);
        assert_eq!(sniff(n, &b), Some(f), "{n}");
        // the extension does not matter when the contents say what it is
        assert_eq!(sniff("noextension", &b).filter(|x| *x != Format::Tga), if f == Format::Tga { None } else { Some(f) }, "{n}");
        assert!(!decode_full(n, &b).unwrap().float, "{n}");
    }
    assert_eq!(sniff("x.hdr", b"#?RADIANCE\n"), Some(Format::Hdr));
    assert_eq!(sniff("x.exr", &[0x76, 0x2f, 0x31, 0x01, 0, 0]), Some(Format::Exr));
    assert!(sniff("x.txt", b"hello world").is_none());
    assert!(materialcraft::imgio::decode("x.txt", b"hello world").is_err());
}

#[test]
fn imgio_float_formats_report_float() {
    use materialcraft::imgio::decode_full;
    // an EXR and a Radiance file written by the library's own encoders
    let px: Vec<[f32; 4]> = (0..16).map(|i| [i as f32 * 0.5, 0.25, 4.0, 1.0]).collect();
    let t = materialcraft::tex::Tex { w: 4, h: 4, px };
    let exr = materialcraft::exr::encode_exr(&t, true);
    let d = decode_full("a.exr", &exr).unwrap();
    assert!(d.float && d.tex.px[15][0] > 7.0);
    let hdr = materialcraft::exr::encode_hdr(&t);
    let d = decode_full("a.hdr", &hdr).unwrap();
    assert!(d.float && d.tex.px[15][0] > 7.0);
    // floating point DDS (BC6H) reports float
    assert!(decode_full("bc6u.dds", &data("bc6u.dds")).unwrap().float);
}

#[test]
fn own_encoders_round_trip() {
    use materialcraft::export::{encode, Format};
    use materialcraft::tex::Tex;
    let t = Tex::from_fn(53, 37, |u, v| [u, v, (u * 3.0).fract(), 0.25 + 0.75 * u]);
    for (depth, tolerance) in [(8, 1.0 / 255.0), (16, 1.0 / 65535.0)] {
        let bytes = encode(Format::Png, &t, depth, 90);
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        let back = png::decode(&bytes).unwrap();
        assert_eq!((back.w, back.h), (t.w, t.h), "PNG {depth}");
        for (a, b) in t.px.iter().zip(&back.px) {
            assert!(a.iter().zip(b).all(|(a, b)| (a - b).abs() <= tolerance + 1e-6), "PNG {depth}: {a:?} versus {b:?}");
        }
    }
    // TIFF at every depth (floats are stored exactly at 32 bit)
    for (depth, tol) in [(8u8, 0.5f32 / 255.0), (16, 1.0 / 65535.0 * 2.0), (32, 1e-6)] {
        let b = encode(Format::Tiff, &t, depth, 90);
        let r = dec_tiff::decode(&b).unwrap_or_else(|e| panic!("tiff {depth}: {e}"));
        assert_eq!((r.w, r.h), (t.w, t.h));
        let worst = r.px.iter().zip(&t.px).flat_map(|(a, b)| (0..3).map(move |k| (a[k] - b[k]).abs())).fold(0f32, f32::max);
        assert!(worst <= tol + 1e-6, "tiff {depth}: worst {worst}");
    }
    // JPEG: lossy, but close at high quality
    assert!(materialcraft::jpeg::tables_complete(), "Huffman tables must cover every symbol");
    let b = encode(Format::Jpeg, &t, 8, 95);
    assert!(b.starts_with(&[0xff, 0xd8]) && b.ends_with(&[0xff, 0xd9]));
    let r = dec_jpeg::decode(&b).unwrap();
    let mean = r.px.iter().zip(&t.px).map(|(a, b)| (0..3).map(|k| (a[k] - b[k]).abs()).sum::<f32>() / 3.0).sum::<f32>() / t.px.len() as f32;
    assert!(mean < 0.02, "jpeg mean error {mean}");
}

#[test]
fn oversized_headers_are_rejected_not_allocated() {
    // TGA header claiming 65535 x 65535 pixels with no data behind it.
    let mut tga = vec![0u8; 18];
    tga[2] = 2;
    tga[12] = 0xff;
    tga[13] = 0xff;
    tga[14] = 0xff;
    tga[15] = 0xff;
    tga[16] = 24;
    let e = materialcraft::imgio::decode("x.tga", &tga).err().expect("must fail");
    assert!(!e.is_empty());
    assert!(materialcraft::imgio::check_size(20_000, 20_000).is_err());
    assert!(materialcraft::imgio::check_size(8192, 8192).is_ok());
    assert!(materialcraft::imgio::check_size(0, 5).is_err());
}

#[test]
fn png_decoder_handles_palette_alpha_and_low_bit_depth() {
    // hand-built 2x2 4-bit palette PNG with tRNS, stored (uncompressed) deflate
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        out.extend_from_slice(&[0, 0, 0, 0]); // CRC is not checked by the decoder
    }
    let mut p = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&2u32.to_be_bytes());
    ihdr.extend_from_slice(&2u32.to_be_bytes());
    ihdr.extend_from_slice(&[4, 3, 0, 0, 0]);
    chunk(&mut p, b"IHDR", &ihdr);
    chunk(&mut p, b"PLTE", &[255, 0, 0, 0, 255, 0]);
    chunk(&mut p, b"tRNS", &[255, 128]);
    let rows = [0u8, 0x01, 0u8, 0x10]; // filter 0 + two pixels per row (indices 0,1 / 1,0)
    let mut z = vec![0x78, 0x01, 1, rows.len() as u8, 0, !(rows.len() as u8), 0xff];
    z.extend_from_slice(&rows);
    z.extend_from_slice(&[0, 0, 0, 0]);
    chunk(&mut p, b"IDAT", &z);
    chunk(&mut p, b"IEND", &[]);
    let t = png::decode(&p).unwrap();
    assert_eq!((t.w, t.h), (2, 2));
    assert!(t.px[0][0] > 0.99 && t.px[0][3] > 0.99); // index 0: opaque red
    assert!(t.px[1][1] > 0.99 && (t.px[1][3] - 128.0 / 255.0).abs() < 0.01); // index 1: half-transparent green
}
