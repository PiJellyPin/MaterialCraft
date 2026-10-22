fn smin_(a: f32, b: f32, k: f32) -> f32 {
    if (k < 1e-5) { return min(a, b); }
    let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return b * (1.0 - h) + a * h - k * h * (1.0 - h);
}
// season anchors: (hue or -1, saturation x, value x, white mix, cold tint)
fn san(i: i32) -> array<f32, 5> {
    let k = remi(i, 4);
    if (k == 0) { return array<f32, 5>(0.22, 1.0, 1.12, 0.0, 0.0); }
    if (k == 1) { return array<f32, 5>(0.30, 1.18, 0.92, 0.0, 0.0); }
    if (k == 2) { return array<f32, 5>(0.07, 1.05, 1.0, 0.0, 0.0); }
    return array<f32, 5>(-1.0, 0.2, 1.1, 0.35, 0.25);
}
fn hgrad(x: i32, y: i32, rsh: f32) -> vec2<f32> {
    let gx = lum(txA(x + 1, y)) - lum(txA(x - 1, y));
    let gy = lum(txA(x, y + 1)) - lum(txA(x, y - 1));
    return vec2<f32>(gx * 0.5 * rsh, gy * 0.5 * rsh);
}
fn xsample(filt: i32, uu: f32, vv: f32) -> vec4<f32> {
    if (filt == 0) { return txA(i32(floor(uu * f32(IW()))), i32(floor(vv * f32(IH())))); }
    if (filt == 2) { return cuA(uu, vv); }
    return spA(uu, vv);
}
fn cs_y(x: i32) -> f32 {
    let w = i32(u.w);
    let v = lum(ia[u32(i32(P(0u))) * u.w + u32(remi(x, w))]);
    return f32(u.h) - P(4u) - clamp(v * P(1u), 0.0, 1.0) * (f32(u.h) - 2.0 * P(4u));
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= u.w || gid.y >= u.h) { return; }
    let i = gid.y * u.w + gid.x;
    let xi = i32(gid.x);
    let yi = i32(gid.y);
    let uu = (f32(gid.x) + 0.5) / f32(u.w);
    let vv = (f32(gid.y) + 0.5) / f32(u.h);
    var o = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    switch (u.op) {
        case 0u: {
            // cross_section: P0 row, P1 height gain, P2 thickness, P3 fill, P4 margin
            let hs = f32(u.h);
            let y = f32(gid.y) + 0.5;
            let a = cs_y(xi - 1);
            let b = cs_y(xi);
            let c = cs_y(xi + 1);
            let lo = min(min(a, b), c);
            let hi = max(max(a, b), c);
            var dy = 0.0;
            if (y < lo) { dy = lo - y; } else if (y > hi) { dy = y - hi; }
            let line = 1.0 - smoothc(P(2u) * 0.5, P(2u) * 0.5 + 1.0, dy);
            var area = 0.0;
            if (P(3u) > 0.5 && y > b) { area = 0.28; }
            let q = (y - P(4u)) / (hs - 2.0 * P(4u)) * 4.0;
            var grid = 0.0;
            if (abs(q - rnd_away(q)) * (hs - 2.0 * P(4u)) / 4.0 < 0.6) { grid = 0.09; }
            o = grey(max(max(line, area), grid));
        }
        case 1u: {
            // uber_emboss: ia height, ib mask, ic gloss; P0 intensity, P1..3 light, P4..6 half vector, P7 highlight, P8 gain,
            // P9 sin(elevation), P10 gloss, P11 output, P12 has mask, P13 has gloss, P14 height scale
            let g2 = hgrad(xi, yi, P(14u));
            let nn = vec3<f32>(-g2.x * P(0u) * 8.0, -g2.y * P(0u) * 8.0, 1.0);
            let m = sqrt(nn.x * nn.x + nn.y * nn.y + 1.0);
            let nv = nn / m;
            let l = vec3<f32>(P(1u), P(2u), P(3u));
            let hv = vec3<f32>(P(4u), P(5u), P(6u));
            let d = dot(nv, l);
            var g = P(10u);
            if (P(13u) > 0.5) { g = lum(ic[i]); }
            let expo = lerpf(4.0, 256.0, g * g);
            let sp = pow(max(dot(nv, hv), 0.0), expo) * P(7u) * lerpf(0.4, 1.0, g);
            var mk = 1.0;
            if (P(12u) > 0.5) { mk = lum(ib[i]); }
            let oi = i32(P(11u));
            var val = 0.5 + (d - P(9u)) * P(8u) * 0.5 / (1.0 - P(9u) * 0.6);
            if (oi == 1) { val = sp; } else if (oi == 2) { val = max(P(9u) - d, 0.0) * P(8u) * 2.0; }
            var base = 0.0;
            if (oi == 0) { base = 0.5; }
            o = grey(clamp(lerpf(base, val, mk), 0.0, 1.0));
        }
        case 2u: {
            // safe_transform: P0 mode, P1 filter, P2/3 offset, P4/5 scale, P6/7 integer matrix, P8 edge, P9 rotate after, P10 sin, P11 cos
            let filt = i32(P(1u));
            if (P(0u) < 0.5) {
                let x = uu * P(4u);
                let y = vv * P(5u);
                o = xsample(filt, P(6u) * x - P(7u) * y + P(2u), P(7u) * x + P(6u) * y + P(3u));
            } else {
                let aw = f32(u.w);
                let ah = f32(u.h);
                var x = (uu - 0.5) * aw;
                var y = (vv - 0.5) * ah;
                if (P(9u) > 0.5) {
                    x = x / P(4u);
                    y = y / P(5u);
                    let rx = x * P(11u) + y * P(10u);
                    let ry = -x * P(10u) + y * P(11u);
                    x = rx;
                    y = ry;
                } else {
                    let rx = x * P(11u) + y * P(10u);
                    let ry = -x * P(10u) + y * P(11u);
                    x = rx / P(4u);
                    y = ry / P(5u);
                }
                var su = x / aw + 0.5 - P(2u);
                var sv = y / ah + 0.5 - P(3u);
                var alpha = 1.0;
                let edge = i32(P(8u));
                if (edge == 1) {
                    var r = su - 2.0 * floor(su / 2.0);
                    if (r > 1.0) { r = 2.0 - r; }
                    su = r;
                    r = sv - 2.0 * floor(sv / 2.0);
                    if (r > 1.0) { r = 2.0 - r; }
                    sv = r;
                } else if (edge == 2) {
                    su = clamp(su, 0.0, 1.0);
                    sv = clamp(sv, 0.0, 1.0);
                } else if (edge == 3) {
                    if (su < 0.0 || su >= 1.0 || sv < 0.0 || sv >= 1.0) { alpha = 0.0; }
                }
                let c = xsample(filt, su, sv);
                o = c * alpha;
            }
        }
        case 3u: {
            // mesh_weathering: ia curvature, ib ao, ic normal, idd noise; P0 wear, P1 dirt, P2 dust, P3 cos limit, P4/5 up,
            // P6 breakup, P7 has ao, P8 has normal, P9 has noise, P10/11 noise periods
            let c = lum(ia[i]) - 0.5;
            var nz: f32;
            if (P(9u) > 0.5) { nz = lum(idd[i]); } else { nz = fbm_n(uu, vv, i32(P(10u)), i32(P(11u)), 4u, 0.5, u.n, 0u); }
            let br = (nz - 0.5) * P(6u);
            let wear = P(0u);
            let dirt = P(1u);
            let dust = P(2u);
            let w = smoothc(0.0, 0.25, c * 2.0 * (0.4 + wear * 1.6) + br * 0.6) * sqrt(min(wear, 1.0));
            let cav = max(-c * 2.0, 0.0);
            var occl = 0.0;
            if (P(7u) > 0.5) { occl = 1.0 - lum(ib[i]); }
            let d = smoothc(0.05, 0.6, (cav + occl) * (0.5 + dirt) + br) * sqrt(dirt);
            var du = 0.0;
            if (P(8u) > 0.5) {
                let p = ic[i];
                let nx = p.x * 2.0 - 1.0;
                let ny = p.y * 2.0 - 1.0;
                let nzz = p.z * 2.0 - 1.0;
                let l = max(sqrt(nx * nx + ny * ny + nzz * nzz), 1e-6);
                let facing = (nx * P(4u) + ny * P(5u)) / l;
                du = smoothc(P(3u), 1.0, facing + br * 0.5) * sqrt(dust);
            }
            o = grey(1.0 - (1.0 - w) * (1.0 - d) * (1.0 - du));
        }
        case 4u: {
            // snow mask: ia height, ib mask; P0 coverage, P1 slope hold, P2 softness, P3 breakup, P4 height bias, P5 steepness,
            // P6 has mask, P7/8 noise periods, P9 height scale
            let g2 = hgrad(xi, yi, P(9u));
            let slope = sqrt(g2.x * g2.x + g2.y * g2.y) * P(5u);
            let up = 1.0 / sqrt(1.0 + slope * slope);
            let hv = lum(ia[i]);
            let up_min = 1.0 - P(1u) * 0.8;
            let fb = fbm_n(uu, vv, i32(P(7u)), i32(P(8u)), 4u, 0.5, u.n, 0u);
            let k = up - up_min + (P(0u) - 0.5) * 0.6 + (hv - 0.5) * P(4u) * 0.6 + (fb - 0.5) * P(3u) * 0.8;
            var mk = 1.0;
            if (P(6u) > 0.5) { mk = lum(ib[i]); }
            o = grey(smoothc(0.0, P(2u), k) * mk);
        }
        case 5u: {
            // snow height: ia height, ib blurred mask, ic wider blur; P0 depth
            let c = lum(ib[i]);
            o = grey(clamp(lum(ia[i]) + P(0u) * (c * 0.7 + lum(ic[i]) * 0.3), 0.0, 1.0));
        }
        case 6u: {
            // season_filter: ia picture, ib mask; P0 has mask, P1 strength, P2 variation, P3 foliage hue, P4 range, P5 season index,
            // P6 blend, P7/8 noise periods
            let p = ia[i];
            let hsv = rgb_to_hsv(p.xyz);
            let h0 = hsv.x;
            let sat = hsv.y;
            let val = hsv.z;
            let fr = P(4u);
            var dh = abs(h0 - P(3u));
            dh = min(dh, 1.0 - dh);
            let foliage = smoothc(fr, fr * 0.4, dh) * smoothc(0.12, 0.3, sat);
            var mk = 1.0;
            if (P(0u) > 0.5) { mk = lum(ib[i]); }
            let wgt = P(1u) * mk;
            let nz = fbm_n(uu, vv, i32(P(7u)), i32(P(8u)), 3u, 0.5, u.n, 0u) - 0.5;
            let i0 = i32(P(5u));
            let t = P(6u);
            let a = san(i0);
            let b = san(i0 + 1);
            var tha = h0;
            if (a[0] >= 0.0) {
                tha = a[0];
                if (remi(i0, 4) == 2) { tha = rem1(a[0] + nz * P(2u) * 0.2); }
            }
            var thb = h0;
            if (b[0] >= 0.0) {
                thb = b[0];
                if (remi(i0 + 1, 4) == 2) { thb = rem1(b[0] + nz * P(2u) * 0.2); }
            }
            var da = tha - h0;
            da = da - rnd_away(da);
            var db = thb - h0;
            db = db - rnd_away(db);
            let hue = lerpf(h0 + da * foliage, h0 + db * foliage, t);
            var wa = 0.0;
            if (a[3] > 0.0) { wa = 1.0; }
            var wb = 0.0;
            if (b[3] > 0.0) { wb = 1.0; }
            let sat_m = lerpf(lerpf(1.0, a[1], max(foliage, wa)), lerpf(1.0, b[1], max(foliage, wb)), t);
            let val_m = lerpf(a[2], b[2], t);
            var c = hsv_to_rgb(rem1(hue), clamp(sat * sat_m, 0.0, 1.0), clamp(val * val_m, 0.0, 1.0));
            let white = lerpf(a[3], b[3], t);
            let cold = lerpf(a[4], b[4], t);
            c = vec3<f32>(lerpf(c.x, 0.95, white), lerpf(c.y, 0.97, white), lerpf(c.z, 1.0, white));
            c = vec3<f32>(lerpf(c.x, c.x * 0.85, cold), lerpf(c.y, c.y * 0.95, cold), lerpf(c.z, min(c.z * 1.1, 1.0), cold));
            o = vec4<f32>(lerpf(p.x, c.x, wgt), lerpf(p.y, c.y, wgt), lerpf(p.z, c.z, wgt), p.w);
        }
        case 7u: {
            // water_filter: ia height, ib base colour; P0 level, P1 softness, P2..4 tint, P5 murkiness, P6 wet edge, P7 ripples,
            // P8 dry roughness, P9 wet roughness, P10 output, P11 has base, P12/13 ripple periods
            let hv = lum(ia[i]);
            let level = P(0u);
            let soft = P(1u);
            let water = smoothc(level + soft, level - soft, hv);
            let wet = smoothc(level + soft + P(6u), level - soft, hv);
            let deep = clamp((level - hv) / max(level, 0.05), 0.0, 1.0);
            let oi = i32(P(10u));
            if (oi == 1) { o = grey(water); }
            else if (oi == 2) {
                let rip = (fbm_n(uu, vv, i32(P(12u)), i32(P(13u)), 3u, 0.5, u.n, 0u) - 0.5) * P(7u) * 0.04;
                o = grey(lerpf(hv, level + rip, water));
            } else if (oi == 3) { o = grey(lerpf(P(8u), P(9u), max(water, wet * 0.5))); }
            else {
                var bc = vec4<f32>(0.5, 0.5, 0.5, 1.0);
                if (P(11u) > 0.5) { bc = ib[i]; }
                let dk = bc.xyz * (1.0 - 0.45 * wet);
                let k = water * (0.4 + 0.6 * P(5u) * (0.4 + 0.6 * deep));
                o = vec4<f32>(lerpf(dk.x, P(2u), k), lerpf(dk.y, P(3u), k), lerpf(dk.z, P(4u), k), bc.w);
            }
        }
        case 8u: {
            // summed_area_table, step one: running sum along every row (one thread per row); P0 per channel
            let t = gid.y * u.w + gid.x;
            if (t < u.h) {
                var acc = vec3<f32>(0.0);
                for (var x = 0u; x < u.w; x = x + 1u) {
                    let p = ia[t * u.w + x];
                    var v = vec3<f32>(lum(p));
                    if (P(0u) > 0.5) { v = p.xyz; }
                    acc = acc + v;
                    dst[t * u.w + x] = vec4<f32>(acc, 1.0);
                }
            }
            return;
        }
        case 9u: {
            // summed_area_table, step two: running sum down every column, then the normalisation; P0 normalise
            let t = gid.y * u.w + gid.x;
            if (t < u.w) {
                var tot = vec3<f32>(0.0);
                for (var y = 0u; y < u.h; y = y + 1u) { tot = tot + ia[y * u.w + (u.w - 1u)].xyz; }
                var acc = vec3<f32>(0.0);
                for (var y = 0u; y < u.h; y = y + 1u) {
                    acc = acc + ia[y * u.w + t].xyz;
                    var r = acc / f32(u.w * u.h);
                    if (P(0u) > 0.5) { r = acc / max(tot, vec3<f32>(1e-9)); }
                    dst[y * u.w + t] = vec4<f32>(r, 1.0);
                }
            }
            return;
        }
        case 10u: {
            // sdf_op: ia a, ib b; P0 op, P1 smoothing, P2 range
            let rng = max(P(2u), 1e-4);
            let da = (0.5 - lum(ia[i])) * 2.0 * rng;
            let db = (0.5 - lum(ib[i])) * 2.0 * rng;
            let k = P(1u);
            let op = u32(P(0u));
            var r: f32;
            if (op == 0u) { r = smin_(da, db, k); }
            else if (op == 1u) { r = -smin_(-da, db, k); }
            else { r = -smin_(-da, -db, k); }
            o = grey(clamp(0.5 - r / (2.0 * rng), 0.0, 1.0));
        }
        case 11u: {
            // sdf_mask: P0 offset, P1 softness
            o = grey(smoothc(0.5 + P(0u) - P(1u) * 0.5, 0.5 + P(0u) + P(1u) * 0.5, lum(ia[i])));
        }
        default: {}
    }
    if (u.n == 0xFFFFFFFFu) { o = o + ia[0] * 0.0 + ib[0] * 0.0 + ic[0] * 0.0 + idd[0] * 0.0; }
    dst[i] = o;
}
