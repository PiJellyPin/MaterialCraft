fn dec3(p: vec4<f32>) -> vec3<f32> { return (p.xyz - vec3<f32>(0.5)) * 2.0; }

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= u.w || id.y >= u.h) { return; }
    let i = id.y * u.w + id.x;
    if (u.n == 0xFFFFFFFFu) { dst[0] = ia[0] + ib[0] + ic[0] + idd[0]; }
    let p = ia[i];
    var o = p;
    switch (u.op) {
        case 0u: {
            // hsl: hue shift, saturation and value multipliers
            let hsv = rgb_to_hsv(p.xyz);
            let c = hsv_to_rgb(hsv.x + P(0u), clamp(hsv.y * P(1u), 0.0, 1.0), max(hsv.z * P(2u), 0.0));
            o = vec4<f32>(c, p.w);
        }
        case 1u: {
            // adjust: brightness, contrast
            o = vec4<f32>((p.xyz - vec3<f32>(0.5)) * P(1u) + vec3<f32>(0.5 + P(0u)), p.w);
        }
        case 2u: {
            // contrast / luminosity
            o = vec4<f32>((p.xyz - vec3<f32>(0.5)) * P(0u) + vec3<f32>(0.5 + P(1u)), p.w);
        }
        case 3u: { o = vec4<f32>(s2l(p.x), s2l(p.y), s2l(p.z), p.w); }
        case 4u: { o = vec4<f32>(l2s(p.x), l2s(p.y), l2s(p.z), p.w); }
        case 5u: {
            // premultiply (0) / unpremultiply (1)
            if (P(0u) > 0.5) {
                if (p.w > 1e-6) { o = vec4<f32>(p.xyz / p.w, p.w); } else { o = vec4<f32>(0.0, 0.0, 0.0, p.w); }
            } else {
                o = vec4<f32>(p.xyz * p.w, p.w);
            }
        }
        case 6u: {
            let g = p.x * P(0u) + p.y * P(1u) + p.z * P(2u);
            o = vec4<f32>(g, g, g, p.w);
        }
        case 7u: {
            // channel mixer: rows r, g, b of the matrix then the mono flag
            var q = vec3<f32>(
                P(0u) * p.x + P(1u) * p.y + P(2u) * p.z,
                P(3u) * p.x + P(4u) * p.y + P(5u) * p.z,
                P(6u) * p.x + P(7u) * p.y + P(8u) * p.z);
            q = clamp(q, vec3<f32>(-8.0), vec3<f32>(8.0));
            o = vec4<f32>(q, p.w);
            if (P(9u) > 0.5) {
                let g = lum(o);
                o = vec4<f32>(g, g, g, p.w);
            }
        }
        case 8u: {
            // colour to mask: colour, range, softness
            let dv = p.xyz - vec3<f32>(P(0u), P(1u), P(2u));
            let d = sqrt(dot(dv, dv) / 3.0);
            o = grey(1.0 - smoothc(P(3u), P(3u) + max(P(4u), 1e-4), d));
        }
        case 9u: {
            // replace colour: source, target, range, softness
            let s = vec3<f32>(P(0u), P(1u), P(2u));
            let t = vec3<f32>(P(3u), P(4u), P(5u));
            let dv = p.xyz - s;
            let d = sqrt(dot(dv, dv) / 3.0);
            let m = 1.0 - smoothc(P(6u), P(6u) + max(P(7u), 1e-4), d);
            o = vec4<f32>(p.xyz + (t - s) * m, p.w);
        }
        case 10u: {
            let l = max(lum(p), 1e-3);
            o = vec4<f32>(clamp(p.xyz / l * 0.5, vec3<f32>(0.0), vec3<f32>(1.0)), p.w);
        }
        case 11u: {
            // normal invert x, y, z, a
            o = vec4<f32>(
                select(p.x, 1.0 - p.x, P(0u) > 0.5),
                select(p.y, 1.0 - p.y, P(1u) > 0.5),
                select(p.z, 1.0 - p.z, P(2u) > 0.5),
                select(p.w, 1.0 - p.w, P(3u) > 0.5));
        }
        case 12u: {
            let v = dec3(p);
            let l = max(length(v), 1e-6);
            o = vec4<f32>(v / l * 0.5 + vec3<f32>(0.5), p.w);
        }
        case 13u: {
            // histogram range: position, range, contrast, invert
            let range = max(P(1u), 1e-4);
            let soft = (1.0 - clamp(P(2u), 0.0, 1.0)) * range * 0.5 + 1e-4;
            let lo = P(0u) - range * 0.5;
            let hi = P(0u) + range * 0.5;
            let l = lum(p);
            var g = smoothc(lo - soft, lo + soft, l) * (1.0 - smoothc(hi - soft, hi + soft, l));
            if (P(3u) > 0.5) { g = 1.0 - g; }
            o = grey(g);
        }
        case 14u: {
            // alpha merge: rgb from a, alpha from the luminance of b (when connected)
            if (P(0u) > 0.5) { o = vec4<f32>(p.xyz, lum(ib[i])); }
        }
        case 15u: {
            // RGBA merge: channel 0 of a, channel 1 of b, channel 2 of c and the luminance of d (its grey value, not
            // its own alpha), when connected
            o = vec4<f32>(
                select(0.0, ia[i].x, P(0u) > 0.5),
                select(0.0, ib[i].y, P(1u) > 0.5),
                select(0.0, ic[i].z, P(2u) > 0.5),
                select(1.0, lum(idd[i]), P(3u) > 0.5));
        }
        case 16u: {
            // channel shuffle: sources p0..3 (0-3 channels of a, 4-7 of b, 8 zero, 9 one), inversions p4..7
            let q = ib[i];
            var r = vec4<f32>(0.0);
            for (var k = 0u; k < 4u; k = k + 1u) {
                let s = i32(P(k));
                var v = 1.0;
                if (s <= 3) { v = p[s]; } else if (s <= 7) { v = q[s - 4]; } else if (s == 8) { v = 0.0; }
                r[k] = select(v, 1.0 - v, P(4u + k) > 0.5);
            }
            o = r;
        }
        case 17u: {
            // normal vector rotation: angle (rad), green sign, has rotation map (b)
            var a = P(0u);
            if (P(2u) > 0.5) { a = a * lum(ib[i]); }
            let sn = sin(a);
            let cs = cos(a);
            let gs = P(1u);
            let x = (p.x - 0.5) * 2.0;
            let y = gs * (p.y - 0.5) * 2.0;
            let rx = cs * x - sn * y;
            let ry = sn * x + cs * y;
            o = vec4<f32>(rx * 0.5 + 0.5, gs * ry * 0.5 + 0.5, p.z, p.w);
        }
        case 18u: {
            // normal combine: a base, b detail, technique
            let pb = p;
            let pd = ib[i];
            let tech = u32(P(0u));
            if (tech < 2u) {
                let bx = (pb.x - 0.5) * 2.0;
                let by = (pb.y - 0.5) * 2.0;
                let bz = max((pb.z - 0.5) * 2.0, 0.05);
                let dx = (pd.x - 0.5) * 2.0;
                let dy = (pd.y - 0.5) * 2.0;
                let dz = max((pd.z - 0.5) * 2.0, 0.05);
                var r: vec3<f32>;
                if (tech == 0u) { r = vec3<f32>(bx + dx, by + dy, bz * dz); } else { r = vec3<f32>(bx / bz + dx / dz, by / bz + dy / dz, 1.0); }
                let l = max(length(r), 1e-6);
                o = vec4<f32>(r / l * 0.5 + vec3<f32>(0.5), 1.0);
            } else {
                let tt = vec3<f32>((pb.x - 0.5) * 2.0, (pb.y - 0.5) * 2.0, (pb.z - 0.5) * 2.0 + 1.0);
                let uu = vec3<f32>(-(pd.x - 0.5) * 2.0, -(pd.y - 0.5) * 2.0, (pd.z - 0.5) * 2.0);
                let d = dot(tt, uu);
                var tz = tt.z;
                if (abs(tz) < 1e-4) { tz = 1e-4; }
                let r = tt * d / tz - uu;
                let l = max(length(r), 1e-6);
                o = vec4<f32>(r / l * 0.5 + vec3<f32>(0.5), 1.0);
            }
        }
        case 19u: {
            // normal blend: a background, b foreground, c mask; opacity, has mask
            let pb = ib[i];
            let x = (p.x - 0.5) * 2.0 + (pb.x - 0.5) * 2.0;
            let y = (p.y - 0.5) * 2.0 + (pb.y - 0.5) * 2.0;
            let z = (p.z - 0.5) * 2.0 * (pb.z - 0.5) * 2.0;
            let fv = vec3<f32>(x, y, z);
            let l = max(length(fv), 1e-6);
            let full = vec4<f32>(fv / l * 0.5 + vec3<f32>(0.5), 1.0);
            var mk = 1.0;
            if (P(1u) > 0.5) { mk = lum(ic[i]); }
            let k = clamp(P(0u) * mk, 0.0, 1.0);
            if (k >= 1.0) {
                o = full;
            } else {
                let v = (p.xyz - vec3<f32>(0.5)) * 2.0 * (1.0 - k) + (full.xyz - vec3<f32>(0.5)) * 2.0 * k;
                let ll = max(length(v), 1e-6);
                o = vec4<f32>(v / ll * 0.5 + vec3<f32>(0.5), 1.0);
            }
        }
        case 20u: { o = vec4<f32>(p.xyz, 1.0); }
        case 21u: {
            // auto levels: statistics in idd (min rgb + min lum, max rgb + max lum); per channel flag
            var lo = idd[0].xyz;
            var hi = idd[1].xyz;
            if (P(0u) <= 0.5) {
                let l = min(lo.x, min(lo.y, lo.z));
                let h = max(hi.x, max(hi.y, hi.z));
                lo = vec3<f32>(l);
                hi = vec3<f32>(h);
            }
            var r = p.xyz;
            for (var c = 0; c < 3; c = c + 1) {
                if (hi[c] - lo[c] > 1e-6) { r[c] = (p[c] - lo[c]) / (hi[c] - lo[c]); }
            }
            o = vec4<f32>(r, p.w);
        }
        case 22u: {
            // levels with the range found on the device: gamma, out low, out high
            let mn = idd[0].w;
            let mx = max(idd[1].w, mn + 1e-4);
            var d: f32;
            if (mx >= mn) { d = max(mx - mn, 1e-6); } else { d = min(mx - mn, -1e-6); }
            let g = 1.0 / max(P(0u), 1e-3);
            let unit = abs(g - 1.0) < 1e-6;
            var r = vec3<f32>(0.0);
            for (var c = 0; c < 3; c = c + 1) {
                let v = clamp((p[c] - mn) / d, 0.0, 1.0);
                var w = v;
                if (!unit) { w = pow(v, g); }
                r[c] = P(1u) + (P(2u) - P(1u)) * w;
            }
            o = vec4<f32>(r, p.w);
        }
        case 23u: {
            // height combine: a, b, optional mask c; mode, amount, has mask
            let x = lum(p);
            let y = lum(ib[i]);
            var k = 1.0;
            if (P(2u) > 0.5) { k = clamp(lum(ic[i]), 0.0, 1.0); }
            let md = i32(P(0u));
            var r = max(x, y);
            if (md == 1) { r = min(x, y); } else if (md == 2) { r = x + (y - 0.5) * P(1u); } else if (md == 3) { r = abs(x - y); }
            o = grey(lerpf(x, r, k));
        }
        case 24u: {
            // image against its blur (a = image, b = blur)
            let q = ib[i];
            let md = i32(P(0u));
            if (md == 0) {
                o = grey(clamp(0.5 + (p.x - q.x) * P(1u), 0.0, 1.0));
            } else if (md == 1) {
                o = grey(clamp(1.0 - (q.x - p.x) * P(1u), 0.0, 1.0));
            } else if (md == 2) {
                o = vec4<f32>(clamp(p.xyz - q.xyz + vec3<f32>(0.5), vec3<f32>(0.0), vec3<f32>(1.0)), p.w);
            } else if (md == 3) {
                o = vec4<f32>(p.xyz + (p.xyz - q.xyz) * P(1u), p.w);
            } else if (md == 4) {
                let d = lum(p) - lum(q);
                var v = d;
                if (P(2u) > 0.5) { v = -d; }
                o = grey(clamp(v * P(1u), 0.0, 1.0));
            } else {
                let clear = P(1u);
                let cut = max((q.xyz - vec3<f32>(clear)) / (1.0 - clear), vec3<f32>(0.0));
                o = vec4<f32>(clamp(p.xyz + cut * vec3<f32>(P(2u), P(3u), P(4u)) * P(5u), vec3<f32>(0.0), vec3<f32>(1.0)), p.w);
            }
        }
        case 25u: {
            // output format: P0 = 1 (8-bit integer), 2 (16-bit integer), 3 (half float)
            let md = u32(P(0u));
            if (md == 3u) {
                let a = unpack2x16float(pack2x16float(vec2<f32>(p.x, p.y)));
                let b = unpack2x16float(pack2x16float(vec2<f32>(p.z, p.w)));
                o = vec4<f32>(a.x, a.y, b.x, b.y);
            } else {
                var k = 255.0;
                if (md == 2u) { k = 65535.0; }
                o = floor(clamp(p, vec4<f32>(0.0), vec4<f32>(1.0)) * k + vec4<f32>(0.5)) / k;
            }
        }
        default: {}
    }
    dst[i] = o;
}
