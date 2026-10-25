// material (PBR) utilities and mask recipes (ia, ib, ic = pictures, idd = a fourth picture read per pixel)
fn dec3(p: vec4<f32>) -> vec3<f32> { return (p.xyz - vec3<f32>(0.5)) * 2.0; }
fn enc3(v: vec3<f32>, a: f32) -> vec4<f32> {
    let l = length(v);
    if (l < 1e-8) { return vec4<f32>(0.5, 0.5, 1.0, a); }
    return vec4<f32>(v / l * 0.5 + vec3<f32>(0.5), a);
}
fn bright(c: vec3<f32>) -> f32 { return sqrt(0.299 * c.x * c.x + 0.587 * c.y * c.y + 0.114 * c.z * c.z); }
fn lut_at(base: u32, n: u32, comp: u32, x: f32) -> f32 {
    let f = clamp(x, 0.0, 1.0) * f32(n - 1u);
    let i = min(u32(f), n - 2u);
    let t = f - f32(i);
    let a = idd[base + i][comp];
    let b = idd[base + i + 1u][comp];
    return a + (b - a) * t;
}
fn solve_metallic(diffuse: f32, specular: f32, oms: f32) -> f32 {
    if (specular < 0.04) { return 0.0; }
    let a = 0.04;
    let b = diffuse * oms / 0.96 + specular - 0.08;
    let c = 0.04 - specular;
    let d = max(b * b - 4.0 * a * c, 0.0);
    return clamp((-b + sqrt(d)) / (2.0 * a), 0.0, 1.0);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= u.w || gid.y >= u.h) { return; }
    let i = gid.y * u.w + gid.x;
    let uu = (f32(gid.x) + 0.5) / f32(u.w);
    let vv = (f32(gid.y) + 0.5) / f32(u.h);
    var o = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    switch (u.op) {
        case 0u: {
            // material_blend weight: ia mask, ib bg height, ic fg height; has_mask, has_bh, has_fh, opacity, influence, sharpness
            var k = 1.0;
            if (P(0u) > 0.5) { k = lum(ia[i]); }
            k = clamp(k * P(3u), 0.0, 1.0);
            var ha = 0.5;
            var hf = 0.5;
            if (P(1u) > 0.5) { ha = lum(ib[i]); }
            if (P(2u) > 0.5) { hf = lum(ic[i]); }
            var w = clamp(k + (hf - ha) * P(4u) * 4.0 * k * (1.0 - k), 0.0, 1.0);
            w = clamp((w - 0.5) * P(5u) + 0.5, 0.0, 1.0);
            o = grey(w);
            // heights are needed by the height output: carried in alpha-free form by the caller
        }
        case 1u: {
            // lerp colour: ia a, ib b, ic weight; has_a, has_b, default a (4..7), default b (8..11)
            var a = vec4<f32>(P(4u), P(5u), P(6u), P(7u));
            var b = vec4<f32>(P(8u), P(9u), P(10u), P(11u));
            if (P(0u) > 0.5) { a = ia[i]; }
            if (P(1u) > 0.5) { b = ib[i]; }
            o = lerp4(a, b, ic[i].x);
        }
        case 2u: {
            var a = vec3<f32>(0.0, 0.0, 1.0);
            var b = vec3<f32>(0.0, 0.0, 1.0);
            if (P(0u) > 0.5) { a = dec3(ia[i]); }
            if (P(1u) > 0.5) { b = dec3(ib[i]); }
            let w = ic[i].x;
            o = enc3(a + (b - a) * w, 1.0);
        }
        case 3u: {
            // lerp grey: default a (4), default b (5)
            var a = P(4u);
            var b = P(5u);
            if (P(0u) > 0.5) { a = lum(ia[i]); }
            if (P(1u) > 0.5) { b = lum(ib[i]); }
            o = grey(a + (b - a) * ic[i].x);
        }
        case 4u: {
            // metal_rough_to_spec_gloss: out, has_metal, has_rough
            let bse = ia[i];
            var m = 0.0;
            var r = 0.5;
            if (P(1u) > 0.5) { m = clamp(lum(ib[i]), 0.0, 1.0); }
            if (P(2u) > 0.5) { r = clamp(lum(ic[i]), 0.0, 1.0); }
            let k = u32(P(0u));
            if (k == 0u) { o = vec4<f32>(bse.xyz * (1.0 - m), bse.w); }
            else if (k == 1u) { o = vec4<f32>(vec3<f32>(0.04) + (bse.xyz - vec3<f32>(0.04)) * m, 1.0); }
            else { o = grey(1.0 - r); }
        }
        case 5u: {
            // spec_gloss_to_metal_rough: out, has_gloss
            let d = ia[i];
            let sp = ib[i];
            let oms = 1.0 - clamp(max(sp.x, max(sp.y, sp.z)), 0.0, 1.0);
            let m = solve_metallic(bright(d.xyz), bright(sp.xyz), oms);
            let k = u32(P(0u));
            if (k == 0u) {
                var r = vec3<f32>(0.0);
                for (var c = 0u; c < 3u; c = c + 1u) {
                    let fd = d[c] * (oms / 0.96) / max(1.0 - m, 1e-4);
                    let fs = (sp[c] - 0.04 * (1.0 - m)) / max(m, 1e-4);
                    r[c] = clamp(fd + (fs - fd) * (m * m), 0.0, 1.0);
                }
                o = vec4<f32>(r, d.w);
            } else if (k == 1u) {
                o = grey(m);
            } else {
                var g = 0.5;
                if (P(1u) > 0.5) { g = clamp(lum(ic[i]), 0.0, 1.0); }
                o = grey(1.0 - g);
            }
        }
        case 6u: {
            // leak_streaks: ia mask, ib noise; length(uv), sin, cos, intensity, breakup, has_noise, aspect
            var m = lum(spA(uu, vv));
            for (var k = 1; k <= 48; k = k + 1) {
                let q = f32(k) / 48.0;
                let d = P(0u) * q * q;
                let src = lum(spA(uu - P(2u) * d, vv - P(1u) * d * P(6u))) * (1.0 - q);
                m = max(m, src);
            }
            if (P(5u) > 0.5) { m = m * lerpf(1.0, lum(ib[i]), P(4u)); }
            o = grey(clamp(m * P(3u), 0.0, 1.0));
        }
        case 7u: {
            // settled_dust: ia height, ib normal of it, ic blurred height, idd noise; amount, slope, cavity, has_noise
            let up = (ib[i].z - 0.5) * 2.0;
            let settle = smoothc(1.0 - P(1u), 1.0, up);
            let cav = clamp((lum(ic[i]) - lum(ia[i])) * 6.0, 0.0, 1.0);
            var m = clamp(settle * P(0u) * (1.0 - P(2u) * 0.5) + cav * P(2u), 0.0, 1.0);
            if (P(3u) > 0.5) { m = m * lum(idd[i]); }
            o = grey(m);
        }
        case 8u: {
            // make_it_tile_mirror: mode
            let md = u32(P(0u));
            var su = abs(1.0 - 2.0 * uu);
            var sv = abs(1.0 - 2.0 * vv);
            if (md == 2u) { su = uu; }
            if (md == 1u) { sv = vv; }
            o = spA(su, sv);
        }
        case 9u: {
            // height_blend: ia bg, ib fg, ic mask; has_mask, opacity, contrast, output (0 height, 1 mask)
            var m = 1.0;
            if (P(0u) > 0.5) { m = lum(ic[i]); }
            m = m * P(1u);
            let b = lum(ia[i]);
            let g = lum(ib[i]);
            let score = (g + (m - 0.5) * 2.0) - b;
            let k = clamp(0.5 + score * max(P(2u), 0.05) * 2.0, 0.0, 1.0) * smoothc(0.0, 0.08, m);
            if (P(3u) > 0.5) { o = grey(k); } else { o = grey(b + (g - b) * k); }
        }
        case 10u: {
            // make_it_tile_photo: width_h, width_v
            let wh = clamp(P(0u), 0.1, 1.0);
            let wv = clamp(P(1u), 0.1, 1.0);
            let au = smoothc(0.0, wh, 1.0 - abs(2.0 * uu - 1.0));
            let av = smoothc(0.0, wv, 1.0 - abs(2.0 * vv - 1.0));
            let c00 = spA(rem1(uu), rem1(vv));
            let c10 = spA(rem1(uu + 0.5), rem1(vv));
            let c01 = spA(rem1(uu), rem1(vv + 0.5));
            let c11 = spA(rem1(uu + 0.5), rem1(vv + 0.5));
            o = c00 * (au * av) + c10 * ((1.0 - au) * av) + c01 * (au * (1.0 - av)) + c11 * ((1.0 - au) * (1.0 - av));
        }
        case 11u: {
            // multiply the colour by the luminance of ib
            let p = ia[i];
            o = vec4<f32>(p.xyz * lum(ib[i]), p.w);
        }
        case 12u: {
            // pack the luminance of four pictures into one
            o = vec4<f32>(lum(ia[i]), lum(ib[i]), lum(ic[i]), lum(idd[i]));
        }
        case 13u: {
            // curve: idd holds n entries (master, r, g, b) and then n entries (a); n = P(0)
            let n = u32(P(0u));
            let p = ia[i];
            var r = vec4<f32>(0.0);
            for (var k = 0u; k < 3u; k = k + 1u) {
                r[k] = lut_at(0u, n, 1u + k, lut_at(0u, n, 0u, p[k]));
            }
            r.w = lut_at(n, n, 0u, p.w);
            o = r;
        }
        case 14u: {
            // flood_fill_random: P0 lo, P1 hi, P2 contrast spread; seed in n
            let q = ia[i];
            if (!(q.x == 0.0 && q.y == 0.0 && q.z == 0.0)) {
                let id = i32(q.x * 100000.0);
                let r = clamp(0.5 + (rnd(id, 5, u.n) - 0.5) * P(2u), 0.0, 1.0);
                o = grey(P(0u) + (P(1u) - P(0u)) * r);
            }
        }
        case 15u: {
            let q = ia[i];
            if (!(q.x == 0.0 && q.y == 0.0 && q.z == 0.0)) {
                let id = i32(q.x * 100000.0);
                o = vec4<f32>(0.2 + 0.8 * rnd(id, 6, u.n), 0.2 + 0.8 * rnd(id, 7, u.n), 0.2 + 0.8 * rnd(id, 8, u.n), 1.0);
            }
        }
        case 16u: {
            // flood_fill_gradient: P0 angle (turns), P1 variation
            let q = ia[i];
            if (!(q.x == 0.0 && q.y == 0.0 && q.z == 0.0)) {
                let id = i32(q.x * 100000.0);
                let a = P(0u) * 6.2831855 + (rnd(id, 9, u.n) - 0.5) * P(1u) * 6.2831855;
                let sc = vec2<f32>(sin(a), cos(a));
                let g = clamp(0.5 + ((q.y - 0.5) * sc.y + (q.z - 0.5) * sc.x) / max(abs(sc.y) + abs(sc.x), 1e-3), 0.0, 1.0);
                o = grey(g);
            }
        }
        case 17u: {
            // morphology pass: P0 direction (0 across, 1 down), P1 maximum, P2 radius; minimum or maximum over a wrapped window
            let horiz = P(0u) < 0.5;
            let nn = i32(select(u.h, u.w, horiz));
            let r = min(i32(P(2u)), (nn - 1) / 2);
            let mx = P(1u) > 0.5;
            let x = i32(gid.x);
            let y = i32(gid.y);
            var acc = select(vec4<f32>(3.0e38), vec4<f32>(-3.0e38), mx);
            for (var k = -r; k <= r; k = k + 1) {
                var q: vec4<f32>;
                if (horiz) { q = ia[gid.y * u.w + u32(remi(x + k, nn))]; }
                else { q = ia[u32(remi(y + k, nn)) * u.w + gid.x]; }
                acc = select(min(acc, q), max(acc, q), mx);
            }
            o = acc;
        }
        case 18u: {
            // lookup_table: ia picture, ib lookup image; P0 by luminance, P1 vertical, P2 repeat, P3 line, P4 length
            let vertical = P(1u) > 0.5;
            let len = u32(P(4u));
            let line = u32(P(3u));
            let p = ia[i];
            var res = vec4<f32>(0.0);
            for (var m = 0u; m < 4u; m = m + 1u) {
                if (m == 3u) { break; }
                var x = p[m];
                if (P(0u) > 0.5) { x = lum(p); }
                if (P(2u) > 0.5) { x = x - floor(x); } else { x = clamp(x, 0.0, 1.0); }
                let fx = x * f32(len - 1u);
                let i0 = min(u32(floor(fx)), len - 1u);
                let i1 = min(i0 + 1u, len - 1u);
                var a: vec4<f32>;
                var b: vec4<f32>;
                if (vertical) { a = ib[i0 * u.w + line]; b = ib[i1 * u.w + line]; }
                else { a = ib[line * u.w + i0]; b = ib[line * u.w + i1]; }
                res[m] = lerp4(a, b, fx - f32(i0))[m];
                if (P(0u) > 0.5) {
                    // by luminance: all three channels come from the one lookup
                    let c = lerp4(a, b, fx - f32(i0));
                    res = vec4<f32>(c.xyz, 0.0);
                    break;
                }
            }
            o = vec4<f32>(res.xyz, p.w);
        }
        case 19u: {
            // histogram_equalize: idd holds the curves (1024 x r, g, b, luminance; negative = leave alone); P0 amount, P1 per channel
            let p = ia[i];
            let amount = P(0u);
            if (P(1u) > 0.5) {
                o = p;
                for (var c = 0u; c < 3u; c = c + 1u) {
                    if (idd[0][c] >= 0.0) {
                        let xx = clamp(p[c], 0.0, 1.0) * 1023.0;
                        let k = u32(floor(xx));
                        let k2 = min(k + 1u, 1023u);
                        let cv = lerpf(idd[k][c], idd[k2][c], xx - floor(xx));
                        o[c] = lerpf(p[c], cv, amount);
                    }
                }
            } else {
                o = p;
                if (idd[0].w >= 0.0) {
                    let l = lum(p);
                    let xx = clamp(l, 0.0, 1.0) * 1023.0;
                    let k = u32(floor(xx));
                    let k2 = min(k + 1u, 1023u);
                    let cv = lerpf(idd[k].w, idd[k2].w, xx - floor(xx));
                    let d = (cv - l) * amount;
                    o = vec4<f32>(p.x + d, p.y + d, p.z + d, p.w);
                }
            }
        }
        case 20u: {
            // auto_crop: idd[0] is the box of the bright area (x0, y0, x1, y1; negative = none); P0 margin, P1 keep aspect
            let b = idd[0];
            if (b.x < 0.0) { o = ia[i]; }
            else {
                let wf = f32(u.w);
                let hf = f32(u.h);
                var u0 = b.x / wf;
                var u1 = b.z / wf;
                var v0 = b.y / hf;
                var v1 = b.w / hf;
                u0 = u0 - P(0u);
                u1 = u1 + P(0u);
                v0 = v0 - P(0u);
                v1 = v1 + P(0u);
                if (P(1u) > 0.5) {
                    let pw = (u1 - u0) * wf;
                    let ph = (v1 - v0) * hf;
                    let want = wf / hf;
                    if (pw / max(ph, 1e-6) > want) {
                        let grow = pw / want / hf - (v1 - v0);
                        v0 = v0 - grow * 0.5;
                        v1 = v1 + grow * 0.5;
                    } else {
                        let grow = ph * want / wf - (u1 - u0);
                        u0 = u0 - grow * 0.5;
                        u1 = u1 + grow * 0.5;
                    }
                }
                o = spA(lerpf(u0, u1, uu), lerpf(v0, v1, vv));
            }
        }
        case 21u: {
            // halftone: P0 cells across, P1 cells down, P2 diagonal, P3 softness, P4 gain
            let nn = P(0u);
            let nyc = P(1u);
            let x = uu * nn;
            let y = vv * nyc;
            var cu: f32;
            var cv: f32;
            var d: f32;
            if (P(2u) > 0.5) {
                let a = x + y;
                let b = x - y;
                let ac = floor(a) + 0.5;
                let bc = floor(b) + 0.5;
                cu = (ac + bc) * 0.5 / nn;
                cv = (ac - bc) * 0.5 / nyc;
                d = sqrt((a - ac) * (a - ac) + (b - bc) * (b - bc));
            } else {
                let cx = floor(x) + 0.5;
                let cy = floor(y) + 0.5;
                cu = cx / nn;
                cv = cy / nyc;
                d = sqrt((x - cx) * (x - cx) + (y - cy) * (y - cy));
            }
            let l = clamp(lum(spA(cu, cv)) * P(4u), 0.0, 1.0);
            let r = sqrt(l) * 0.56;
            var k = 1.0;
            if (l <= 0.0) { k = 0.0; }
            o = grey(smoothc(r + P(3u) * 0.3 + 1e-3, max(r - P(3u) * 0.3, 0.0), d) * k);
        }
        case 22u: {
            // fxaa: P0 strength, P1 threshold
            let x = i32(gid.x);
            let y = i32(gid.y);
            let m = lum(txA(x, y));
            let n = lum(txA(x, y - 1));
            let so = lum(txA(x, y + 1));
            let e = lum(txA(x + 1, y));
            let we = lum(txA(x - 1, y));
            let hi = max(max(max(m, n), max(so, e)), we);
            let lo = min(min(min(m, n), min(so, e)), we);
            let range = hi - lo;
            o = txA(x, y);
            if (!(range < max(P(1u), hi * 0.125))) {
                let avg = (n + so + e + we) * 0.25;
                let bl = pow(smoothc(0.0, 1.0, clamp(abs(avg - m) / range, 0.0, 1.0)), 2.0) * 0.75 * P(0u) + smoothc(0.0, 0.5, range) * 0.25 * P(0u);
                var acc = vec4<f32>(0.0);
                for (var dy = -1; dy <= 1; dy = dy + 1) {
                    for (var dx = -1; dx <= 1; dx = dx + 1) { acc = acc + txA(x + dx, y + dy) / 9.0; }
                }
                o = lerp4(o, acc, bl);
            }
        }
        default: {}
    }
    if (u.n == 0xFFFFFFFFu) { o = o + ia[0] * 0.0 + ib[0] * 0.0 + ic[0] * 0.0 + idd[0] * 0.0; }
    dst[i] = o;
}
