const INF: u32 = 0xFFFFFFFFu;
var<workgroup> part: array<u32, 64>;

// the region key of a pixel: 0 = not part of any region, equal non-zero keys that touch belong to one region
fn key_at(i: u32) -> u32 {
    let p = src[i];
    if (u.mode == 0u) {
        if (lum(p) > u.thr) { return 1u; }
        return 0u;
    }
    if (p.x == 0.0 && p.y == 0.0 && p.z == 0.0) { return 0u; }
    return u32(i32(p.x * 100000.0)) + 1u;
}
fn find(a: u32) -> u32 {
    var x = a;
    loop {
        let p = atomicLoad(&par[x]);
        if (p == x) { return x; }
        let gp = atomicLoad(&par[p]);
        if (gp != p) { atomicStore(&par[x], gp); }
        x = gp;
    }
    return x;
}
fn union_px(a0: u32, b0: u32) {
    var a = a0;
    var b = b0;
    loop {
        let ra = find(a);
        let rb = find(b);
        if (ra == rb) { return; }
        let hi = max(ra, rb);
        let lo = min(ra, rb);
        let old = atomicMin(&par[hi], lo);
        if (old == hi) { return; }
        if (old > lo) { a = old; b = lo; } else { a = ra; b = rb; }
    }
}
fn occ_bit(base: u32, slot: u32, ww: u32, pos: u32) -> bool {
    let wv = atomicLoad(&msc[base + slot * ww + (pos >> 5u)]);
    return ((wv >> (pos & 31u)) & 1u) != 0u;
}
// position of pixel (x, y) in the region's unwrapped frame: the tile is cut where the region does not cross the seam
fn lin(x: u32, y: u32, root: u32) -> vec2<u32> {
    var xp = x;
    var yp = y;
    let sx = atomicLoad(&slt[2u * root]);
    if (sx != INF) {
        let st = atomicLoad(&msc[u.off_sx + sx]);
        if (st != INF) { xp = (x + u.w - st) % u.w; }
    }
    let sy = atomicLoad(&slt[2u * root + 1u]);
    if (sy != INF) {
        let st = atomicLoad(&msc[u.off_sy + sy]);
        if (st != INF) { yp = (y + u.h - st) % u.h; }
    }
    return vec2<u32>(xp, yp);
}
fn lvl_get(l: u32, i: u32) -> u32 {
    if (l == 0u) { return atomicLoad(&scn[i]); }
    return atomicLoad(&msc[u.lbase + i]);
}
fn lvl_set(l: u32, i: u32, v: u32) {
    if (l == 0u) { atomicStore(&scn[i], v); } else { atomicStore(&msc[u.lbase + i], v); }
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(local_invocation_index) li: u32,
        @builtin(workgroup_id) wid: vec3<u32>, @builtin(num_workgroups) nwg: vec3<u32>) {
    let op = u.op;
    // one-dimensional launches
    if (op == 1u) {
        // start positions of the seam cuts: none yet
        let i = gid.y * (nwg.x * 8u) + gid.x;
        if (i < u.h) { atomicStore(&msc[u.off_sx + i], INF); }
        if (i < u.w) { atomicStore(&msc[u.off_sy + i], INF); }
        if (u.n == 0xFFFFFFFFu) { dst[0] = src[0] * 0.0; }
        return;
    }
    if (op == 5u) {
        // inclusive prefix sums inside blocks of 1024, block totals go one level up
        let blk = wid.y * nwg.x + wid.x;
        let first = blk * 1024u + li * 16u;
        var s = 0u;
        for (var k = 0u; k < 16u; k = k + 1u) {
            if (first + k < u.cnt) { s = s + lvl_get(u.lvl, first + k); }
        }
        part[li] = s;
        workgroupBarrier();
        var off = 0u;
        for (var k = 0u; k < li; k = k + 1u) { off = off + part[k]; }
        var run = off;
        for (var k = 0u; k < 16u; k = k + 1u) {
            if (first + k < u.cnt) {
                run = run + lvl_get(u.lvl, first + k);
                lvl_set(u.lvl, first + k, run);
            }
        }
        if (li == 63u) { atomicStore(&msc[u.nbase + blk], off + s); }
        return;
    }
    if (op == 6u) {
        // add the totals of the earlier blocks (already scanned one level up)
        let i = gid.y * (nwg.x * 8u) + gid.x;
        if (i < u.cnt && i >= 1024u) {
            let blk = i / 1024u;
            let add = atomicLoad(&msc[u.nbase + blk - 1u]);
            lvl_set(u.lvl, i, lvl_get(u.lvl, i) + add);
        }
        return;
    }
    if (gid.x >= u.w || gid.y >= u.h) { return; }
    let x = gid.x;
    let y = gid.y;
    let i = y * u.w + x;
    let xr = select(x + 1u, 0u, x + 1u == u.w);
    let yd = select(y + 1u, 0u, y + 1u == u.h);
    switch (op) {
        case 0u: {
            atomicStore(&par[i], i);
            atomicStore(&scn[i], 0u);
            atomicStore(&slt[2u * i], INF);
            atomicStore(&slt[2u * i + 1u], INF);
            atomicStore(&bbx[4u * i], INF);
            atomicStore(&bbx[4u * i + 1u], 0u);
            atomicStore(&bbx[4u * i + 2u], INF);
            atomicStore(&bbx[4u * i + 3u], 0u);
        }
        case 2u: {
            let k = key_at(i);
            if (k != 0u) {
                let j = y * u.w + xr;
                if (key_at(j) == k) { union_px(i, j); }
                let j2 = yd * u.w + x;
                if (key_at(j2) == k) { union_px(i, j2); }
            }
        }
        case 3u: {
            if (key_at(i) != 0u) { atomicStore(&par[i], find(i)); }
        }
        case 4u: {
            var v = 0u;
            if (key_at(i) != 0u && atomicLoad(&par[i]) == i) { v = 1u; }
            atomicStore(&scn[i], v);
        }
        case 7u: {
            let k = key_at(i);
            if (k != 0u) {
                let r = atomicLoad(&par[i]);
                if (x + 1u == u.w && key_at(y * u.w) == k) { atomicMin(&slt[2u * r], y); }
                if (y + 1u == u.h && key_at(x) == k) { atomicMin(&slt[2u * r + 1u], x); }
            }
        }
        case 8u: {
            if (key_at(i) != 0u) {
                let r = atomicLoad(&par[i]);
                let sx = atomicLoad(&slt[2u * r]);
                if (sx != INF) { atomicOr(&msc[u.off_ox + sx * u.wwx + (x >> 5u)], 1u << (x & 31u)); }
                let sy = atomicLoad(&slt[2u * r + 1u]);
                if (sy != INF) { atomicOr(&msc[u.off_oy + sy * u.wwy + (y >> 5u)], 1u << (y & 31u)); }
            }
        }
        case 9u: {
            // the first column (row) of a region's run: occupied, the one before it is not
            let px = select(x - 1u, u.w - 1u, x == 0u);
            if (occ_bit(u.off_ox, y, u.wwx, x) && !occ_bit(u.off_ox, y, u.wwx, px)) { atomicStore(&msc[u.off_sx + y], x); }
            let py = select(y - 1u, u.h - 1u, y == 0u);
            if (occ_bit(u.off_oy, x, u.wwy, y) && !occ_bit(u.off_oy, x, u.wwy, py)) { atomicStore(&msc[u.off_sy + x], y); }
        }
        case 10u: {
            if (key_at(i) != 0u) {
                let r = atomicLoad(&par[i]);
                let l = lin(x, y, r);
                atomicMin(&bbx[4u * r], l.x);
                atomicMax(&bbx[4u * r + 1u], l.x);
                atomicMin(&bbx[4u * r + 2u], l.y);
                atomicMax(&bbx[4u * r + 3u], l.y);
            }
        }
        case 11u, 12u, 13u, 14u: {
            var o = vec4<f32>(0.0, 0.0, 0.0, 1.0);
            if (key_at(i) != 0u) {
                let r = atomicLoad(&par[i]);
                let label = atomicLoad(&scn[r]);
                let x0 = atomicLoad(&bbx[4u * r]);
                let x1 = atomicLoad(&bbx[4u * r + 1u]);
                let y0 = atomicLoad(&bbx[4u * r + 2u]);
                let y1 = atomicLoad(&bbx[4u * r + 3u]);
                if (op == 11u) {
                    let l = lin(x, y, r);
                    let gx = (f32(i32(l.x) - i32(x0)) + 0.5) / f32(i32(x1) - i32(x0) + 1);
                    let gy = (f32(i32(l.y) - i32(y0)) + 0.5) / f32(i32(y1) - i32(y0) + 1);
                    o = vec4<f32>(0.1 + 0.9 * rnd(i32(label), 3, 17u), gx, gy, 1.0);
                } else if (op == 12u) {
                    let total = atomicLoad(&scn[u.total]);
                    let v = f32(label) / f32(max(total, 1u));
                    o = vec4<f32>(v, v, v, 1.0);
                } else {
                    let rx = r % u.w;
                    let ry = r / u.w;
                    let lr = lin(rx, ry, r);
                    let ax = i32(x0) - i32(lr.x);
                    let bx = i32(x1) - i32(lr.x);
                    let ay = i32(y0) - i32(lr.y);
                    let by = i32(y1) - i32(lr.y);
                    let fw = f32(u.w);
                    let fh = f32(u.h);
                    if (op == 13u) {
                        let a = f32(rx) + 0.5 + f32(ax + bx) * 0.5;
                        let b = f32(ry) + 0.5 + f32(ay + by) * 0.5;
                        o = vec4<f32>((a - floor(a / fw) * fw) / fw, (b - floor(b / fh) * fh) / fh, 0.0, 1.0);
                    } else {
                        o = vec4<f32>(f32(bx - ax + 1) / fw, f32(by - ay + 1) / fh, 0.0, 1.0);
                    }
                }
            }
            dst[i] = o;
        }
        default: {}
    }
}
