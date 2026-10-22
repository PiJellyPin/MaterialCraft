// neighbourhood, transform and warp operations (ia = main picture, ib / ic = secondary pictures)
fn gA(x: i32, y: i32) -> f32 { return lum(txA(x, y)); }
fn gB(x: i32, y: i32) -> f32 { return lum(txB(x, y)); }
fn dec3(p: vec4<f32>) -> vec3<f32> { return (p.xyz - vec3<f32>(0.5)) * 2.0; }
fn enc3(v: vec3<f32>, a: f32) -> vec4<f32> {
    let l = length(v);
    if (l < 1e-8) { return vec4<f32>(0.5, 0.5, 1.0, a); }
    let n = v / l;
    return vec4<f32>(n * 0.5 + vec3<f32>(0.5), a);
}
fn seam_w(i: u32, n: u32) -> f32 {
    let x = (f32(i) + 0.5) / f32(n);
    let d = abs(2.0 * x - 1.0);
    let k = 1.0 - d;
    return k * k * (3.0 - 2.0 * k);
}
fn fold_i(x: i32, n: i32, o: f32, inv: bool) -> i32 {
    let ax = clamp(o, 0.0, 1.0) * f32(n);
    let xc = f32(x) + 0.5;
    var source = xc < ax;
    if (inv) { source = xc >= ax; }
    if (source) { return x; }
    return wrapi(i32(rnd_away(2.0 * ax - xc - 0.5)), n);
}
fn wrapi(a: i32, m: i32) -> i32 {
    let q = i32(floor(f32(a) / f32(m)));
    return a - q * m;
}
fn diag_f(uu: f32, vv: f32, sg: f32) -> vec2<f32> {
    let x = abs(wrapd(uu - vv)) * sg;
    let y = uu + vv;
    return vec2<f32>((y + x) * 0.5, (y - x) * 0.5);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= u.w || gid.y >= u.h) { return; }
    let i = gid.y * u.w + gid.x;
    let x = i32(gid.x);
    let y = i32(gid.y);
    let uu = (f32(gid.x) + 0.5) / f32(u.w);
    let vv = (f32(gid.y) + 0.5) / f32(u.h);
    var o = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    switch (u.op) {
        case 0u: {
            // emboss: angle, intensity
            let sc = vec2<f32>(sin(P(0u) * TAU), cos(P(0u) * TAU));
            let dx = gA(x + 1, y) - gA(x - 1, y);
            let dy = gA(x, y + 1) - gA(x, y - 1);
            o = grey(clamp(0.5 + (dx * sc.y + dy * sc.x) * P(1u), 0.0, 1.0));
        }
        case 1u: {
            // emboss_lit: ia = source, ib = height. angle, intensity, gloss, hl(4..6), sh(8..10)
            let s = sin(P(0u) * TAU);
            let c = cos(P(0u) * TAU);
            let gloss = P(2u);
            let l = normalize(vec3<f32>(c, s, 1.0));
            let hv = normalize(vec3<f32>(l.x, l.y, l.z + 1.0));
            let flat_spec = hv.z;
            var expo = 0.0;
            if (gloss > 1e-4) { expo = clamp(2.0 / (gloss * gloss), 2.0, 4096.0); }
            let p = spA(uu, vv);
            let dx = gB(x + 1, y) - gB(x - 1, y);
            let dy = gB(x, y + 1) - gB(x, y - 1);
            let d = (dx * c + dy * s) * P(1u);
            let lit = max(d, 0.0);
            let shade = max(-d, 0.0);
            var spec = 0.0;
            if (expo > 0.0) {
                let n = vec3<f32>(-dx * P(1u), -dy * P(1u), 1.0);
                let nl = sqrt(n.x * n.x + n.y * n.y + 1.0);
                let ndh = dot(n, hv) / nl;
                spec = min(max(pow(max(ndh, 0.0), expo) - pow(flat_spec, expo), 0.0) * gloss * 2.0, 1.0);
            }
            var r = vec4<f32>(0.0, 0.0, 0.0, p.w);
            for (var k = 0u; k < 3u; k = k + 1u) {
                r[k] = clamp(p[k] + P(4u + k) * (lit + spec) - (1.0 - P(8u + k)) * shade, 0.0, 1.0);
            }
            o = r;
        }
        case 2u: {
            // edge_detect (sobel magnitude): intensity
            let gx = -gA(x - 1, y - 1) - 2.0 * gA(x - 1, y) - gA(x - 1, y + 1) + gA(x + 1, y - 1) + 2.0 * gA(x + 1, y) + gA(x + 1, y + 1);
            let gy = -gA(x - 1, y - 1) - 2.0 * gA(x, y - 1) - gA(x + 1, y - 1) + gA(x - 1, y + 1) + 2.0 * gA(x, y + 1) + gA(x + 1, y + 1);
            o = grey(clamp(sqrt(gx * gx + gy * gy) * P(0u), 0.0, 1.0));
        }
        case 3u: {
            // swirl: amount, radius
            let dx = wrapd(uu - 0.5);
            let dy = wrapd(vv - 0.5);
            let r = sqrt(dx * dx + dy * dy);
            let k = clamp(1.0 - r / max(P(1u), 1e-3), 0.0, 1.0);
            let a = P(0u) * k * k * TAU;
            let s = sin(a);
            let c = cos(a);
            o = spA(0.5 + c * dx - s * dy, 0.5 + s * dx + c * dy);
        }
        case 4u: {
            // transform: ox, oy, sx, sy, rotation(turns), tile
            let s = sin(P(4u) * TAU);
            let c = cos(P(4u) * TAU);
            let sx = max(abs(P(2u)), 1e-4);
            let sy = max(abs(P(3u)), 1e-4);
            let px = uu - 0.5;
            let py = vv - 0.5;
            let rx = c * px + s * py;
            let ry = -s * px + c * py;
            let su = rx / sx + 0.5 - P(0u);
            let sv = ry / sy + 0.5 - P(1u);
            if (P(5u) < 0.5 && (su < 0.0 || su >= 1.0 || sv < 0.0 || sv >= 1.0)) {
                o = vec4<f32>(0.0, 0.0, 0.0, 1.0);
            } else {
                o = cuA(su, sv);
            }
        }
        case 5u: {
            // mirror: do_x, do_y, inv_x, inv_y, ox, oy
            var sx = x;
            var sy = y;
            if (P(1u) > 0.5) { sy = fold_i(y, i32(u.h), P(5u), P(3u) > 0.5); }
            if (P(0u) > 0.5) { sx = fold_i(x, i32(u.w), P(4u), P(2u) > 0.5); }
            o = txA(sx, sy);
        }
        case 6u: {
            // seamless blend of the picture with itself shifted by half
            let wx = seam_w(gid.x, u.w);
            let wy = seam_w(gid.y, u.h);
            let x2 = (gid.x + u.w / 2u) % u.w;
            let y2 = (gid.y + u.h / 2u) % u.h;
            let a = ia[gid.y * u.w + gid.x];
            let b = ia[gid.y * u.w + x2];
            let c = ia[y2 * u.w + gid.x];
            let d = ia[y2 * u.w + x2];
            o = a * wx * wy + b * (1.0 - wx) * wy + c * wx * (1.0 - wy) + d * (1.0 - wx) * (1.0 - wy);
        }
        case 7u: {
            // sobel node: k
            let gx = gA(x + 1, y - 1) + 2.0 * gA(x + 1, y) + gA(x + 1, y + 1) - gA(x - 1, y - 1) - 2.0 * gA(x - 1, y) - gA(x - 1, y + 1);
            let gy = gA(x - 1, y + 1) + 2.0 * gA(x, y + 1) + gA(x + 1, y + 1) - gA(x - 1, y - 1) - 2.0 * gA(x, y - 1) - gA(x + 1, y - 1);
            o = grey(clamp(sqrt(gx * gx + gy * gy) * P(0u) * 0.25, 0.0, 1.0));
        }
        case 8u: {
            // polar_convert: mode, radius, rotation
            let rad = P(1u);
            if (P(0u) < 0.5) {
                let a = (uu + P(2u)) * TAU;
                let r = 0.5 * rad * abs(1.0 - 2.0 * vv);
                o = cuA(0.5 + r * cos(a), 0.5 + r * sin(a));
            } else {
                let dx = uu - 0.5;
                let dy = vv - 0.5;
                let rn = sqrt(dx * dx + dy * dy) * 2.0 / rad;
                let a = atan2(dy, dx) / TAU - P(2u);
                o = cuA(a, (1.0 - min(rn, 1.0)) * 0.5);
            }
        }
        case 9u: {
            // symmetry_slice: slices, mirror, rotation
            let dx = uu - 0.5;
            let dy = vv - 0.5;
            let r = sqrt(dx * dx + dy * dy);
            let sl = P(0u);
            let a = rem1(atan2(dy, dx) / TAU - P(2u)) * sl;
            let k = floor(a);
            var local = a - k;
            if (P(1u) > 0.5 && (i32(k) % 2) != 0) { local = 1.0 - local; }
            let ang = (local / sl + P(2u)) * TAU;
            o = cuA(0.5 + r * cos(ang), 0.5 + r * sin(ang));
        }
        case 10u: {
            // homography (quad / trapezoid): h0..h7
            let den = P(6u) * uu + P(7u) * vv + 1.0;
            if (abs(den) < 1e-5) {
                o = spA(uu, vv);
            } else {
                o = cuA((P(0u) * uu + P(1u) * vv + P(2u)) / den, (P(3u) * uu + P(4u) * vv + P(5u)) / den);
            }
        }
        case 11u: {
            // min_max: mode (1 max)
            let a = ia[i];
            let b = ib[i];
            if (P(0u) > 0.5) { o = max(a, b); } else { o = min(a, b); }
        }
        case 12u: {
            // frequency_merge: ia low, ib high, gain
            let l = ia[i];
            let h = ib[i];
            o = vec4<f32>(l.xyz + (h.xyz - vec3<f32>(0.5)) * P(0u), l.w);
        }
        case 13u: {
            // frequency_split (high band): ia source, ib low band
            let p = ia[i];
            let l = ib[i];
            o = vec4<f32>(p.xyz - l.xyz + vec3<f32>(0.5), p.w);
        }
        case 14u: {
            // clone_patch: src_x, src_y, dst_x, dst_y, w, h, feather, opacity, sin, cos
            let lx = wrapd(uu - P(2u));
            let ly = wrapd(vv - P(3u));
            let p = spA(uu, vv);
            o = p;
            let pw = P(4u);
            let ph = P(5u);
            if (abs(lx) < pw * 0.5 && abs(ly) < ph * 0.5) {
                let e = min((pw * 0.5 - abs(lx)) / (pw * 0.5), (ph * 0.5 - abs(ly)) / (ph * 0.5));
                let wgt = smoothc(0.0, max(P(6u), 1e-4), e) * P(7u);
                let q = spA(P(0u) + P(9u) * lx - P(8u) * ly, P(1u) + P(8u) * lx + P(9u) * ly);
                o = lerp4(p, q, wgt);
            }
        }
        case 16u: {
            // symmetry: axis, offset, sign, transfer
            let axis = u32(P(0u));
            let c = P(1u);
            let sg = P(2u);
            let tm = u32(P(3u));
            var ab = vec2<f32>(uu, vv);
            let fu = 0.5 + c + sg * abs(wrapd(uu - 0.5 - c));
            let fv = 0.5 + c + sg * abs(wrapd(vv - 0.5 - c));
            if (axis == 0u) { ab = vec2<f32>(fu, vv); }
            else if (axis == 1u) { ab = vec2<f32>(uu, fv); }
            else if (axis == 2u) { ab = vec2<f32>(fu, fv); }
            else if (axis == 3u) { ab = diag_f(uu, vv, sg); }
            else if (axis == 4u) {
                let xx = abs(wrapd(uu + vv)) * sg;
                let yy = uu - vv;
                ab = vec2<f32>((yy + xx) * 0.5, (xx - yy) * 0.5);
            } else { ab = diag_f(fu, fv, sg); }
            let m = spA(ab.x, ab.y);
            if (tm == 0u) {
                o = m;
            } else {
                let q = spA(uu, vv);
                var r = q;
                for (var k = 0u; k < 3u; k = k + 1u) {
                    var v = q[k];
                    if (tm == 1u) { v = q[k] + m[k]; }
                    else if (tm == 2u) { v = q[k] - m[k]; }
                    else if (tm == 3u) { v = q[k] * m[k]; }
                    else if (tm == 4u) { v = q[k] + (m[k] - 0.5) * 2.0; }
                    else if (tm == 5u) { v = max(q[k], m[k]); }
                    else { v = min(q[k], m[k]); }
                    r[k] = clamp(v, 0.0, 1.0);
                }
                o = r;
            }
        }
        case 17u: {
            // skew: kx, ky, pivot
            o = cuA(uu + P(0u) * (vv - P(2u)), vv + P(1u) * (uu - P(2u)));
        }
        case 18u: {
            // mosaic grid: count
            let c = P(0u);
            o = spA((floor(uu * c) + 0.5) / c, (floor(vv * c) + 0.5) / c);
        }
        case 19u: {
            // non_uniform_directional_warp: ia = picture, ib = intensity map, ic = angle map (has_angle in P2); k, base
            var a = P(1u);
            if (P(2u) > 0.5) { a = a + (lum(spC(uu, vv)) - 0.5) * TAU; }
            let d = (lum(spB(uu, vv)) - 0.5) * 2.0 * P(0u);
            o = cuA(uu + cos(a) * d, vv + sin(a) * d);
        }
        case 20u: {
            // multi_directional_warp (average / max / min): ia picture, ib warp; k, a0, n, mode
            let n = i32(P(2u));
            let mode = u32(P(3u));
            let d = (lum(spB(uu, vv)) - 0.5) * 2.0 * P(0u);
            var acc = vec4<f32>(0.0);
            if (mode == 1u) { acc = vec4<f32>(-3.0e38); }
            if (mode == 2u) { acc = vec4<f32>(3.0e38); }
            for (var k = 0; k < n; k = k + 1) {
                let a = (P(1u) + f32(k) / f32(n)) * TAU;
                let p = spA(uu + cos(a) * d, vv + sin(a) * d);
                if (mode == 1u) { acc = max(acc, p); }
                else if (mode == 2u) { acc = min(acc, p); }
                else { acc = acc + p / f32(n); }
            }
            o = acc;
        }
        case 21u: {
            // non_uniform_blur accumulate: ia accumulator, ib blur level i, ic mask; level index i
            let k = clamp(lum(ic[i]), 0.0, 1.0) * 4.0;
            let wgt = max(0.0, 1.0 - abs(k - P(0u)));
            if (P(0u) < 0.5) { o = ib[i] * wgt; } else { o = ia[i] + ib[i] * wgt; }
        }
        case 22u: {
            // normal_facing: dir(0..2), sharpness
            let nv = dec3(ia[i]);
            let l = max(length(nv), 1e-6);
            let dt = dot(nv, vec3<f32>(P(0u), P(1u), P(2u))) / l;
            o = grey(pow(max(dt, 0.0), P(3u)));
        }
        case 23u: {
            // height_to_normal_world: factor (height_cm / pitch), directx
            let dx = (gA(x + 1, y) - gA(x - 1, y)) * 0.5 * P(0u);
            var dy = (gA(x, y + 1) - gA(x, y - 1)) * 0.5 * P(0u);
            if (P(1u) > 0.5) { dy = -dy; }
            o = enc3(vec3<f32>(-dx, dy, 1.0), 1.0);
        }
        case 24u: {
            // normal_sobel: intensity, curvature mode, ysgn
            var gx = vec3<f32>(0.0);
            var gy = vec3<f32>(0.0);
            let a = dec3(txA(x - 1, y - 1)); let b = dec3(txA(x, y - 1)); let c = dec3(txA(x + 1, y - 1));
            let d = dec3(txA(x - 1, y));                                     let f = dec3(txA(x + 1, y));
            let g = dec3(txA(x - 1, y + 1)); let h = dec3(txA(x, y + 1)); let k = dec3(txA(x + 1, y + 1));
            gx = (c - a + 2.0 * (f - d) + k - g) / 8.0;
            gy = (g - a + 2.0 * (h - b) + k - c) / 8.0;
            if (P(1u) > 0.5) {
                o = grey((gx.x + P(2u) * gy.y) * P(0u) * 0.5 + 0.5);
            } else {
                o = grey(sqrt(dot(gx, gx) + dot(gy, gy)) * P(0u));
            }
        }
        case 25u: {
            // normal_transform vector rotation: sin, cos, sign
            let p = ia[i];
            let nx = (p.x - 0.5) * 2.0;
            let ny = (p.y - 0.5) * 2.0 * P(2u);
            let rx = P(1u) * nx + P(0u) * ny;
            let ry = -P(0u) * nx + P(1u) * ny;
            o = enc3(vec3<f32>(rx, ry * P(2u), (p.z - 0.5) * 2.0), p.w);
        }
        case 26u: {
            // height_normal_blend: ia base normal, ib detail normal; mode, opacity
            let p = spA(uu, vv);
            let a = dec3(p);
            let b = dec3(spB(uu, vv));
            var r = vec3<f32>(0.0);
            let mode = u32(P(0u));
            if (mode == 0u) {
                let t = vec3<f32>(a.x, a.y, a.z + 1.0);
                let uv3 = vec3<f32>(-b.x, -b.y, b.z);
                let k = dot(t, uv3) / max(t.z, 1e-4);
                r = t * k - uv3;
            } else if (mode == 1u) {
                r = vec3<f32>(a.x + b.x, a.y + b.y, a.z * b.z);
            } else {
                r = vec3<f32>(a.x / max(a.z, 1e-4) + b.x / max(b.z, 1e-4), a.y / max(a.z, 1e-4) + b.y / max(b.z, 1e-4), 1.0);
            }
            o = lerp4(p, enc3(r, p.w), P(1u));
        }
        case 27u: {
            // shift (bilinear): dx, dy in uv
            o = spA(uu - P(0u), vv - P(1u));
        }
        case 28u: {
            // shape_drop_shadow combine: ia shape, ib blurred shifted; opacity, spread
            let sv = min(lum(spB(uu, vv)) / (1.0 - P(1u) * 0.99), 1.0);
            o = grey(clamp(max(lum(spA(uu, vv)), sv * P(0u)), 0.0, 1.0));
        }
        default: {}
    }
    if (u.n == 0xFFFFFFFFu) { o = o + ia[0] * 0.0 + ib[0] * 0.0 + ic[0] * 0.0 + idd[0] * 0.0; }
    dst[i] = o;
}
