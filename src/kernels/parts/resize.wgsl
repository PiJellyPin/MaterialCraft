// resample the picture ia (P0 x P1 texels) to the output size: exact box averaging for whole-number reductions,
// bilinear (wrapped) otherwise - the host's ops::resize_to
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= u.w || gid.y >= u.h) { return; }
    let sw = IW();
    let sh = IH();
    var o = vec4<f32>(0.0);
    if (u.op == 1u) {
        // noise_upscale, first step: average of a 4 x 4 grid of samples around every cell centre
        let cx = f32(u.w);
        let cy = f32(u.h);
        let uu = (f32(gid.x) + 0.5) / cx;
        let vv = (f32(gid.y) + 0.5) / cy;
        var a = vec4<f32>(0.0);
        for (var j = 0u; j < 4u; j = j + 1u) {
            for (var k = 0u; k < 4u; k = k + 1u) {
                a = a + spA(uu + ((f32(k) + 0.5) / 4.0 - 0.5) / cx, vv + ((f32(j) + 0.5) / 4.0 - 0.5) / cy) / 16.0;
            }
        }
        dst[gid.y * u.w + gid.x] = a;
        return;
    }
    if (u.op >= 2u) {
        // noise_upscale, second step: P2 filtering (0 bicubic, 1 bilinear, 2 nearest) of the small picture ia
        let uu = (f32(gid.x) + 0.5) / f32(u.w);
        let vv = (f32(gid.y) + 0.5) / f32(u.h);
        if (u.op == 2u) { o = cuA(uu, vv); }
        else if (u.op == 3u) { o = spA(uu, vv); }
        else { o = txA(i32(floor(uu * f32(sw))), i32(floor(vv * f32(sh)))); }
        dst[gid.y * u.w + gid.x] = o;
        return;
    }
    if (sw >= u.w && sh >= u.h && sw % u.w == 0u && sh % u.h == 0u) {
        let fx = sw / u.w;
        let fy = sh / u.h;
        let inv = 1.0 / f32(fx * fy);
        var a = vec4<f32>(0.0);
        for (var y = gid.y * fy; y < gid.y * fy + fy; y = y + 1u) {
            for (var x = gid.x * fx; x < gid.x * fx + fx; x = x + 1u) {
                a = a + ia[y * sw + x];
            }
        }
        o = a * inv;
    } else {
        o = spA((f32(gid.x) + 0.5) / f32(u.w), (f32(gid.y) + 0.5) / f32(u.h));
    }
    dst[gid.y * u.w + gid.x] = o;
}
