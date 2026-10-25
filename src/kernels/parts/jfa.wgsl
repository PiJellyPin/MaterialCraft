// signed distance by jump flooding. State per pixel: xy = offset to the nearest edge point (pixels), z = inside flag.
fn len2(o: vec2<f32>) -> f32 { return o.x * o.x + o.y * o.y; }
fn sdist(s: vec4<f32>) -> f32 {
    let d = sqrt(len2(s.xy));
    if (s.z > 0.5) { return d; }
    return -d;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= u.w || gid.y >= u.h) { return; }
    let i = gid.y * u.w + gid.x;
    let x = i32(gid.x);
    let y = i32(gid.y);
    let w = i32(u.w);
    let h = i32(u.h);
    var o = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    switch (u.op) {
        case 0u: {
            // seed: where the grey values cross the threshold on the way to a neighbour of the other side
            let thr = P(0u);
            let g = lum(ia[i]);
            let inside = g > thr;
            var best = vec2<f32>(1.0e9, 1.0e9);
            for (var k = 0; k < 4; k = k + 1) {
                var dx = 0;
                var dy = 0;
                if (k == 0) { dx = 1; }
                if (k == 1) { dx = -1; }
                if (k == 2) { dy = 1; }
                if (k == 3) { dy = -1; }
                let j = u32(remi(y + dy, h)) * u.w + u32(remi(x + dx, w));
                let gj = lum(ia[j]);
                if ((gj > thr) == inside) { continue; }
                let d = gj - g;
                var f = 0.5;
                if (abs(d) >= 1.0e-6) { f = clamp((thr - g) / d, 0.0, 1.0); }
                let c = vec2<f32>(f32(dx) * f, f32(dy) * f);
                if (len2(c) < len2(best)) { best = c; }
            }
            o = vec4<f32>(best, select(0.0, 1.0, inside), 1.0);
        }
        case 1u: {
            // one jump of P(0) pixels
            let st = i32(P(0u));
            let me = ia[i];
            var best = me.xy;
            for (var dj = -1; dj <= 1; dj = dj + 1) {
                for (var di = -1; di <= 1; di = di + 1) {
                    if (di == 0 && dj == 0) { continue; }
                    let j = u32(remi(y + dj * st, h)) * u.w + u32(remi(x + di * st, w));
                    let c = ia[j].xy + vec2<f32>(f32(di * st), f32(dj * st));
                    if (len2(c) < len2(best)) { best = c; }
                }
            }
            o = vec4<f32>(best, me.z, 1.0);
        }
        case 2u: {
            // Distance node: range in pixels, mode
            let d = sdist(ia[i]);
            let r = max(P(0u), 1.0e-3);
            let md = u32(P(1u));
            var g = 0.0;
            if (md == 0u) { g = clamp(d / r, 0.0, 1.0); }
            else if (md == 1u) { g = 1.0 - clamp(-d / r, 0.0, 1.0); }
            else { g = clamp(0.5 + d / r * 0.5, 0.0, 1.0); }
            o = grey(g);
        }
        case 3u: {
            // Bevel node: distance in pixels, smoothing, round
            let d = sdist(ia[i]);
            let outward = P(0u) < 0.0;
            let reach = max(abs(P(0u)), 1.0);
            var tt = 0.0;
            var done = false;
            if (outward) {
                if (d > 0.0) { tt = 1.0; } else { tt = 1.0 - clamp(-d / reach, 0.0, 1.0); }
            } else if (d <= 0.0) {
                o = vec4<f32>(0.0, 0.0, 0.0, 1.0);
                done = true;
            } else {
                tt = clamp(d / reach, 0.0, 1.0);
            }
            if (!done) {
                if (P(2u) > 0.5) { tt = sqrt(max(1.0 - (1.0 - tt) * (1.0 - tt), 0.0)); }
                let g = tt + (tt * tt * (3.0 - 2.0 * tt) - tt) * clamp(P(1u), 0.0, 1.0);
                o = grey(g);
            }
        }
        case 4u: {
            // shape_stroke / shape_glow: stroke flag, width, offset, softness, intensity, opacity, spread, colour (7..9)
            let sdv = sdist(ia[i]);
            let inside = sdv > 0.0;
            let sd = -clamp(sdv, -1.0e8, 1.0e8);
            let w = P(1u);
            let soft = P(3u);
            var g = 0.0;
            if (P(0u) > 0.5) {
                let c = P(2u) * w * 0.5;
                g = 1.0 - smoothc(w * 0.5 * (1.0 - soft), w * 0.5 + 0.5, abs(sd - c));
            } else {
                if (inside) { g = 1.0; } else { g = pow(1.0 - clamp(sd / w, 0.0, 1.0), 1.0 + soft * 3.0) * P(4u); }
                g = g / (1.0 - P(6u) * 0.99);
            }
            g = clamp(g, 0.0, 1.0) * P(5u);
            o = vec4<f32>(g * P(7u), g * P(8u), g * P(9u), 1.0);
        }
        default: {}
    }
    if (u.n == 0xFFFFFFFFu) { o = o + ia[0] * 0.0 + ib[0] * 0.0 + ic[0] * 0.0 + idd[0] * 0.0; }
    dst[i] = o;
}
