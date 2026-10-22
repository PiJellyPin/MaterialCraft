@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) { return; }
    let i = id.y * u.w + id.x;
    if (u.op == 0xFFFFFFFFu) { dst[0] = ia[0] + ib[0] + ic[0] + idd[0]; }
    let p = ia[i];
    var g = lum(p);
    if (P(2u) > 0.5) { g = rem1(g); } else { g = clamp(g, 0.0, 1.0); }
    let n = u.n;
    let use_a = P(3u) > 0.5;
    var o = vec4<f32>(g, g, g, p.w);
    if (n > 0u) {
        let s0 = u.x[0];
        var res = vec4<f32>(s0.y, s0.z, s0.w, select(p.w, u.x[1].x, use_a));
        var found = g <= s0.x;
        for (var k = 1u; k < n; k = k + 1u) {
            if (found) { break; }
            let sb = u.x[2u * k];
            if (g <= sb.x) {
                let sa = u.x[2u * (k - 1u)];
                let ab = u.x[2u * k + 1u].x;
                let aa = u.x[2u * (k - 1u) + 1u].x;
                var f = (g - sa.x) / max(sb.x - sa.x, 1e-9);
                let interp = u32(P(0u));
                if (interp == 1u) {
                    f = f + (f * f * (3.0 - 2.0 * f) - f) * P(1u);
                } else if (interp == 2u) {
                    f = 0.0;
                } else if (interp == 3u) {
                    f = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
                }
                let c = sa.yzw + (sb.yzw - sa.yzw) * f;
                let al = aa + (ab - aa) * f;
                res = vec4<f32>(c, select(p.w, al, use_a));
                found = true;
            }
        }
        if (!found) {
            let e = u.x[2u * (n - 1u)];
            res = vec4<f32>(e.yzw, select(p.w, u.x[2u * (n - 1u) + 1u].x, use_a));
        }
        o = res;
    }
    dst[i] = o;
}
