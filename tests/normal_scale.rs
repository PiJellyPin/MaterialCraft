// SPDX-License-Identifier: MIT OR Apache-2.0
//! The imported Normal node tilts surfaces like the original: a ramp falling by 1 over the picture height gives
//! G = 0.5 + intensity / 512 (DirectX), independent of the resolution (values read from the original program).
use materialcraft::ops;
use materialcraft::tex::Tex;

#[test]
fn normal_intensity_matches_the_original() {
    for size in [64usize, 256, 1024] {
        let ramp = Tex::from_fn(size, size, |_, v| [1.0 - v, 1.0 - v, 1.0 - v, 1.0]);
        for intensity in [1.0f32, 10.0] {
            let n = ops::normal_from_height(&ramp, intensity / 12.8, true);
            let p = n.px[(size / 2) * size + size / 2];
            let ny = (p[1] - 0.5) * 2.0;
            let want = intensity / 256.0;
            let want = want / (1.0 + want * want).sqrt();
            assert!((ny - want).abs() < want * 0.02, "size {size} intensity {intensity}: ny {ny} want {want}");
        }
    }
}
