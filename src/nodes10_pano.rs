// SPDX-License-Identifier: MIT OR Apache-2.0
//! Library graphs implemented natively (see `nodes10`): panorama (equirectangular environment) tools, 3D textures
//! stored as slice atlases, and the 3D cube pattern.
//!
//! Panoramas: u runs around the horizon (0.5 looks down -Z), v from the zenith (0) to the nadir (1); Y is up.
//! 3D textures: `grid_size` (x, y) slices of the volume laid out row by row, the first slice at the top left; each
//! slice is one tile of the picture.
use crate::nodes10::{add3, blackbody, cross3, dot3, g, grey, lerp, lerp4, mul3, norm3, smooth, sub3, to_lin, to_srgb, Cx};
use crate::tex::{lum, Tex};
use std::f32::consts::{PI, TAU};

/// Direction of panorama position (u, v).
pub fn dir_of(u: f32, v: f32) -> [f32; 3] {
    let (lon, lat) = ((u - 0.5) * TAU, (0.5 - v) * PI);
    [lat.cos() * lon.sin(), lat.sin(), -lat.cos() * lon.cos()]
}

/// Panorama position of direction `d`.
pub fn uv_of(d: [f32; 3]) -> (f32, f32) {
    let d = norm3(d);
    let lon = d[0].atan2(-d[2]);
    let lat = d[1].clamp(-1.0, 1.0).asin();
    ((lon / TAU + 0.5).rem_euclid(1.0), 0.5 - lat / PI)
}

fn rot_y(d: [f32; 3], a: f32) -> [f32; 3] {
    let (s, c) = a.sin_cos();
    [d[0] * c + d[2] * s, d[1], -d[0] * s + d[2] * c]
}

fn rot_x(d: [f32; 3], a: f32) -> [f32; 3] {
    let (s, c) = a.sin_cos();
    [d[0], d[1] * c - d[2] * s, d[1] * s + d[2] * c]
}

/// Rotate `d` about the unit axis `k` by `a` radians (Rodrigues).
fn rot_axis(d: [f32; 3], k: [f32; 3], a: f32) -> [f32; 3] {
    let (s, c) = a.sin_cos();
    let kxd = cross3(k, d);
    let kd = dot3(k, d);
    [d[0] * c + kxd[0] * s + k[0] * kd * (1.0 - c), d[1] * c + kxd[1] * s + k[1] * kd * (1.0 - c), d[2] * c + kxd[2] * s + k[2] * kd * (1.0 - c)]
}

/// A slice-atlas volume.
struct Vol<'a> {
    t: &'a Tex,
    gx: usize,
    gy: usize,
}

impl<'a> Vol<'a> {
    fn new(t: &'a Tex, grid: [f32; 2]) -> Vol<'a> {
        Vol { t, gx: (grid[0].round() as usize).clamp(1, 64), gy: (grid[1].round() as usize).clamp(1, 64) }
    }
    fn depth(&self) -> usize {
        self.gx * self.gy
    }
    /// Value at volume position (x, y, z), each 0..1, wrapping.
    fn sample(&self, x: f32, y: f32, z: f32) -> [f32; 4] {
        let n = self.depth();
        let zf = crate::noise::wrap01(z) * n as f32 - 0.5;
        let z0 = zf.floor();
        let k = zf - z0;
        let at = |zi: i64| {
            let zi = zi.rem_euclid(n as i64) as usize;
            let (tx, ty) = (zi % self.gx, zi / self.gx);
            // keep the read inside the tile (no bleeding into the next slice)
            let tw = 1.0 / self.gx as f32;
            let th = 1.0 / self.gy as f32;
            let half_px = (0.5 / self.t.w as f32, 0.5 / self.t.h as f32);
            let lu = (crate::noise::wrap01(x) * tw).clamp(half_px.0, tw - half_px.0);
            let lv = (crate::noise::wrap01(y) * th).clamp(half_px.1, th - half_px.1);
            self.t.sample(tx as f32 * tw + lu, ty as f32 * th + lv)
        };
        lerp4(at(z0 as i64), at(z0 as i64 + 1), k)
    }
    /// Volume position of picture position (u, v).
    fn pos_of(&self, u: f32, v: f32) -> [f32; 3] {
        let (fx, fy) = (u * self.gx as f32, v * self.gy as f32);
        let (tx, ty) = (fx.floor().min(self.gx as f32 - 1.0), fy.floor().min(self.gy as f32 - 1.0));
        let z = ty * self.gx as f32 + tx;
        [fx - tx, fy - ty, (z + 0.5) / self.depth() as f32]
    }
}

/// Colour of a shape light: a colour or a black-body temperature, times 2^exposure.
fn light_color(cx: &Cx, mode_id: &str, color_id: &str, temp_id: &str, exp_id: &str) -> [f32; 3] {
    let e = if cx.lib.input(exp_id).is_some() { cx.f(exp_id) } else { 1.0 };
    let c = if cx.i(mode_id) == 1 { blackbody(cx.f(temp_id)) } else { let c = cx.col(color_id); [c[0], c[1], c[2]] };
    mul3(c, e)
}

pub fn compute(cx: &Cx) -> Option<Tex> {
    let (s, hs) = (cx.s, cx.hs);
    let rs = cx.rs();
    Some(match cx.kind {
        "x_panorama_gradient_2_points" => {
            let (p1, p2) = (dir_of(cx.lane("point_1_position", 0), cx.lane("point_1_position", 1)), dir_of(cx.lane("point_2_position", 0), cx.lane("point_2_position", 1)));
            let (c1, c2) = (cx.col("point_1_color"), cx.col("point_2_color"));
            let (k1, k2) = (cx.f("point_1_contrast"), cx.f("point_2_contrast"));
            Tex::from_fn(s, hs, |u, v| {
                let d = dir_of(u, v);
                // closeness to each point on the sphere (0 opposite .. 1 at the point), sharpened by its contrast
                let a = ((dot3(d, p1) + 1.0) * 0.5).powf(1.0 + k1 * 8.0);
                let b = ((dot3(d, p2) + 1.0) * 0.5).powf(1.0 + k2 * 8.0);
                let t = b / (a + b).max(1e-6);
                lerp4(c1, c2, t)
            })
        }
        "x_panorama_gradient_linear" => {
            let p = cx.v2("point_position");
            let top = dir_of(p[0], p[1]);
            let (ct, cb) = (cx.col("top_color"), cx.col("bottom_color"));
            let con = cx.f("contrast").clamp(0.0, 1.0);
            Tex::from_fn(s, hs, |u, v| {
                let t = (dot3(dir_of(u, v), top) + 1.0) * 0.5;
                let t = smooth(0.5 * con, 1.0 - 0.5 * con, t);
                lerp4(cb, ct, t)
            })
        }
        "x_panorama_physical_sun_sky" => {
            let sp = cx.v2("sun_pos");
            let sun = dir_of(sp[0], sp[1]);
            let turb = cx.f("turbidity").clamp(1.0, 10.0);
            let albedo = cx.f("albedo");
            let ground = cx.col("ground_color");
            let exposure = 2f32.powf(cx.f("exposure"));
            let (size, si, ki) = (cx.f("sun_size").max(0.01), cx.f("sun_intensity"), cx.f("sky_intensity"));
            let elev = sun[1].max(-0.1);
            // a sky model in the spirit of Preetham's: blue zenith fading to a hazy horizon (whiter with turbidity),
            // warmer and dimmer when the sun is low, a glow around the sun and the sun's disc
            let zen = [0.25 + 0.02 * turb, 0.45 + 0.015 * turb, 0.95];
            let hor = [0.9, 0.92, 1.0];
            let warm = (1.0 - elev.max(0.0)).powi(3);
            let sun_col = [1.0, 1.0 - 0.45 * warm, 1.0 - 0.8 * warm];
            let day = smooth(-0.1, 0.15, elev);
            let disc = (0.53f32 * size).to_radians();
            Tex::from_fn(s, hs, |u, v| {
                let d = dir_of(u, v);
                let mu = dot3(d, sun);
                let ang = mu.clamp(-1.0, 1.0).acos();
                let mut c = if d[1] >= 0.0 {
                    let t = d[1].powf(0.4);
                    let base = [lerp(hor[0], zen[0], t), lerp(hor[1], zen[1], t), lerp(hor[2], zen[2], t)];
                    let glow = (1.0 + mu).powi(2) * 0.25 * (0.5 + turb * 0.1) * (1.0 - t * 0.5);
                    [base[0] * (0.6 + glow * sun_col[0]) * day, base[1] * (0.6 + glow * sun_col[1]) * day, base[2] * (0.6 + glow * sun_col[2]) * day]
                } else {
                    let bounce = (0.3 + albedo) * day * 0.8;
                    [ground[0] * bounce, ground[1] * bounce, ground[2] * bounce]
                };
                for k in 0..3 {
                    c[k] *= ki;
                }
                if ang < disc && d[1] > -0.02 {
                    let limb = 1.0 - 0.6 * (ang / disc).powi(2);
                    for k in 0..3 {
                        c[k] += sun_col[k] * 40.0 * si * limb;
                    }
                }
                [c[0] * exposure, c[1] * exposure, c[2] * exposure, 1.0]
            })
        }
        "x_panorama_3d_position" => {
            // up vector 0: Z up (as the panorama lights' default positions show), 1: Y up
            let z_up = cx.i("up_vector") == 0;
            Tex::from_fn(s, hs, |u, v| {
                let d = dir_of(u, v);
                let d = if z_up { [d[0], -d[2], d[1]] } else { d };
                [d[0] * 0.5 + 0.5, d[1] * 0.5 + 0.5, d[2] * 0.5 + 0.5, 1.0]
            })
        }
        "x_panorama_rotation" => {
            let t = cx.img_or_ref("input", grey(0.0));
            let (yaw, dir, pre, post) = (cx.f("rotation") * TAU, cx.f("direction_angle") * TAU, cx.f("pre_rotation_around_pole") * TAU, cx.f("post_rotation_around_pole") * TAU);
            Tex::from_fn(s, hs, |u, v| {
                // undo, in reverse order: post spin, tilt of the pole toward the direction, pre spin, yaw
                let mut d = dir_of(u, v);
                d = rot_y(d, -post);
                d = rot_x(d, -dir);
                d = rot_y(d, -pre);
                d = rot_y(d, -yaw);
                let (su, sv) = uv_of(d);
                t.sample(su, sv)
            })
        }
        "x_panorama_straighten_horizon" => {
            let t = cx.img_or_ref("input", grey(0.0));
            let enable = cx.b("enable_horizon_correction");
            let (a, b) = (cx.v2("horizon_p1"), cx.v2("horizon_p2"));
            let yaw = cx.f("horizontal_rotation") * TAU;
            // the plane through the two horizon points and the eye; its normal should point up
            let n = norm3(cross3(dir_of(a[0], a[1]), dir_of(b[0], b[1])));
            let n = if n[1] < 0.0 { [-n[0], -n[1], -n[2]] } else { n };
            let axis = cross3(n, [0.0, 1.0, 0.0]);
            let al = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
            let ang = n[1].clamp(-1.0, 1.0).acos();
            Tex::from_fn(s, hs, |u, v| {
                let mut d = rot_y(dir_of(u, v), -yaw);
                if enable && al > 1e-6 {
                    d = rot_axis(d, mul3(axis, 1.0 / al), -ang);
                }
                let (su, sv) = uv_of(d);
                t.sample(su, sv)
            })
        }
        "x_nadir_extract" => {
            let t = cx.img_or_ref("input", grey(0.0));
            let scale = cx.f("scale").max(0.01);
            let rot = cx.f("rotation") * TAU;
            let gamma = cx.i("output_gamma");
            Tex::from_fn(s, hs, |u, v| {
                // a straight-down view of the floor: the picture's plane at unit distance below the eye
                let (x, y) = ((u - 0.5) * 2.0 / scale * 2.0, (v - 0.5) * 2.0 / scale * 2.0);
                let (sn, cs) = rot.sin_cos();
                let (x, y) = (x * cs - y * sn, x * sn + y * cs);
                let (su, sv) = uv_of([x, -1.0, y]);
                let p = t.sample(su, sv);
                if gamma == 1 {
                    [to_srgb(p[0]), to_srgb(p[1]), to_srgb(p[2]), p[3]]
                } else {
                    p
                }
            })
        }
        "x_panorama_nadir_patch" => {
            let t = cx.img_or_ref("input", grey(0.0));
            if !cx.b("enable") {
                return Some(t.into_owned());
            }
            let mask = cx.img("mask_input");
            let scale = cx.f("patch_scale").max(0.01);
            let size = cx.v2("patch_size");
            let rot = cx.f("patch_rotation") * TAU;
            let hard = cx.f("patch_hardness").clamp(0.0, 1.0);
            let src = cx.v2("source_position");
            let tgt = cx.v2("patch_position");
            let src_rot = cx.f("source_rotation_offset") * TAU;
            let frames = cx.b("show_frames_helper");
            let ft = cx.f("frame_thickness") * 0.02;
            let (sd, td) = (dir_of(src[0], src[1]), dir_of(tgt[0], tgt[1]));
            // tangent frames at the source and the target
            let frame = |d: [f32; 3]| {
                let up = if d[1].abs() > 0.95 { [0.0, 0.0, 1.0] } else { [0.0, 1.0, 0.0] };
                let a = norm3(cross3(up, d));
                (a, cross3(d, a))
            };
            let ((sa, sb), (ta, tb)) = (frame(sd), frame(td));
            Tex::from_fn(s, hs, |u, v| {
                let base = t.sample(u, v);
                let d = dir_of(u, v);
                let c = dot3(d, td);
                if c <= 0.0 {
                    return base;
                }
                let (x, y) = (dot3(d, ta) / c, dot3(d, tb) / c);
                let (sn, cs) = rot.sin_cos();
                let (x, y) = ((x * cs - y * sn) / (scale * size[0].max(0.01)), (x * sn + y * cs) / (scale * size[1].max(0.01)));
                let r = x.abs().max(y.abs());
                let k = 1.0 - smooth(hard, 1.0 + 1e-3, r);
                let edge = if frames && (r - 1.0).abs() < ft * 4.0 { 1.0 } else { 0.0 };
                if k <= 0.0 && edge <= 0.0 {
                    return base;
                }
                let (sn2, cs2) = src_rot.sin_cos();
                let (xs, ys) = ((x * cs2 - y * sn2) * scale * size[0].max(0.01), (x * sn2 + y * cs2) * scale * size[1].max(0.01));
                let sdir = add3(sd, add3(mul3(sa, xs), mul3(sb, ys)));
                let (su, sv) = uv_of(sdir);
                let m = mask.map(|mm| g(mm, u, v)).unwrap_or(1.0);
                let p = lerp4(base, t.sample(su, sv), k * m);
                lerp4(p, [0.0, 1.0, 0.2, 1.0], edge)
            })
        }
        "x_panorama_shape" | "x_panorama_shape_2" => {
            let two = cx.kind.ends_with("_2");
            let m = cx.v4("matrix22");
            let off = cx.v2("offset");
            let shape = cx.i("shape");
            let (col, hard, hot_k, hot_size, hot_fall, hot_pos) = if two {
                (light_color(cx, "shape_color_mode", "shape_color", "shape_temperature", "shape_exposure"), cx.f("shape_hardness"), cx.f("hotspot_exposure"), cx.f("hotspot_size"), cx.f("hotspot_falloff"), cx.v2("hotspot_position"))
            } else {
                let c = cx.col("shape_color");
                ([c[0] * cx.f("shape_intensity"), c[1] * cx.f("shape_intensity"), c[2] * cx.f("shape_intensity")], 1.0 - cx.f("shape_soft_borders"), cx.f("hotspot_intensity") / 10.0, cx.f("hotspot_size"), cx.f("hotspot_falloff"), cx.v2("hotspot_position"))
            };
            let bg_on = if two { cx.b("enable_background_input") } else { cx.b("enable_background") };
            let bgc = cx.col("background_color");
            let bgi = if two { cx.img("background_image_input") } else { None };
            let tex_in = if two { cx.img("shape_image_input") } else if cx.b("enable_texture_input") { cx.img("input") } else { None };
            Tex::from_fn(s, hs, |u, v| {
                // the shape's own coordinates: the matrix scales the panorama around its centre, the offset moves the
                // shape in its own units (y up)
                let (x, y) = (u - 0.5, 0.5 - v);
                let (lx, ly) = (m[0] * x + m[1] * y - off[0], m[2] * x + m[3] * y - off[1]);
                // the shape fills local -0.5..0.5
                let d = match shape {
                    1 => (lx * lx + ly * ly).sqrt() * 2.0,
                    _ => lx.abs().max(ly.abs()) * 2.0,
                };
                let k = 1.0 - smooth(hard.clamp(0.0, 0.999), 1.0, d);
                let back = if bg_on { bgi.map(|b| b.sample(u, v)).unwrap_or(bgc) } else { [0.0, 0.0, 0.0, 1.0] };
                if k <= 0.0 {
                    return back;
                }
                let (hx, hy) = (lx + 0.5 - hot_pos[0], 0.5 - ly - hot_pos[1]);
                let hd = (hx * hx + hy * hy).sqrt() / hot_size.max(0.01);
                let hot = 1.0 + hot_k * (1.0 - smooth(0.0, 1.0 - hot_fall * 0.9, hd)) * 4.0;
                let tc = tex_in.map(|t| t.sample(lx + 0.5, 0.5 - ly)).unwrap_or([1.0; 4]);
                let c = [col[0] * hot * tc[0], col[1] * hot * tc[1], col[2] * hot * tc[2], 1.0];
                lerp4(back, c, k)
            })
        }
        "x_panorama_line_light" | "x_panorama_plane_light" | "x_panorama_sphere_light" => shape_light(cx),
        // ---------------------------------------------------------------- 3D textures
        "x_3d_texture_position" => {
            let one = Tex::solid(1, 1, [0.0; 4]);
            let vol = Vol::new(&one, cx.v2("grid_size"));
            let (gx, gy) = (vol.gx, vol.gy);
            Tex::from_fn(s, hs, |u, v| {
                let (fx, fy) = (u * gx as f32, v * gy as f32);
                let (tx, ty) = (fx.floor(), fy.floor());
                let z = (ty * gx as f32 + tx + 0.5) / (gx * gy) as f32;
                [fx - tx, fy - ty, z, 1.0]
            })
        }
        "x_3d_texture_offset_color" | "x_3d_texture_offset_grayscale" => {
            let t = cx.img_or_ref("input", grey(0.0));
            let vol = Vol::new(&t, cx.v2("grid_size"));
            let off = cx.v3("offset");
            Tex::from_fn(s, hs, |u, v| {
                let p = vol.pos_of(u, v);
                vol.sample(p[0] - off[0], p[1] - off[1], p[2] - off[2])
            })
        }
        "x_3d_texture_sdf" => {
            let t = cx.img_or_ref("mask_input", grey(0.0));
            let grid = cx.v2("grid_size");
            let (gx, gy) = ((grid[0].round() as usize).clamp(1, 64), (grid[1].round() as usize).clamp(1, 64));
            let thr = cx.f("threshold");
            let (tw, th) = ((s / gx).max(1), (hs / gy).max(1));
            let nz = gx * gy;
            // resample the mask into a tw x th x nz voxel grid
            let vol = Vol::new(&t, grid);
            let n = tw * th * nz;
            let inside: Vec<bool> = (0..n)
                .map(|i| {
                    let (x, y, z) = (i % tw, (i / tw) % th, i / (tw * th));
                    lum(vol.sample((x as f32 + 0.5) / tw as f32, (y as f32 + 0.5) / th as f32, (z as f32 + 0.5) / nz as f32)) > thr
                })
                .collect();
            let d_out = edt3(&inside, tw, th, nz, true);
            let d_in = edt3(&inside, tw, th, nz, false);
            let signed = cx.i("output") == 1;
            let range = tw.max(th).max(nz) as f32 * 0.5;
            Tex::from_fn(s, hs, |u, v| {
                let p = vol.pos_of(u, v);
                let (x, y, z) = (((p[0] * tw as f32) as usize).min(tw - 1), ((p[1] * th as f32) as usize).min(th - 1), ((p[2] * nz as f32) as usize).min(nz - 1));
                let i = (z * th + y) * tw + x;
                let d = if inside[i] { -(d_in[i].sqrt() - 0.5) } else { d_out[i].sqrt() - 0.5 };
                grey(if signed { (0.5 + d / range * 0.5).clamp(0.0, 1.0) } else { (d.max(0.0) / range).clamp(0.0, 1.0) })
            })
        }
        "x_3d_texture_surface_render" | "x_3d_texture_volume_render_light" => volume_render(cx),
        "x_cube_3d" | "x_cube_3d_gbuffers" => {
            let o = cx.v2("orientation_offset");
            let size = cx.v3("size");
            let sc = cx.f("scale").max(0.01);
            let half = [size[0] * sc, size[1] * sc, size[2] * sc];
            let (yaw, pitch) = (o[0] * PI, o[1] * PI);
            let aspect = s as f32 / hs as f32;
            let res: Vec<[[f32; 4]; 5]> = crate::tex::par_map(s * hs, |i| {
                let (u, v) = (((i % s) as f32 + 0.5) / s as f32, ((i / s) as f32 + 0.5) / hs as f32);
                // an orthographic camera looking at the cube from the yaw / pitch direction
                let ro0 = [(u - 0.5) * aspect, 0.5 - v, 4.0];
                let rd0 = [0.0, 0.0, -1.0];
                let ro = rot_y(rot_x(ro0, -pitch), -yaw);
                let rd = rot_y(rot_x(rd0, -pitch), -yaw);
                let mut tmin = f32::MIN;
                let mut tmax = f32::MAX;
                let mut axis = 0usize;
                for k in 0..3 {
                    if rd[k].abs() < 1e-8 {
                        if ro[k].abs() > half[k] {
                            return [grey(0.0), [0.0; 4], [0.5, 0.5, 1.0, 0.0], grey(0.0), [0.0; 4]];
                        }
                        continue;
                    }
                    let (a, b) = ((-half[k] - ro[k]) / rd[k], (half[k] - ro[k]) / rd[k]);
                    let (a, b) = (a.min(b), a.max(b));
                    if a > tmin {
                        tmin = a;
                        axis = k;
                    }
                    tmax = tmax.min(b);
                }
                if tmin > tmax || tmax < 0.0 {
                    return [grey(0.0), [0.0; 4], [0.5, 0.5, 1.0, 0.0], grey(0.0), [0.0; 4]];
                }
                let p = add3(ro, mul3(rd, tmin));
                let mut n = [0.0f32; 3];
                n[axis] = if rd[axis] > 0.0 { -1.0 } else { 1.0 };
                let shade = match axis {
                    1 => 1.0,
                    0 => 0.7,
                    _ => 0.45,
                };
                let lp = [(p[0] / half[0] + 1.0) * 0.5, (p[1] / half[1] + 1.0) * 0.5, (p[2] / half[2] + 1.0) * 0.5];
                let uv = match axis {
                    0 => [lp[2], 1.0 - lp[1]],
                    1 => [lp[0], lp[2]],
                    _ => [lp[0], 1.0 - lp[1]],
                };
                let depth = (1.0 - (tmin - 2.0) / 4.0).clamp(0.0, 1.0);
                [grey(shade), [lp[0], lp[1], lp[2], 1.0], [n[0] * 0.5 + 0.5, n[1] * 0.5 + 0.5, n[2] * 0.5 + 0.5, 1.0], grey(depth), [uv[0], uv[1], 0.0, 1.0]]
            });
            if cx.kind == "x_cube_3d" {
                Tex { w: s, h: hs, px: res.iter().map(|r| r[0]).collect() }
            } else {
                let outs: Vec<Option<Tex>> = (1..5).map(|k| if cx.wants(k - 1) { Some(Tex { w: s, h: hs, px: res.iter().map(|r| r[k]).collect() }) } else { None }).collect();
                cx.pick(outs)
            }
        }
        _ => {
            let _ = rs;
            return None;
        }
    })
}

/// One-dimensional squared distance transform (Felzenszwalb & Huttenlocher) of `f`.
fn edt1(f: &[f64]) -> Vec<f64> {
    let n = f.len();
    let mut d = vec![0.0f64; n];
    if n == 0 {
        return d;
    }
    let mut v = vec![0usize; n];
    let mut z = vec![0.0f64; n + 1];
    let mut k = 0usize;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        let mut sv;
        loop {
            let p = v[k];
            sv = ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * (q as f64 - p as f64));
            if sv <= z[k] && k > 0 {
                k -= 1;
            } else {
                break;
            }
        }
        k += 1;
        v[k] = q;
        z[k] = sv;
        z[k + 1] = f64::INFINITY;
    }
    k = 0;
    for (q, dq) in d.iter_mut().enumerate() {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let p = v[k];
        *dq = (q as f64 - p as f64).powi(2) + f[p];
    }
    d
}

/// Squared Euclidean distance (in voxels) from every voxel to the nearest voxel of the other kind (`to_inside`:
/// distance of outside voxels to the inside, else the reverse); separable exact transform, wrapping in every axis.
fn edt3(inside: &[bool], w: usize, h: usize, d: usize, to_inside: bool) -> Vec<f32> {
    const INF: f64 = 1e10;
    let mut f: Vec<f64> = inside.iter().map(|&b| if b == to_inside { 0.0 } else { INF }).collect();
    let mut pass = |n: usize, stride: usize, count: usize, offset: &dyn Fn(usize) -> usize| {
        for c in 0..count {
            let base = offset(c);
            // the line three times over so the transform wraps
            let line: Vec<f64> = (0..n * 3).map(|k| f[base + (k % n) * stride]).collect();
            let out = edt1(&line);
            for i in 0..n {
                f[base + i * stride] = out[n + i];
            }
        }
    };
    pass(w, 1, h * d, &|c| c * w);
    pass(h, w, w * d, &|c| (c / w) * w * h + c % w);
    pass(d, w * h, w * h, &|c| c);
    f.into_iter().map(|x| x.min(INF) as f32).collect()
}

/// Ray-marched picture of a 3D-texture distance field (surface) or density (volume with light).
fn volume_render(cx: &Cx) -> Tex {
    let (s, hs) = (cx.s, cx.hs);
    let surface = cx.kind == "x_3d_texture_surface_render";
    let field = if surface { cx.img("3d_distance_field") } else { cx.img("3d_signed_distance_field") };
    let density = if surface { None } else { cx.img("density") };
    let grid = [16.0, 16.0];
    let cam = cx.v2("camera_uv_position");
    let dist = cx.f("camera_distance").max(0.5);
    let fov = cx.f("camera_fov").clamp(1.0, 170.0).to_radians();
    let albedo = cx.v3("albedo");
    let bg_mode = cx.i("background_mode");
    let bgc = cx.col("background_color");
    let env = if surface { cx.img("environment") } else { None };
    let ground = cx.b("enable_ground_plane");
    let infinite = cx.b("infinite_plane");
    let gsize = cx.v2("ground_plane_size");
    // camera on a sphere around the unit volume (centred at 0.5)
    let (az, el) = ((cam[0] - 0.5) * TAU, (0.5 - cam[1]) * PI * 0.98);
    let eye = add3([0.5, 0.5, 0.5], mul3([el.cos() * az.sin(), el.sin(), el.cos() * az.cos()], dist * 0.5));
    let fwd = norm3(sub3([0.5, 0.5, 0.5], eye));
    let right = norm3(cross3(fwd, [0.0, 1.0, 0.0]));
    let up = cross3(right, fwd);
    let tanf = (fov * 0.5).tan();
    let aspect = s as f32 / hs as f32;
    let light = if surface {
        norm3([0.5, 0.8, 0.3])
    } else {
        let lp = cx.v2("light_uv_pos");
        let (la, le) = ((lp[0] - 0.5) * TAU, (0.5 - lp[1]) * PI);
        [le.cos() * la.sin(), le.sin(), le.cos() * la.cos()]
    };
    let lcol = if surface { [1.0, 0.98, 0.95] } else { let c = if cx.i("light_color_mode") == 1 { blackbody(cx.f("light_temperature")) } else { cx.v3("light_color") }; mul3(c, cx.f("light_intensity")) };
    let amb = if surface { [0.25, 0.28, 0.32] } else { mul3(cx.v3("ambient_color"), cx.f("ambient_intensity") * 0.3) };
    let absorption = if surface { 0.0 } else { cx.f("absorption") * 40.0 };
    let empty = Tex::solid(1, 1, grey(1.0));
    let ft = field.unwrap_or(&empty);
    let vol = Vol::new(ft, grid);
    let dvol = density.map(|d| Vol::new(d, grid));
    let sdf = |p: [f32; 3]| -> f32 {
        if field.is_none() {
            // no field: a sphere of radius 0.3
            return ((p[0] - 0.5).powi(2) + (p[1] - 0.5).powi(2) + (p[2] - 0.5).powi(2)).sqrt() - 0.3;
        }
        let inside_box = (0.0..=1.0).contains(&p[0]) && (0.0..=1.0).contains(&p[1]) && (0.0..=1.0).contains(&p[2]);
        if !inside_box {
            let q = [p[0].clamp(0.0, 1.0), p[1].clamp(0.0, 1.0), p[2].clamp(0.0, 1.0)];
            return ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt() + 0.01;
        }
        (lum(vol.sample(p[0], 1.0 - p[1], p[2])) - 0.5) * 2.0
    };
    let res: Vec<[f32; 4]> = crate::tex::par_map(s * hs, |i| {
        let (u, v) = (((i % s) as f32 + 0.5) / s as f32, ((i / s) as f32 + 0.5) / hs as f32);
        let rd = norm3(add3(fwd, add3(mul3(right, (u - 0.5) * 2.0 * tanf * aspect), mul3(up, (0.5 - v) * 2.0 * tanf))));
        let back = match (bg_mode, env) {
            (1, Some(e)) => {
                let (su, sv) = uv_of(rd);
                e.sample(su, sv)
            }
            _ => bgc,
        };
        // ground plane at the volume's bottom
        let tg = if ground && rd[1] < 0.0 { (0.0 - eye[1]) / rd[1] } else { f32::MAX };
        let gp = add3(eye, mul3(rd, tg));
        let on_ground = tg < f32::MAX && (infinite || ((gp[0] - 0.5).abs() < gsize[0] * 0.5 && (gp[2] - 0.5).abs() < gsize[1] * 0.5));
        if surface {
            let mut t = 0.0f32;
            for _ in 0..160 {
                let p = add3(eye, mul3(rd, t));
                let d = sdf(p);
                if d < 0.002 {
                    let e = 0.01;
                    let n = norm3([sdf(add3(p, [e, 0.0, 0.0])) - sdf(sub3(p, [e, 0.0, 0.0])), sdf(add3(p, [0.0, e, 0.0])) - sdf(sub3(p, [0.0, e, 0.0])), sdf(add3(p, [0.0, 0.0, e])) - sdf(sub3(p, [0.0, 0.0, e]))]);
                    let ndl = dot3(n, light).max(0.0);
                    let c = [albedo[0] * (amb[0] + lcol[0] * ndl), albedo[1] * (amb[1] + lcol[1] * ndl), albedo[2] * (amb[2] + lcol[2] * ndl)];
                    return [to_srgb(c[0]), to_srgb(c[1]), to_srgb(c[2]), 1.0];
                }
                t += d.max(0.002);
                if t > dist * 2.0 || t > tg {
                    break;
                }
            }
            if on_ground {
                let sh = 0.6 + 0.4 * ((gp[0] * 8.0).floor() as i32 + (gp[2] * 8.0).floor() as i32).rem_euclid(2) as f32;
                return [0.5 * sh, 0.5 * sh, 0.5 * sh, 1.0];
            }
            back
        } else {
            // volume: march through the unit box accumulating density lit by the light (one shadow ray per step)
            let mut trans = 1.0f32;
            let mut acc = [0.0f32; 3];
            let steps = 64;
            let dt = 1.8 / steps as f32;
            let t0 = (dist * 0.5 - 0.9).max(0.0);
            for k in 0..steps {
                let p = add3(eye, mul3(rd, t0 + k as f32 * dt));
                if !(0.0..=1.0).contains(&p[0]) || !(0.0..=1.0).contains(&p[1]) || !(0.0..=1.0).contains(&p[2]) {
                    continue;
                }
                let inside = if field.is_some() { (sdf(p) < 0.0) as i32 as f32 } else { 1.0 };
                let dens = dvol.as_ref().map(|dv| lum(dv.sample(p[0], 1.0 - p[1], p[2]))).unwrap_or(0.5) * inside;
                if dens <= 0.0 {
                    continue;
                }
                let mut shadow = 1.0f32;
                for j in 1..8 {
                    let q = add3(p, mul3(light, j as f32 * 0.08));
                    if !(0.0..=1.0).contains(&q[0]) || !(0.0..=1.0).contains(&q[1]) || !(0.0..=1.0).contains(&q[2]) {
                        break;
                    }
                    let dq = dvol.as_ref().map(|dv| lum(dv.sample(q[0], 1.0 - q[1], q[2]))).unwrap_or(0.5);
                    shadow *= (-dq * absorption * 0.08).exp();
                }
                let a = 1.0 - (-dens * absorption * dt).exp();
                for c in 0..3 {
                    acc[c] += trans * a * albedo[c] * (lcol[c] * shadow + amb[c]);
                }
                trans *= 1.0 - a;
                if trans < 0.01 {
                    break;
                }
            }
            let b = if on_ground { [0.35, 0.35, 0.35, 1.0] } else { back };
            [to_srgb(acc[0] + to_lin(b[0]) * trans), to_srgb(acc[1] + to_lin(b[1]) * trans), to_srgb(acc[2] + to_lin(b[2]) * trans), 1.0]
        }
    });
    Tex { w: s, h: hs, px: res }
}

/// Line, plane and sphere lights drawn into a panorama: each pixel's view ray is tested against the light's shape.
fn shape_light(cx: &Cx) -> Tex {
    let (s, hs) = (cx.s, cx.hs);
    let k = cx.kind;
    // world positions are given Z-up unless the up vector says Y (the defaults put every light above the horizon:
    // a sphere at (-1, -1, 1), a shading light straight up at (0, 0, 1))
    let z_up = cx.i("up_vector") == 0;
    let to_world = |p: [f32; 3]| if z_up { [p[0], p[2], -p[1]] } else { p };
    let cam = to_world(cx.v3("camera_position"));
    let world_mode = cx.i("position_mode") == 1;
    let bg_on = cx.b("enable_background_input");
    let bgc = cx.col("background_color");
    let bgi = cx.img("background_image_input");
    let bg_gamma = cx.i("background_image_input_gamma");
    let ground_clip = cx.lib.input("enable_ground_clipping").is_some() && cx.b("enable_ground_clipping");
    let ground_h = if ground_clip { cx.f("ground_height") } else { -1e9 };
    let pattern_in = cx.img("pattern_image_input");
    let pattern_on = cx.lib.input("shape_pattern").is_some() && cx.i("shape_pattern") > 0;
    let hard = if cx.lib.input("shape_pattern_hardness").is_some() { cx.f("shape_pattern_hardness") } else { 0.9 };
    let (color, img) = match k {
        "x_panorama_sphere_light" => (light_color(cx, "sphere_color_mode", "sphere_color", "sphere_temperature", "sphere_exposure"), cx.img("sphere_image_input")),
        _ => (light_color(cx, "shape_color_mode", "shape_color", "shape_temperature", "shape_exposure"), cx.img("shape_image_input")),
    };
    let place = |uv_id: &str, world_id: &str, dist: f32| -> [f32; 3] {
        if world_mode {
            to_world(cx.v3(world_id))
        } else {
            let uv = cx.v2(uv_id);
            add3(cam, mul3(dir_of(uv[0], uv[1]), dist))
        }
    };
    let dist = if cx.lib.input("distance_from_origin").is_some() { cx.f("distance_from_origin").max(0.01) } else { 1.0 };
    Tex::from_fn(s, hs, |u, v| {
        let back = if bg_on {
            match bgi {
                Some(b) => {
                    let p = b.sample(u, v);
                    if bg_gamma == 1 {
                        [to_lin(p[0]), to_lin(p[1]), to_lin(p[2]), p[3]]
                    } else {
                        p
                    }
                }
                None => bgc,
            }
        } else {
            [0.0, 0.0, 0.0, 1.0]
        };
        let rd = dir_of(u, v);
        // (ray distance, local coordinates across the shape 0..1, falloff)
        let hit: Option<(f32, [f32; 2], f32)> = match k {
            "x_panorama_line_light" => {
                let (a, b) = if world_mode { (to_world(cx.v3("point_1_world_position")), to_world(cx.v3("point_2_position"))) } else { (place("point_1_uv_position", "", dist), place("point_2_uv_position", "", dist)) };
                let r = cx.f("line_thickness") * 0.1 * dist;
                // closest approach between the view ray and the segment
                let (d1, d2, r0) = (rd, sub3(b, a), sub3(cam, a));
                let (a11, a12, a22) = (dot3(d1, d1), dot3(d1, d2), dot3(d2, d2));
                let (b1, b2) = (dot3(d1, r0), dot3(d2, r0));
                let det = a11 * a22 - a12 * a12;
                let (t, sl) = if det.abs() < 1e-9 { (0.0, 0.0) } else { (((a12 * b2 - a22 * b1) / det).max(0.0), ((a11 * b2 - a12 * b1) / det).clamp(0.0, 1.0)) };
                let p1 = add3(cam, mul3(d1, t));
                let p2 = add3(a, mul3(d2, sl));
                let dd = sub3(p1, p2);
                let d = dot3(dd, dd).sqrt();
                if d < r && t > 0.0 {
                    Some((t, [sl, d / r.max(1e-6)], 1.0 - smooth(hard, 1.0, d / r.max(1e-6))))
                } else {
                    None
                }
            }
            "x_panorama_plane_light" => {
                let c = place("plane_uv_position", "plane_world_position", dist);
                let size = cx.v2("plane_size");
                let sc = cx.f("plane_scale");
                let rot = cx.v3("plane_rotation");
                // the plane faces the camera, then turns by its rotation
                let n0 = norm3(sub3(cam, c));
                let mut n = rot_axis(n0, [0.0, 1.0, 0.0], rot[1] * TAU);
                n = rot_axis(n, [1.0, 0.0, 0.0], rot[0] * TAU);
                let up = if n[1].abs() > 0.95 { [0.0, 0.0, 1.0] } else { [0.0, 1.0, 0.0] };
                let ax = norm3(cross3(up, n));
                let ay = cross3(n, ax);
                let (ax, ay) = (rot_axis(ax, n, rot[2] * TAU), rot_axis(ay, n, rot[2] * TAU));
                let den = dot3(rd, n);
                if den.abs() < 1e-6 {
                    None
                } else {
                    let t = dot3(sub3(c, cam), n) / den;
                    let p = sub3(add3(cam, mul3(rd, t)), c);
                    let (x, y) = (dot3(p, ax) / (sc * size[0].max(0.01) * dist), dot3(p, ay) / (sc * size[1].max(0.01) * dist));
                    if t > 0.0 && x.abs() <= 0.5 && y.abs() <= 0.5 {
                        Some((t, [x + 0.5, 0.5 - y], 1.0))
                    } else {
                        None
                    }
                }
            }
            _ => {
                let c = place("sphere_uv_position", "sphere_world_position", dist);
                let r = cx.f("sphere_radius").max(0.001);
                let oc = sub3(cam, c);
                let b = dot3(oc, rd);
                let disc = b * b - (dot3(oc, oc) - r * r);
                if disc < 0.0 {
                    None
                } else {
                    let t = -b - disc.sqrt();
                    if t <= 0.0 {
                        None
                    } else {
                        let p = add3(cam, mul3(rd, t));
                        let n = norm3(sub3(p, c));
                        let (su, sv) = uv_of(n);
                        let edge = (disc.sqrt() / r).clamp(0.0, 1.0);
                        let shading = if cx.i("shading") == 1 { (dot3(n, norm3(to_world(cx.v3("shading_light_world_position")))) * 0.5 + 0.5).max(cx.f("penombra_transparency")) } else { 1.0 };
                        Some((t, [su, sv], smooth(0.0, 1.0 - hard.clamp(0.0, 0.99), edge) * shading))
                    }
                }
            }
        };
        let Some((t, l, fall)) = hit else { return back };
        let p = add3(cam, mul3(rd, t));
        if p[1] < ground_h {
            return back;
        }
        let tc = img.map(|i| i.sample(l[0], l[1])).unwrap_or([1.0; 4]);
        let pat = if pattern_on { pattern_in.map(|pi| g(pi, l[0], l[1])).unwrap_or(1.0) } else { 1.0 };
        let kk = fall * pat;
        let c = [color[0] * tc[0], color[1] * tc[1], color[2] * tc[2], 1.0];
        lerp4(back, c, kk.clamp(0.0, 1.0))
    })
}
