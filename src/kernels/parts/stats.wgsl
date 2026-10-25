// whole-picture statistics: histogram -> equalising curves (ops 0, 1) and bounding box of the bright area (ops 2..4)
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    switch (u.op) {
        case 0u: {
            if (gid.x >= u.w || gid.y >= u.h) { return; }
            let p = src[gid.y * u.w + gid.x];
            let l = lum(p);
            let nb = f32(u.n - 1u);
            atomicAdd(&cnt[u32(clamp(p.x, 0.0, 1.0) * nb + 0.5)], 1u);
            atomicAdd(&cnt[u.n + u32(clamp(p.y, 0.0, 1.0) * nb + 0.5)], 1u);
            atomicAdd(&cnt[2u * u.n + u32(clamp(p.z, 0.0, 1.0) * nb + 0.5)], 1u);
            atomicAdd(&cnt[3u * u.n + u32(clamp(l, 0.0, 1.0) * nb + 0.5)], 1u);
        }
        case 1u: {
            // one thread per channel: cumulative distribution, shifted to start at zero
            if (gid.x != 0u || gid.y != 0u) { return; }
            let total = f32(u.w * u.h);
            for (var c = 0u; c < 4u; c = c + 1u) {
                var acc = 0.0;
                var first = -1.0;
                for (var i = 0u; i < u.n; i = i + 1u) {
                    acc = acc + f32(atomicLoad(&cnt[c * u.n + i]));
                    if (first < 0.0 && acc > 0.0) { first = acc / total; }
                }
                let ident = !(first >= 0.0 && 1.0 - first >= 1e-9);
                acc = 0.0;
                for (var i = 0u; i < u.n; i = i + 1u) {
                    acc = acc + f32(atomicLoad(&cnt[c * u.n + i]));
                    var v = -1.0;
                    if (!ident) { v = clamp((acc / total - first) / (1.0 - first), 0.0, 1.0); }
                    var o = dst[i];
                    o[c] = v;
                    dst[i] = o;
                }
            }
        }
        case 2u: {
            if (gid.x >= u.w || gid.y >= u.h) { return; }
            let p = src[gid.y * u.w + gid.x];
            if (max(p.x, max(p.y, p.z)) > u.thr) {
                atomicMin(&cnt[0], gid.x);
                atomicMax(&cnt[1], gid.x + 1u);
                atomicMin(&cnt[2], gid.y);
                atomicMax(&cnt[3], gid.y + 1u);
            }
        }
        case 3u: {
            if (gid.x != 0u || gid.y != 0u) { return; }
            atomicStore(&cnt[0], 0xFFFFFFFFu);
            atomicStore(&cnt[1], 0u);
            atomicStore(&cnt[2], 0xFFFFFFFFu);
            atomicStore(&cnt[3], 0u);
        }
        default: {
            if (gid.x != 0u || gid.y != 0u) { return; }
            let x0 = atomicLoad(&cnt[0]);
            let x1 = atomicLoad(&cnt[1]);
            let y0 = atomicLoad(&cnt[2]);
            let y1 = atomicLoad(&cnt[3]);
            if (x1 <= x0 || y1 <= y0) {
                dst[0] = vec4<f32>(-1.0, -1.0, -1.0, -1.0);
            } else {
                dst[0] = vec4<f32>(f32(x0), f32(y0), f32(x1), f32(y1));
            }
        }
    }
}
