// Disorder: the sampling position is displaced by a smooth tileable field (noise::Disorder); amplitude 0 = off.
fn warp(uu: f32, vv: f32) -> vec2<f32> {
    let amp = P(24u);
    if (amp <= 0.0) { return vec2<f32>(uu, vv); }
    var dx = (perlin(uu, vv, 3, 3, u.n + 101u) - 0.5) * amp;
    var dy = (perlin(uu, vv, 3, 3, u.n + 202u) - 0.5) * amp;
    let an = P(25u);
    if (an > 0.0) {
        let along = (dx * P(26u) + dy * P(27u)) * 1.41421356237;
        dx = dx + (along * P(26u) - dx) * an;
        dy = dy + (along * P(27u) - dy) * an;
    }
    return vec2<f32>(uu + dx, vv + dy);
}
// Contrast (stored minus one) and Invert of the generators
fn fin(g: f32) -> f32 {
    let dc = P(29u);
    let inv = P(30u) > 0.5;
    if (abs(dc) > 1e-4 || inv) {
        let v = clamp((g - 0.5) * (1.0 + dc) + 0.5, 0.0, 1.0);
        if (inv) { return 1.0 - v; }
        return v;
    }
    return g;
}
fn polygon_sdf(x: f32, y: f32, sides: f32, radius: f32, rot: f32) -> f32 {
    let n = max(rnd_away(sides), 3.0);
    let a = atan2(y, x) - rot * TAU;
    let sector = TAU / n;
    let r = sqrt(x * x + y * y);
    let local = (a - sector * floor(a / sector)) - sector * 0.5;
    return r * cos(local) - radius * cos(PI / n);
}
fn star_at(x: f32, y: f32) -> f32 {
    let pts = P(0u);
    let inner = P(1u);
    let size = P(2u);
    let so = P(3u);
    let rot = P(4u);
    let a = atan2(y, x) - rot * TAU + HALF_PI;
    let seg = TAU / pts;
    let t = (a - seg * floor(a / seg)) / seg;
    if (P(5u) > 0.5) {
        let r = sqrt(x * x + y * y);
        let h = abs(t - 0.5) * seg;
        let px = r * cos(h);
        let py = r * sin(h);
        let ox = size;
        let ix = size * inner * cos(seg * 0.5);
        let iy = size * inner * sin(seg * 0.5);
        let ex = ix - ox;
        let ey = iy;
        let l = max(sqrt(ex * ex + ey * ey), 1e-6);
        var d = ((px - ox) * ey - py * ex) / l;
        if ((0.0 - ox) * ey > 0.0) { d = -d; }
        return 1.0 - smoothc(-so, 0.0, d);
    }
    let edge = size * inner + (size - size * inner) * (1.0 - abs(2.0 * t - 1.0));
    return 1.0 - smoothc(-so, 0.0, sqrt(x * x + y * y) - edge);
}
fn poly_at(x: f32, y: f32) -> f32 {
    let so = P(2u);
    let d = polygon_sdf(x, y, P(0u), P(1u), P(3u));
    let hard = 1.0 - smoothc(-so, 0.0, d);
    let grad = P(4u);
    if (grad <= 0.0) { return hard; }
    var ramp = clamp(-d / P(8u), 0.0, 1.0);
    if (P(6u) > 0.5) { ramp = 1.0 - ramp; }
    let curve = P(5u);
    if (abs(curve) > 1e-4) { ramp = pow(ramp, pow(2.0, -curve * 2.0)); }
    return hard * (1.0 - grad + grad * ramp);
}
fn circle_at(x: f32, y: f32) -> f32 {
    return 1.0 - smoothc(P(0u) - P(1u), P(0u), sqrt(x * x + y * y));
}
fn shape_val(uu: f32, vv: f32) -> f32 {
    let kind = u32(P(0u));
    let sx = P(1u);
    let sy = P(2u);
    let rot = P(3u);
    let px = P(4u);
    let py = P(5u);
    let t = clamp(rnd_away(P(6u)), 1.0, 32.0);
    let so = max(clamp(P(7u), 0.0, 1.0), 1e-3);
    let x = rem1((uu - 0.5 - px) * t + 0.5) - 0.5;
    let y = rem1((vv - 0.5 - py) * t + 0.5) - 0.5;
    let s = sin(rot * TAU);
    let c = cos(rot * TAU);
    let rx = c * x + s * y;
    let ry = -s * x + c * y;
    let nx = rx / (max(sx, 1e-3) * 0.5);
    let ny = ry / (max(sy, 1e-3) * 0.5);
    let r = sqrt(nx * nx + ny * ny);
    switch (kind) {
        case 0u: { return 1.0 - smoothc(1.0 - so, 1.0, max(abs(nx), abs(ny))); }
        case 1u: { return 1.0 - smoothc(1.0 - so, 1.0, r); }
        case 2u: { return clamp(1.0 - r * r, 0.0, 1.0); }
        case 3u: { let q = r * 1.8; return exp(-(q * q)); }
        case 4u: { return 1.0 - smoothc(1.0 - so, 1.0, abs(nx) + abs(ny)); }
        case 5u: { return clamp(1.0 - max(abs(nx), abs(ny)), 0.0, 1.0); }
        case 6u: { return sqrt(max(1.0 - r * r, 0.0)); }
        default: { return clamp(1.0 - r, 0.0, 1.0); }
    }
}
fn checker_val(uu: f32, vv: f32) -> f32 {
    let t = P(0u);
    let white_first = i32(P(1u));
    let r45 = P(2u) > 0.5;
    let lv = P(3u);
    let aa = P(4u);
    var x = uu * t;
    var y = vv * t;
    if (r45) {
        x = (uu + vv) * t * 0.5;
        y = (vv - uu) * t * 0.5;
    }
    let fxl = floor(x);
    let fyl = floor(y);
    let fx = x - fxl;
    let fy = y - fyl;
    let ix = i32(fxl);
    let iy = i32(fyl);
    let sx = min(min(fx, 1.0 - fx) / aa, 1.0) * 0.5 + 0.5;
    let sy = min(min(fy, 1.0 - fy) / aa, 1.0) * 0.5 + 0.5;
    let base = f32(((ix + iy) & 1) ^ white_first);
    let kx = 1.0 - sx;
    let ky = 1.0 - sy;
    let v1 = base + (1.0 - 2.0 * base) * kx;
    let v2 = v1 + (1.0 - 2.0 * v1) * ky;
    if (lv <= 0.0) { return clamp(v2, 0.0, 1.0); }
    var wx = ix;
    var wy = iy;
    if (base <= 0.5) {
        if (min(fx, 1.0 - fx) < min(fy, 1.0 - fy)) {
            if (fx < 0.5) { wx = ix - 1; } else { wx = ix + 1; }
        } else {
            if (fy < 0.5) { wy = iy - 1; } else { wy = iy + 1; }
        }
    }
    let n = i32(P(5u));
    let r = unitp(remi(wx, n), remi(wy, n), u.n);
    return clamp(v2 * (1.0 - lv * r), 0.0, 1.0);
}
fn vor_one(uu0: f32, vv0: f32, px: i32, sd: u32) -> f32 {
    let aspect = P(4u);
    let py = max(i32(rnd_away(f32(px) * aspect)), 1);
    var uu = uu0 + P(5u);
    var vv = vv0 + P(6u);
    let dist = P(7u);
    if (dist > 0.0) {
        let ds = i32(P(8u));
        let nu = uu + (perlin(uu, vv, ds, ds, u.n + 5u) - 0.5) * dist * 0.3;
        let nv = vv + (perlin(uu, vv, ds, ds, u.n + 6u) - 0.5) * dist * 0.3;
        uu = nu;
        vv = nv;
    }
    let r = voronoi_gen(uu, vv, px, py, sd, PU(14u) + (sd - u.n), P(1u), u32(P(9u)), P(10u));
    return clamp(voronoi_style(r, u32(P(2u)), P(11u), P(12u)) * P(13u), 0.0, 1.0);
}
fn voronoi_val(uu: f32, vv: f32) -> f32 {
    let p = i32(P(0u));
    if (P(3u) < 0.5) { return vor_one(uu, vv, p, u.n); }
    let lo = i32(P(15u));
    let hi = max(i32(P(16u)), lo);
    let rough = P(17u);
    let lac = P(18u);
    let opac = P(19u);
    let blend = u32(P(20u));
    var acc = 0.0;
    if (blend == 2u) { acc = 1.0; }
    var amp = 1.0;
    var lp = 1.0;
    var norm = 0.0;
    var i = 0u;
    // levels below `lo` are skipped but still scale the period (lacunarity^l) like the host
    for (var l = 0; l < lo; l = l + 1) { lp = lp * lac; }
    for (var l = lo; l <= hi; l = l + 1) {
        let px = i32(clamp(rnd_away(f32(p) * lp), 1.0, 512.0));
        let val = vor_one(uu, vv, px, u.n + i * 131u);
        var o = 1.0;
        if (i > 0u) { o = opac; }
        if (blend == 1u) {
            acc = max(acc, val * o);
        } else if (blend == 2u) {
            acc = min(acc, 1.0 - (1.0 - val) * o);
        } else {
            acc = acc + amp * val * o;
        }
        norm = norm + amp;
        amp = amp * rough;
        lp = lp * lac;
        i = i + 1u;
    }
    if (blend == 0u) { acc = acc / max(norm, 1e-6); }
    return clamp(acc, 0.0, 1.0);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) { return; }
    let i = id.y * u.w + id.x;
    let uu = (f32(id.x) + 0.5) / f32(u.w);
    let vv = (f32(id.y) + 0.5) / f32(u.h);
    let seed = u.n;
    var g = 0.0;
    switch (u.op) {
        case 0u: {
            // white noise: distribution (0 uniform, 1 bell, 2 triangle); positions use the width for both axes
            let x = i32(uu * f32(u.w));
            let y = i32(vv * f32(u.w));
            let dist = u32(P(0u));
            if (dist == 1u) {
                g = (rnd(x, y, seed) + rnd(x, y, seed + 17u) + rnd(x, y, seed + 34u) + rnd(x, y, seed + 51u)) / 4.0;
            } else if (dist == 2u) {
                g = (rnd(x, y, seed) + rnd(x, y, seed + 17u)) * 0.5;
            } else {
                g = rnd(x, y, seed);
            }
        }
        case 1u: {
            // gaussian noise on cells of `scale` pixels
            let w = warp(uu, vv);
            let s = i32(u.w);
            let k = max(i32(P(0u)), 1);
            let x = remi(i32(floor(w.x * f32(s))), s) / k;
            let y = remi(i32(floor(w.y * f32(s))), s) / k;
            let sum = rnd(x, y, seed) + rnd(x, y, seed + 17u) + rnd(x, y, seed + 34u) + rnd(x, y, seed + 51u);
            g = fin(clamp((sum / 4.0 - 0.5) * 3.46 + 0.5, 0.0, 1.0));
        }
        case 2u: {
            let w = warp(uu, vv);
            let sc = P(0u);
            let p = sin(TAU * (sc * w.x + 0.35 * sin(TAU * (sc + 1.0) * w.y))) + sin(TAU * (sc * w.y + 0.35 * sin(TAU * (sc + 2.0) * w.x))) + sin(TAU * ((sc + 1.0) * (w.x + w.y)));
            g = fin(clamp(0.5 + p / 6.0, 0.0, 1.0));
        }
        case 3u: {
            // stripes: a, b, phase, softness, width
            let t = rem1(uu * P(0u) + vv * P(1u) - P(2u));
            g = smoothc(0.0, P(3u), t) * (1.0 - smoothc(P(4u), P(4u) + P(3u), t));
        }
        case 4u: {
            // weave: count, width
            let cnt = P(0u);
            let wd = P(1u);
            let x = uu * cnt;
            let y = vv * cnt;
            let fi = floor(x);
            let fj = floor(y);
            let fx = x - fi;
            let fy = y - fj;
            let hv = sqrt(1.0 - clamp(abs(2.0 * fx - 1.0) / wd, 0.0, 1.0));
            let hh = sqrt(1.0 - clamp(abs(2.0 * fy - 1.0) / wd, 0.0, 1.0));
            if (((i32(fi) + i32(fj)) & 1) == 0) { g = max(hv, 0.5 * hh); } else { g = max(hh, 0.5 * hv); }
        }
        case 5u: { g = shape_val(uu, vv); }
        case 6u: {
            // star (the union of the 3 x 3 copies when it is larger than half a tile)
            let x = wrapd(uu - 0.5);
            let y = wrapd(vv - 0.5);
            if (P(2u) > 0.5) {
                var m = -3.4e38;
                for (var dy = -1; dy <= 1; dy = dy + 1) {
                    for (var dx = -1; dx <= 1; dx = dx + 1) { m = max(m, star_at(x + f32(dx), y + f32(dy))); }
                }
                g = m;
            } else {
                g = star_at(x, y);
            }
        }
        case 7u: {
            // axial gradient, folded 0 -> 1 -> 0 along a whole wave vector (a, b)
            g = 1.0 - abs(2.0 * rem1((uu - 0.5) * P(0u) + (vv - 0.5) * P(1u) + 0.5) - 1.0);
        }
        case 8u: {
            let x = wrapd(uu - 0.5);
            let y = wrapd(vv - 0.5);
            let a = rem1(atan2(y, x) / TAU + 0.5 + P(0u));
            let w = 1.0 - smoothc(0.3, 0.5, max(abs(x), abs(y)));
            g = 0.5 + (a - 0.5) * w;
        }
        case 9u: {
            // gradient node: mode 1 radial, otherwise axial (a, b)
            if (P(2u) > 0.5) {
                let x = wrapd(uu - 0.5);
                let y = wrapd(vv - 0.5);
                g = clamp(sqrt(x * x + y * y) * 2.0, 0.0, 1.0);
            } else {
                g = 1.0 - abs(2.0 * rem1((uu - 0.5) * P(0u) + (vv - 0.5) * P(1u) + 0.5) - 1.0);
            }
        }
        case 10u: {
            // circle: radius, softness
            let x = wrapd(uu - 0.5);
            let y = wrapd(vv - 0.5);
            if (P(0u) > 0.5) {
                var m = -3.4e38;
                for (var dy = -1; dy <= 1; dy = dy + 1) {
                    for (var dx = -1; dx <= 1; dx = dx + 1) { m = max(m, circle_at(x + f32(dx), y + f32(dy))); }
                }
                g = m;
            } else {
                g = circle_at(x, y);
            }
        }
        case 11u: {
            let d = max(abs(wrapd(uu - 0.5)) - P(0u) * 0.5, abs(wrapd(vv - 0.5)) - P(1u) * 0.5);
            g = 1.0 - smoothc(-P(2u), 0.0, d);
        }
        case 12u: {
            // polygon: P(7) tiling; large shapes take the union of the 3 x 3 copies
            let til = P(7u);
            let x = wrapd(rem1(uu * til) - 0.5);
            let y = wrapd(rem1(vv * til) - 0.5);
            if (P(1u) > 0.5) {
                var m = -3.4e38;
                for (var dy = -1; dy <= 1; dy = dy + 1) {
                    for (var dx = -1; dx <= 1; dx = dx + 1) { m = max(m, poly_at(x + f32(dx), y + f32(dy))); }
                }
                g = m;
            } else {
                g = poly_at(x, y);
            }
        }
        case 13u: { g = checker_val(uu, vv); }
        case 14u: {
            // bricks: rows, cols, mortar, bevel, offset, luminance variation
            let rows = P(0u);
            let cols = P(1u);
            let row = floor(vv * rows);
            var shift = 0.0;
            if ((i32(row) % 2) != 0) { shift = P(4u); }
            let cx = uu * cols + shift;
            let col = floor(cx);
            let fu = cx - col;
            let fv = vv * rows - row;
            let e = min(min(fu, 1.0 - fu), min(fv, 1.0 - fv));
            let hgt = clamp((e - P(2u)) / max(P(3u), 1e-4), 0.0, 1.0);
            let idv = rnd(remi(i32(col), i32(cols)), i32(row), seed);
            g = hgt * (1.0 - P(5u) + P(5u) * idv);
        }
        case 15u: {
            // two-point gradient
            let p1 = vec2<f32>(P(0u), P(1u));
            let p2 = vec2<f32>(P(2u), P(3u));
            let mode = i32(P(4u));
            let til = P(5u);
            let peak = P(7u);
            let dx = p2.x - p1.x;
            let dy = p2.y - p1.y;
            let len2 = max(dx * dx + dy * dy, 1e-8);
            let len = sqrt(len2);
            let x = uu - p1.x;
            let y = vv - p1.y;
            var t = 0.0;
            if (mode == 0 || mode == 1) {
                t = (x * dx + y * dy) / len2;
                if (til > 1.0) { t = rem1(t * til); } else { t = clamp(t, 0.0, 1.0); }
                if (mode == 1) {
                    if (t <= peak) {
                        if (peak > 0.0) { t = t / peak; } else { t = 1.0; }
                    } else if (peak < 1.0) {
                        t = (1.0 - t) / (1.0 - peak);
                    } else {
                        t = 1.0;
                    }
                }
            } else if (mode == 2) {
                let a0 = atan2(-dy, dx) + P(9u) * TAU;
                let a = atan2(-y, x);
                t = 1.0 - rem1((a - a0) / TAU);
            } else {
                var d = sqrt(x * x + y * y);
                if (P(6u) > 0.5) { d = max(abs(x), abs(y)); }
                t = 1.0 - d / max(len, 1e-6);
            }
            t = clamp(t, 0.0, 1.0);
            let prof = i32(P(8u));
            if (prof == 1) { t = 1.0 - (1.0 - t) * (1.0 - t); } else if (prof == 2) { t = t * t; } else if (prof == 3) { t = t * t * (3.0 - 2.0 * t); }
            g = fin(t);
        }
        case 16u: {
            let w = warp(uu, vv);
            g = fin(voronoi_val(w.x, w.y));
        }
        case 17u: {
            // cells: variant, period x, period y, jitter, luminance random, edge width, hardness, colour source, pseudo seed, extra output
            let w = warp(uu, vv);
            let p = i32(P(1u));
            let py = i32(P(2u));
            let vr = voronoi_gen(w.x, w.y, p, py, seed, seed, P(3u), 0u, 2.0);
            let extra = i32(P(9u));
            var cell = vr.id1;
            if (u32(P(7u)) == 1u) {
                cell = rnd(i32(floor((w.x + vr.dx / f32(p)) * f32(p))), i32(floor((w.y + vr.dy / f32(p)) * f32(p))), PU(8u) + 4242u);
            }
            let edge = smoothc(0.0, P(5u), vr.f2 - vr.f1);
            if (extra == 1) {
                g = vr.id1;
            } else if (extra == 2) {
                g = vr.f1;
            } else if (extra == 3) {
                g = edge;
            } else {
                let var_ = u32(P(0u));
                if (var_ == 0u) { g = (1.0 - vr.f1) * (1.0 - P(4u) * vr.id1); }
                else if (var_ == 1u) { g = cell; }
                else if (var_ == 2u) { g = edge; }
                else { g = vr.id1 * (1.0 - pow(vr.f1, P(6u))); }
            }
            g = fin(g);
        }
        case 18u: {
            // BnW spots: variant, period
            let w = warp(uu, vv);
            let vari = u32(P(0u));
            let p = i32(P(1u));
            var grain = 0.0;
            if (vari >= 2u) { grain = (fbm_n(uu, vv, p * 4, p * 4, 3u, 0.5, seed + 5u, 0u) - 0.5) * 0.25; }
            g = fin(spots(w.x, w.y, p, p, seed, min(vari, 2u), 0.0, 0.0, grain));
        }
        case 19u: {
            // crystal: variant, period
            let w = warp(uu, vv);
            let p = i32(P(1u));
            let vr = voronoi_gen(w.x, w.y, p, p, seed, seed, 1.0, 0u, 2.0);
            let l = max(sqrt(vr.dx * vr.dx + vr.dy * vr.dy), 1e-4);
            var facet = 1.0 - vr.f1;
            if (u32(P(0u)) != 0u) { facet = 0.5 + 0.5 * ((vr.dx / l) * 0.6 + (vr.dy / l) * 0.8); }
            let edge = max(smoothc(0.0, 0.08, vr.f2 - vr.f1), 0.2);
            g = fin((vr.id1 * 0.55 + facet * 0.45) * edge);
        }
        case 20u: {
            // waveform (plain): wave vector a, b, warp amount, warp scale, shape
            let w = warp(uu, vv);
            let ph = P(0u) * w.x + P(1u) * w.y + (perlin(w.x, w.y, i32(P(3u)), i32(P(3u)), seed) - 0.5) * P(2u);
            let x = ph - floor(ph);
            let sh = u32(P(4u));
            if (sh == 1u) { g = 1.0 - abs(x * 2.0 - 1.0); }
            else if (sh == 2u) { g = x; }
            else if (sh == 3u) { g = select(0.0, 1.0, x < 0.5); }
            else { g = 0.5 + 0.5 * sin(x * TAU); }
            g = fin(g);
        }
        case 21u: {
            // waveform (sound bars): samples, waves, size min, size max, noise, intensity, pixel height
            let samples = P(0u);
            let ns = i32(samples);
            let ix = remi(i32(floor(uu * samples)), ns);
            let uc = (f32(ix) + 0.5) / samples;
            let q = uc * P(1u);
            let s1 = sin(PI * (q - trunc(q)));
            let env = s1 * s1;
            let r = unitp(ix, 0, seed);
            let half = 0.125 * (P(2u) + (P(3u) - P(2u)) * env * (1.0 - P(4u) * r));
            if (half <= 0.0) {
                g = fin(0.0);
            } else {
                let sigma = max(half / 1.1774, P(6u) * 0.5);
                let z = (vv - 0.5) / sigma;
                g = fin(P(5u) * exp(-0.5 * z * z));
            }
        }
        case 22u: {
            // gaussian spots: period, size, size variation
            let w = warp(uu, vv);
            let p = i32(P(0u));
            g = fin(spots(w.x, w.y, p, p, seed, 3u, P(1u), P(2u), 0.0));
        }
        case 23u: {
            // fractal sum: period, lowest level, highest level, roughness, mode
            let w = warp(uu, vv);
            let p = P(0u);
            let lo = i32(P(1u));
            let hi = max(i32(P(2u)), lo);
            let rough = P(3u);
            let mode = u32(P(4u));
            var acc = 0.0;
            var norm = 0.0;
            var amp = 1.0;
            for (var o = lo; o <= hi; o = o + 1) {
                let q = i32(clamp(p * f32(1u << u32(o)), 1.0, 1024.0));
                let x = perlin(w.x, w.y, q, q, seed + u32(o) * 31u);
                var v = x;
                if (mode == 1u) { v = abs(x * 2.0 - 1.0); } else if (mode == 2u) { v = 1.0 - abs(x * 2.0 - 1.0); }
                acc = acc + amp * v;
                norm = norm + amp;
                amp = amp * rough;
            }
            g = fin(clamp(acc / max(norm, 1e-6), 0.0, 1.0));
        }
        case 24u: {
            // gradient_reflected: whole wave vector (a, b)
            g = fin(1.0 - abs(2.0 * rem1(P(0u) * uu + P(1u) * vv) - 1.0));
        }
        case 25u: {
            // triangle_grid (plain style): count, border, variation
            let c = i32(P(0u));
            let x = uu * P(0u);
            let y = vv * P(0u);
            let ix = floor(x);
            let iy = floor(y);
            let fx = x - ix;
            let fy = y - iy;
            var tri = 0u;
            if (fx + fy >= 1.0) { tri = 1u; }
            let d = min(min(min(fx, fy), min(1.0 - fx, 1.0 - fy)), abs(fx + fy - 1.0) * 0.7071);
            let id = rnd(remi(i32(ix), c), remi(i32(iy), c), seed + tri * 131u);
            g = fin((1.0 - P(2u) + P(2u) * id) * smoothc(0.0, max(P(1u), 1.0e-4), d));
        }
        case 26u: {
            // weave_generator (plain style): nx, ny, shape, gap_x, gap_y, output, apply contrast / invert
            let nxi = i32(P(0u));
            let nyi = i32(P(1u));
            let x = uu * P(0u);
            let y = vv * P(1u);
            let fi = floor(x);
            let fj = floor(y);
            let fx = x - fi;
            let fy = y - fj;
            let i = remi(i32(fi), nxi);
            let j = remi(i32(fj), nyi);
            let in_gx = fx < P(3u) * 0.5 || fx > 1.0 - P(3u) * 0.5;
            let in_gy = fy < P(4u) * 0.5 || fy > 1.0 - P(4u) * 0.5;
            let thread = !in_gx && !in_gy;
            let horizontal = ((i + j) & 1) == 0;
            let which = u32(P(5u));
            var v = 0.0;
            var done = false;
            if (which == 1u) { v = select(0.0, 1.0, thread && horizontal); done = true; }
            else if (which == 2u) { v = select(0.0, 1.0, thread && !horizontal); done = true; }
            else if (which == 3u) { v = select(0.0, 1.0, thread); done = true; }
            if (!done) {
                if (in_gx && in_gy) { v = 0.0; }
                else if (in_gx || in_gy) { v = 0.5; }
                else {
                    var along = fy;
                    if (horizontal) { along = fx; }
                    let e = abs(along - 0.5) * 2.0;
                    if (P(2u) <= 0.0) { v = 1.0; } else { v = 1.0 - 0.5 * smoothc(1.0 - P(2u), 1.0, e); }
                }
            }
            if (which == 0u) { g = fin(v); } else { g = v; }
        }
        case 27u: {
            // starburst (plain style): count, rays, sharpness, size, rotation, rotation_random
            let c = i32(P(0u));
            let x = uu * P(0u);
            let y = vv * P(0u);
            let ix = floor(x);
            let iy = floor(y);
            let dx = x - ix - 0.5;
            let dy = y - iy - 0.5;
            let r = sqrt(dx * dx + dy * dy);
            let off = P(4u) + P(5u) * rnd(remi(i32(ix), c), remi(i32(iy), c), seed);
            let a = (atan2(dy, dx) / TAU + off) * P(1u);
            let ray = 1.0 - abs(2.0 * (a - floor(a)) - 1.0);
            g = fin(pow(ray, P(2u)) * clamp(1.0 - r * 2.0 / P(3u), 0.0, 1.0));
        }
        case 30u: {
            let w = warp(uu, vv);
            g = fin(perlin(w.x, w.y, i32(P(0u)), i32(P(1u)), seed));
        }
        case 31u: {
            let w = warp(uu, vv);
            g = fin(fbm_n(w.x, w.y, i32(P(0u)), i32(P(1u)), u32(P(2u)), P(3u), seed, u32(P(4u))));
        }
        default: {}
    }
    dst[i] = vec4<f32>(g, g, g, 1.0);
}
