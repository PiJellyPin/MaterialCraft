// finishes the instance records of a Tile Sampler: applies the map inputs sampled at each tile centre.
// ia = records before the maps (3 vec4 per instance); ib mask, ic scale, id4 rotation, ie displacement, if_ colour, ig distribution.
// one thread per output vec4 (instance = x / 3, part = x % 3)
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.y != 0u || gid.x >= u.w) { return; }
    let k = gid.x / 3u;
    let part = gid.x % 3u;
    let a = ia[3u * k];
    let b = ia[3u * k + 1u];
    let c = ia[3u * k + 2u];
    let fl = u32(b.w);
    var on = (fl & 4u) != 0u;
    var cx = a.x;
    var cy = a.y;
    var s = a.z;
    var turns = a.w;
    var col = b.x;
    var pick = u32(c.x);
    let fnx = P(10u);
    let fny = P(11u);
    if (P(0u) > 0.5) {
        let m = lum(spB(a.x, a.y));
        if (P(7u) > 0.5) { on = on && (m < P(6u)); } else { on = on && (m >= P(6u)); }
    }
    if (P(3u) > 0.5) {
        let d = lum(spE(a.x, a.y));
        let an = P(8u) * TAU;
        let kk = (d - 0.5) * 2.0 * P(9u);
        cx = cx + cos(an) * kk / fnx;
        cy = cy + sin(an) * kk / fny;
    }
    if (P(1u) > 0.5) {
        let m = lum(spC(a.x, a.y));
        s = s * (1.0 + P(12u) * (m - 1.0));
    }
    s = max(s, 0.01);
    if (P(2u) > 0.5) {
        let m = lum(spD(a.x, a.y));
        turns = turns + m * P(13u);
    }
    let sn = sin(turns * TAU);
    let cs = cos(turns * TAU);
    if (P(16u) > 0.0) {
        let rel = clamp(s / max(P(17u), 1e-3), 0.0, 1.0);
        col = col * (1.0 - P(16u) * (1.0 - rel));
    }
    if (P(4u) > 0.5) {
        let m = lum(spF(a.x, a.y));
        col = col * (1.0 + P(14u) * (m - 1.0));
    }
    if (P(5u) > 0.5) {
        let d = lum(spG(a.x, a.y));
        let np = max(u32(P(18u)), 1u);
        pick = min(u32(max(d, 0.0) * f32(np)), np - 1u);
    }
    var o = vec4<f32>(0.0);
    if (part == 0u) {
        o = vec4<f32>(cx, cy, s, s * max(P(15u), 0.01));
    } else if (part == 1u) {
        o = vec4<f32>(sn, cs, col, b.y);
    } else {
        o = vec4<f32>(f32(pick), b.z, f32((fl & 3u) | (select(0u, 1u, on) << 2u)), 0.0);
    }
    dst[gid.x] = o;
}
