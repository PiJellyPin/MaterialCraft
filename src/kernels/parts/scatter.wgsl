// draws pattern instances: tile grids (gather from the neighbouring cells), splatter and circular splatter
// (every instance). ia / ib = luminance of up to six patterns packed four per picture, ic = background,
// idd = instance records (3 vec4 per instance, see scatter.rs)
fn builtin_pat(kind: u32, lx: f32, ly: f32, spec: f32) -> f32 {
    let x = (lx - 0.5) * 2.0;
    let y = (ly - 0.5) * 2.0;
    let r = sqrt(x * x + y * y);
    var v = 1.0;
    switch (kind) {
        case 1u: { v = 1.0; }
        case 2u: { v = 1.0 - smoothc(0.85, 1.0, r); }
        case 3u: { v = 1.0 - r * r; }
        case 4u: { v = 0.5 + 0.5 * cos(min(r, 1.0) * PI); }
        case 5u: { let q = r * 2.5; v = exp(-(q * q)); }
        case 6u: { v = pow(max(1.0 - r, 0.0), 1.0 + spec * 6.0); }
        case 7u: { v = 1.0 - max(abs(x), abs(y)); }
        case 8u: {
            let e = min(1.0 - abs(x), 1.0 - abs(y));
            v = clamp(e / (0.04 + spec * 0.5), 0.0, 1.0);
        }
        case 9u: { v = lx; }
        case 10u: { v = 0.5 + 0.5 * cos(TAU * (1.0 + rnd_away(spec * 8.0)) * lx); }
        case 11u: { v = 0.5 + 0.5 * cos(min(abs(y), 1.0) * PI); }
        case 12u: {
            let b = 0.5 + 0.5 * cos(min(r, 1.0) * PI);
            v = 1.0 - abs(1.0 - 2.0 * b);
        }
        case 13u: {
            let o = 0.2 + spec * 0.6;
            let inner = sqrt((x - o) * (x - o) + y * y);
            if (r < 1.0 && inner > 0.8) { v = 1.0 - smoothc(0.85, 1.0, r); } else { v = 0.0; }
        }
        case 14u: {
            let half = 0.15 + spec * 0.7;
            let dx = max(abs(x) - half, 0.0);
            let d = sqrt(dx * dx + y * y);
            v = 1.0 - smoothc(0.85, 1.0, d);
        }
        case 15u: { v = 1.0 - r; }
        case 99u: { v = 1.0 - smoothc(0.8, 1.0, r); }
        default: { v = 1.0; }
    }
    return clamp(v, 0.0, 1.0);
}
fn pat_value(which: u32, lx: f32, ly: f32, spec: f32) -> f32 {
    let b = u32(P(12u));
    if (b > 0u) { return builtin_pat(b, lx, ly, spec); }
    if (P(22u) > 0.5) {
        let w = which % max(u32(P(11u)), 1u);
        if (w < 4u) { return spA(lx, ly)[w]; }
        return spB(lx, ly)[w - 4u];
    }
    return builtin_pat(u32(P(13u)), lx, ly, spec);
}
fn blend_apply(m: u32, acc: f32, a: f32, col: f32) -> f32 {
    if (m == 0u) { return max(acc, a * col); }
    if (m == 2u) { return acc + (col - acc) * a; }
    return acc + a * col;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= u.w || gid.y >= u.h) { return; }
    let i = gid.y * u.w + gid.x;
    let uu = (f32(gid.x) + 0.5) / f32(u.w);
    let vv = (f32(gid.y) + 0.5) / f32(u.h);
    let mode = u32(P(0u));
    let blend = u32(P(7u));
    let opacity = P(6u);
    var acc = P(9u);
    if (P(10u) > 0.5) { acc = lum(ic[i]); }
    var o = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    if (mode == 0u) {
        let nx = i32(P(1u));
        let ny = i32(P(2u));
        let fnx = f32(nx);
        let fny = f32(ny);
        let rad = i32(P(5u));
        let dedupe = P(15u) > 0.5;
        let ci = i32(floor(uu * fnx));
        let cj = i32(floor(vv * fny));
        var ck: array<u32, 49>;
        var ca: array<f32, 49>;
        var cc: array<f32, 49>;
        var seen: array<u32, 49>;
        var ns = 0;
        var nc = 0;
        for (var dj = -rad; dj <= rad; dj = dj + 1) {
            for (var di = -rad; di <= rad; di = di + 1) {
                let wi = remi(ci + di, nx);
                let wj = remi(cj + dj, ny);
                let base = u32(wj * nx + wi) * 3u;
                let r0 = idd[base];
                let r1 = idd[base + 1u];
                let r2 = idd[base + 2u];
                let fl = u32(r2.z);
                if ((fl & 4u) == 0u) { continue; }
                let key = u32(r2.y);
                if (dedupe) {
                    var dup = false;
                    for (var q = 0; q < ns; q = q + 1) { if (seen[q] == key) { dup = true; } }
                    if (dup) { continue; }
                    seen[ns] = key;
                    ns = ns + 1;
                }
                let dx = rem1(uu - r0.x + 0.5) - 0.5;
                let dy = rem1(vv - r0.y + 0.5) - 0.5;
                var lx = (r1.y * dx + r1.x * dy) * P(3u) / r0.z + 0.5;
                var ly = (-r1.x * dx + r1.y * dy) * P(4u) / r0.w + 0.5;
                if (lx < 0.0 || lx > 1.0 || ly < 0.0 || ly > 1.0) { continue; }
                if ((fl & 1u) != 0u) { lx = 1.0 - lx; }
                if ((fl & 2u) != 0u) { ly = 1.0 - ly; }
                var spec = P(17u);
                if (P(18u) > 0.5) { spec = r1.w; }
                let a = pat_value(u32(r2.x), lx, ly, spec);
                if (nc < 49) {
                    ck[nc] = key;
                    ca[nc] = a * opacity;
                    cc[nc] = r1.z;
                    nc = nc + 1;
                }
            }
        }
        if (blend >= 2u) {
            for (var a = 1; a < nc; a = a + 1) {
                let k0 = ck[a];
                let a0 = ca[a];
                let c0 = cc[a];
                var b = a - 1;
                loop {
                    if (b < 0 || ck[b] <= k0) { break; }
                    ck[b + 1] = ck[b];
                    ca[b + 1] = ca[b];
                    cc[b + 1] = cc[b];
                    b = b - 1;
                }
                ck[b + 1] = k0;
                ca[b + 1] = a0;
                cc[b + 1] = c0;
            }
        }
        if (P(8u) > 0.5) {
            for (var e = nc - 1; e >= 0; e = e - 1) { acc = blend_apply(blend, acc, ca[e], cc[e]); }
        } else {
            for (var e = 0; e < nc; e = e + 1) { acc = blend_apply(blend, acc, ca[e], cc[e]); }
        }
        if (acc != acc || abs(acc) > 3.0e38) { acc = 0.0; }
        o = vec4<f32>(acc, acc, acc, 1.0);
    } else {
        let n = u32(P(14u));
        let outk = u32(P(16u));
        var hit = -1;
        for (var k = 0u; k < n; k = k + 1u) {
            let r0 = idd[3u * k];
            let r1 = idd[3u * k + 1u];
            let r2 = idd[3u * k + 2u];
            var dx = uu - r0.x;
            var dy = vv - r0.y;
            if (mode == 1u) {
                dx = rem1(dx + 0.5) - 0.5;
                dy = rem1(dy + 0.5) - 0.5;
            }
            var lx = (r1.y * dx + r1.x * dy) / r0.z + 0.5;
            var ly = (-r1.x * dx + r1.y * dy) / r0.w + 0.5;
            if (lx < 0.0 || lx > 1.0 || ly < 0.0 || ly > 1.0) { continue; }
            let fl = u32(r2.z);
            if ((fl & 1u) != 0u) { lx = 1.0 - lx; }
            if ((fl & 2u) != 0u) { ly = 1.0 - ly; }
            let a = pat_value(u32(r2.x), lx, ly, P(17u)) * opacity;
            if (outk == 0u) { acc = blend_apply(blend, acc, a, r1.z); }
            else if (a > 1.0e-3) { hit = i32(k); }
        }
        if (outk == 0u) {
            var v = acc;
            if (v != v || abs(v) > 3.0e38) { v = 0.0; }
            let gain = P(19u);
            if (abs(gain - 1.0) > 1.0e-6) { v = pow(clamp(v, 0.0, 1.0), 1.0 / max(gain, 1.0e-3)); }
            if (P(20u) != 0.0 || P(21u) != 1.0) { v = P(20u) + (P(21u) - P(20u)) * v; }
            o = vec4<f32>(v, v, v, 1.0);
        } else if (hit >= 0) {
            let r0 = idd[3u * u32(hit)];
            let r1 = idd[3u * u32(hit) + 1u];
            let r2 = idd[3u * u32(hit) + 2u];
            if (outk == 1u) { o = vec4<f32>(r2.w, r2.w, r2.w, 1.0); }
            else if (outk == 2u) { o = vec4<f32>(rem1(r0.x), rem1(r0.y), 0.0, 1.0); }
            else if (outk == 3u) { o = vec4<f32>(min(r0.z, 1.0), min(r0.w, 1.0), 0.0, 1.0); }
            else { o = vec4<f32>(r1.z, r1.z, r1.z, 1.0); }
        }
    }
    dst[i] = o;
}
