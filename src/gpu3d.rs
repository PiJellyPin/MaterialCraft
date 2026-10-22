// SPDX-License-Identifier: MIT OR Apache-2.0
//! GPU PBR viewport (OpenGL 3.3 core through glow / egui_glow paint callbacks).
//!
//! The scene (the shape, optionally with floor, reference balls or a pedestal) is one tessellated triangle mesh. The
//! height map displaces the vertices in the vertex shader (true displacement); the fragment shader does GGX PBR with
//! normal mapping, outward parallax relief, a light rig with shadow maps and image based lighting from an HDR
//! environment (prefiltered levels plus spherical harmonics). The frame is rendered 4x multisampled into a floating
//! point target and then goes through the post chain (ambient occlusion, depth of field, bloom, grading, tone
//! mapping), a line by line port of `post.rs`, so the CPU renderer (`raster.rs`) shows the same picture.
use crate::hdri::Env;
use crate::objects;
use crate::render3d::{View, RELIEF_BUMP, RELIEF_DISPLACE, RELIEF_PARALLAX};
use crate::scene::{self, Background, Channel, LightKind};
use crate::v3::*;
use crate::tex::{lum, par_map, Tex};
use eframe::egui::PaintCallbackInfo;
use eframe::egui_glow::{self, glow};
use glow::HasContext;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Shared handle between the UI and the paint callback.
pub type Shared = Arc<Mutex<Gpu>>;

pub fn new_shared() -> Shared {
    Arc::new(Mutex::new(Gpu::default()))
}

/// Extra viewer options that live next to (not inside) the CPU renderer's `View`.
#[derive(Clone, Debug, PartialEq)]
pub struct GpuOpts {
    /// Contact shadows computed by marching the height field towards the light.
    pub self_shadow: bool,
    /// Anisotropic filtering level (1..16). 0 = use the maximum the GPU offers.
    pub aniso: f32,
    /// Texture filtering: 0 nearest (pixelated), 1 bilinear, 2 trilinear (smooth between mip levels).
    pub filter: u8,
    /// Mip maps (smaller copies of the texture for surfaces far away or at a slant).
    pub mipmaps: bool,
    /// Outside 0..1: 0 repeat, 1 mirrored repeat, 2 clamp to the edge.
    pub wrap: u8,
    /// Added to the mip level the GPU picks: positive blurs, negative sharpens (and may shimmer).
    pub lod_bias: f32,
}

impl Default for GpuOpts {
    fn default() -> Self {
        GpuOpts { self_shadow: true, aniso: 0.0, filter: 2, mipmaps: true, wrap: 0, lod_bias: 0.0 }
    }
}

/// Texture data prepared on the CPU (resampled to one size, packed, converted) and ready to upload.
pub struct GpuMaps {
    pub size: usize,
    /// sRGB RGBA8
    pub base: Vec<u8>,
    /// linear RGBA8 tangent-space normal
    pub normal: Vec<u8>,
    /// R = roughness, G = metallic, B = ambient occlusion
    pub rma: Vec<u8>,
    /// sRGB RGBA8, absent when the graph has no emissive output
    pub emissive: Option<Vec<u8>>,
    /// luminance height (w, h, values)
    pub height: Option<(usize, usize, Arc<Vec<f32>>)>,
    /// the same height as 16-bit floats, the form the GPU stores it in (converted here, off the paint thread)
    pub height_half: Option<Vec<u16>>,
    /// fingerprints of base, normal, rma, emissive and height: a map that did not change is not uploaded again
    pub hashes: [u64; 5],
}

pub struct MapInputs<'a> {
    pub base: Option<&'a Tex>,
    pub normal: Option<&'a Tex>,
    pub rough: Option<&'a Tex>,
    pub metal: Option<&'a Tex>,
    pub ao: Option<&'a Tex>,
    pub emissive: Option<&'a Tex>,
    pub height: Option<&'a Tex>,
}

fn b8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Convert graph outputs into upload-ready buffers. All colour maps are brought to the largest map size.
/// Bring a map to `size` x `size`: exact box averaging when shrinking by a whole factor (no aliasing, e.g. 8K -> 4K),
/// bilinear otherwise.
fn fit(t: &Tex, size: usize) -> Tex {
    if t.w == size && t.h == size {
        return t.clone();
    }
    if t.w > size && t.h > size && t.w % size == 0 && t.h % size == 0 {
        let (fx, fy) = (t.w / size, t.h / size);
        let inv = 1.0 / (fx * fy) as f32;
        let px = par_map(size * size, |i| {
            let (x0, y0) = ((i % size) * fx, (i / size) * fy);
            let mut a = [0.0f32; 4];
            for y in y0..y0 + fy {
                for x in x0..x0 + fx {
                    let p = t.px[y * t.w + x];
                    for c in 0..4 {
                        a[c] += p[c];
                    }
                }
            }
            [a[0] * inv, a[1] * inv, a[2] * inv, a[3] * inv]
        });
        return Tex { w: size, h: size, px };
    }
    Tex::from_fn(size, size, |u, v| t.sample(u, v))
}

/// Convert graph outputs into upload-ready buffers. All colour maps are brought to the largest map size.
pub fn prepare(m: &MapInputs) -> GpuMaps {
    let all = [m.base, m.normal, m.rough, m.metal, m.ao, m.emissive];
    let size = all.iter().flatten().map(|t| t.w.max(t.h)).max().unwrap_or(4).clamp(1, 4096);
    let fitted: Vec<Option<Tex>> = all.iter().map(|t| t.filter(|t| t.w != size || t.h != size).map(|t| fit(t, size))).collect();
    let px = |k: usize, t: Option<&Tex>, dflt: [f32; 4], i: usize| -> [f32; 4] {
        match (&fitted[k], t) {
            (Some(f), _) => f.px[i],
            (None, Some(t)) => t.px[i],
            (None, None) => dflt,
        }
    };
    let n = size * size;
    let rgba = |k: usize, t: Option<&Tex>, dflt: [f32; 4]| -> Vec<u8> {
        let v: Vec<[u8; 4]> = par_map(n, |i| {
            let p = px(k, t, dflt, i);
            [b8(p[0]), b8(p[1]), b8(p[2]), 255]
        });
        v.into_iter().flatten().collect()
    };
    let base = rgba(0, m.base, [0.5, 0.5, 0.5, 1.0]);
    let normal = rgba(1, m.normal, [0.5, 0.5, 1.0, 1.0]);
    let rma: Vec<u8> = {
        let v: Vec<[u8; 4]> = par_map(n, |i| {
            let r = lum(px(2, m.rough, [0.5, 0.5, 0.5, 1.0], i));
            let me = lum(px(3, m.metal, [0.0, 0.0, 0.0, 1.0], i));
            let a = lum(px(4, m.ao, [1.0, 1.0, 1.0, 1.0], i));
            [b8(r), b8(me), b8(a), 255]
        });
        v.into_iter().flatten().collect()
    };
    let emissive = m.emissive.map(|e| rgba(5, Some(e), [0.0, 0.0, 0.0, 1.0]));
    let height = m.height.map(|h| (h.w, h.h, h.px.iter().map(|p| lum(*p)).collect::<Vec<f32>>()));
    let height = height.map(|(w, h, v)| (w, h, Arc::new(v)));
    let height_half = height.as_ref().map(|(_, _, v)| to_half(v));
    let hashes = [
        fingerprint(&base, size as u64),
        fingerprint(&normal, size as u64),
        fingerprint(&rma, size as u64),
        emissive.as_ref().map(|e| fingerprint(e, size as u64)).unwrap_or(0),
        height_half.as_ref().map(|h| fingerprint(bytes_of(h), height.as_ref().map(|x| (x.0 * 65536 + x.1) as u64).unwrap_or(0))).unwrap_or(0),
    ];
    GpuMaps { size, base, normal, rma, emissive, height, height_half, hashes }
}

/// 32-bit floats to 16-bit floats (round to nearest even; values past the 16-bit range become infinity).
fn f32_to_f16(x: f32) -> u16 {
    let b = x.to_bits();
    let sign = ((b >> 16) & 0x8000) as u16;
    let exp = ((b >> 23) & 0xff) as i32;
    let man = b & 0x007f_ffff;
    if exp == 0xff {
        // infinity or NaN
        return sign | 0x7c00 | if man != 0 { 0x0200 } else { 0 };
    }
    let e = exp - 127 + 15;
    if e >= 0x1f {
        return sign | 0x7c00;
    }
    if e <= 0 {
        // below the normal range: a subnormal half, or zero
        if e < -10 {
            return sign;
        }
        let m = man | 0x0080_0000;
        let shift = (14 - e) as u32;
        let half = m >> shift;
        let rem = m & ((1 << shift) - 1);
        let mid = 1 << (shift - 1);
        let up = rem > mid || (rem == mid && (half & 1) == 1);
        return sign | (half + up as u32) as u16;
    }
    let half = ((e as u32) << 10) | (man >> 13);
    let rem = man & 0x1fff;
    let up = rem > 0x1000 || (rem == 0x1000 && (half & 1) == 1);
    // a carry out of the mantissa correctly bumps the exponent (and may reach infinity)
    sign | (half + up as u32) as u16
}

fn to_half(v: &[f32]) -> Vec<u16> {
    const CHUNK: usize = 1 << 16;
    let parts: Vec<Vec<u16>> = par_map(v.len().div_ceil(CHUNK), |c| v[c * CHUNK..((c + 1) * CHUNK).min(v.len())].iter().map(|x| f32_to_f16(*x)).collect());
    parts.concat()
}

/// A fast 64-bit fingerprint of some bytes (and a salt), computed in parallel pieces. It only tells "the same map as
/// last time" from "a different one", so it is not a cryptographic hash.
fn fingerprint(bytes: &[u8], salt: u64) -> u64 {
    const CHUNK: usize = 1 << 20;
    let parts: Vec<u64> = par_map(bytes.len().div_ceil(CHUNK).max(1), |c| {
        let piece = &bytes[(c * CHUNK).min(bytes.len())..((c + 1) * CHUNK).min(bytes.len())];
        let mut h: u64 = 0x9e37_79b9_7f4a_7c15 ^ (c as u64).wrapping_mul(0xff51_afd7_ed55_8ccd);
        let mut words = piece.chunks_exact(8);
        for w in &mut words {
            let x = u64::from_le_bytes([w[0], w[1], w[2], w[3], w[4], w[5], w[6], w[7]]);
            h = (h ^ x).wrapping_mul(0x2545_f491_4f6c_dd1d);
            h ^= h >> 29;
        }
        for &b in words.remainder() {
            h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
        }
        h
    });
    let mut h = salt ^ (bytes.len() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    for p in parts {
        h = (h ^ p).wrapping_mul(0x2545_f491_4f6c_dd1d);
        h ^= h >> 32;
    }
    h | 1 // never 0, which stands for "no map"
}

// ------------------------------------------------------------------------------------------------ GL state
struct Tx {
    id: glow::Texture,
    w: usize,
    h: usize,
    /// GL internal format, and the fingerprint of the picture it holds (0 = unknown)
    internal: u32,
    hash: u64,
}

struct Prog {
    id: glow::Program,
    locs: HashMap<String, glow::UniformLocation>,
}

impl Prog {
    fn loc(&self, n: &str) -> Option<&glow::UniformLocation> {
        self.locs.get(n)
    }
}

/// The names of the uniforms a GLSL source declares (`uniform float uName[8];` gives `uName`).
fn uniform_names(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in src.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("uniform ") {
            let mut it = rest.split_whitespace();
            let _ty = it.next();
            if let Some(n) = it.next() {
                let n = n.trim_end_matches(';');
                let n = n.split('[').next().unwrap_or(n);
                out.push(n.to_string());
            }
        }
    }
    out
}

const SCENE_VS: &str = include_str!("shaders/scene.vert");
const SCENE_FS: &str = include_str!("shaders/scene.frag");
const DEPTH_FS: &str = include_str!("shaders/depth.frag");
const WIRE_FS: &str = include_str!("shaders/wire.frag");
const BG_VS: &str = include_str!("shaders/bg.vert");
const BG_FS: &str = include_str!("shaders/bg.frag");
const POST_FS: &str = include_str!("shaders/post.frag");

/// Fullscreen triangle vertex shader for the post passes.
const FS_VS: &str = r#"#version 330 core
void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
"#;

/// Render targets of one viewport size.
struct Targets {
    w: usize,
    h: usize,
    /// multisampled scene target (absent when the driver cannot do it; the scene then renders into `res_fbo`)
    ms_fbo: Option<glow::Framebuffer>,
    ms_color: Option<glow::Renderbuffer>,
    ms_depth: Option<glow::Renderbuffer>,
    res_fbo: glow::Framebuffer,
    hdr: glow::Texture,
    depth: glow::Texture,
    ping: [(glow::Framebuffer, glow::Texture); 2],
    bright: (glow::Framebuffer, glow::Texture),
}

struct EnvTex {
    key: String,
    tex: glow::Texture,
    levels: usize,
    sh: Vec<f32>,
}

struct ShadowMaps {
    fbo: glow::Framebuffer,
    tex: glow::Texture,
    size: i32,
}

struct MeshGl {
    key: MeshKey,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    ebo: glow::Buffer,
    index_count: i32,
    objects: Vec<scene::Instance>,
    /// radius of the part of the scene that casts shadows
    radius: f32,
    /// lowest vertices of the displaced object (height, uv, vertical displacement, fade) and their undisplaced lowest
    /// point: a new height map only moves the floor, which these give without building the scene again
    bottom: Vec<(f32, [f32; 2], f32, f32)>,
    ymin0: f32,
}

#[derive(Clone, PartialEq)]
struct MeshKey {
    shape: u32,
    tess: u32,
    tiling: u32,
    scene: scene::SceneKind,
    custom: usize,
    /// where the floor sits under the displaced object (quantised), when displacement moves it
    floor: i64,
    disp: (u32, u32),
}

struct Res {
    prog: Prog,
    depth_prog: Prog,
    wire_prog: Prog,
    bg_prog: Prog,
    post_prog: Prog,
    mesh: Option<MeshGl>,
    empty_vao: glow::VertexArray,
    base: Tx,
    normal: Tx,
    rma: Tx,
    emis: Option<Tx>,
    height: Option<Tx>,
    dummy_h: Tx,
    dummy_shadow: glow::Texture,
    max_aniso: f32,
    max_samples: i32,
    targets: Option<Targets>,
    env: Option<EnvTex>,
    shadows: Option<ShadowMaps>,
}

#[derive(Default)]
pub struct Gpu {
    pub opts: GpuOpts,
    /// Set when shader compilation or a GL call failed; the UI then falls back to the CPU renderer.
    pub error: Option<String>,
    /// Maps waiting to be uploaded on the next paint.
    pending: Option<GpuMaps>,
    /// Height luminance of the uploaded maps (for placing the floor under a displaced object).
    height_cpu: Option<(usize, usize, Arc<Vec<f32>>)>,
    height_gen: u64,
    /// Request a pixel read-back of the next frame (screenshots, tests).
    pub want_shot: bool,
    /// Result of a read-back: (width, height, RGBA8 rows from the TOP).
    pub shot: Option<(usize, usize, Vec<u8>)>,
    /// GPU name reported by the driver, for the UI.
    pub renderer: String,
    /// Why the chosen environment file could not be used (the studio is shown instead).
    pub env_error: Option<String>,
    res: Option<Res>,
    /// The sampling options the uploaded textures carry (they are set again when the options change).
    applied: Option<GpuOpts>,
    pub frames: u64,
}

impl Gpu {
    pub fn set_maps(&mut self, maps: GpuMaps) {
        self.height_cpu = maps.height.clone();
        self.height_gen += 1;
        self.pending = Some(maps);
    }
    pub fn ready(&self) -> bool {
        self.error.is_none()
    }
}

// ------------------------------------------------------------------------------------------------ matrices
type M4 = [f32; 16]; // column-major

fn m_mul(a: &M4, b: &M4) -> M4 {
    let mut o = [0.0f32; 16];
    for c in 0..4 {
        for r in 0..4 {
            let mut s = 0.0;
            for k in 0..4 {
                s += a[k * 4 + r] * b[c * 4 + k];
            }
            o[c * 4 + r] = s;
        }
    }
    o
}

fn look_at(eye: V, target: V, up_hint: V) -> M4 {
    let fwd = norm(sub(target, eye));
    let up_hint = if cross(fwd, up_hint).iter().map(|x| x * x).sum::<f32>() < 1e-6 { [0.0, 0.0, 1.0] } else { up_hint };
    let right = norm(cross(fwd, up_hint));
    let up = cross(right, fwd);
    [
        right[0], up[0], -fwd[0], 0.0, //
        right[1], up[1], -fwd[1], 0.0, //
        right[2], up[2], -fwd[2], 0.0, //
        -dot(right, eye), -dot(up, eye), dot(fwd, eye), 1.0,
    ]
}

fn perspective(tan_half: f32, aspect: f32, n: f32, f: f32) -> M4 {
    [
        1.0 / (aspect * tan_half), 0.0, 0.0, 0.0, //
        0.0, 1.0 / tan_half, 0.0, 0.0, //
        0.0, 0.0, -(f + n) / (f - n), -1.0, //
        0.0, 0.0, -2.0 * f * n / (f - n), 0.0,
    ]
}

fn ortho(half: f32, n: f32, f: f32) -> M4 {
    [
        1.0 / half, 0.0, 0.0, 0.0, //
        0.0, 1.0 / half, 0.0, 0.0, //
        0.0, 0.0, -2.0 / (f - n), 0.0, //
        0.0, 0.0, -(f + n) / (f - n), 1.0,
    ]
}

const NEAR: f32 = 0.03;
const FAR: f32 = 80.0;

// ------------------------------------------------------------------------------------------------ GL helpers
const ANISO_PARAM: u32 = 0x84FE;
const ANISO_MAX: u32 = 0x84FF;

unsafe fn compile(gl: &glow::Context, vs: &str, fs: &str) -> Result<Prog, String> {
    let prog = gl.create_program()?;
    let mut shaders = Vec::new();
    for (kind, src, what) in [(glow::VERTEX_SHADER, vs, "vertex"), (glow::FRAGMENT_SHADER, fs, "fragment")] {
        let sh = gl.create_shader(kind)?;
        gl.shader_source(sh, src);
        gl.compile_shader(sh);
        if !gl.get_shader_compile_status(sh) {
            let log = gl.get_shader_info_log(sh);
            return Err(format!("{what} shader compile failed: {log}"));
        }
        gl.attach_shader(prog, sh);
        shaders.push(sh);
    }
    gl.link_program(prog);
    if !gl.get_program_link_status(prog) {
        return Err(format!("shader link failed: {}", gl.get_program_info_log(prog)));
    }
    for sh in shaders {
        gl.detach_shader(prog, sh);
        gl.delete_shader(sh);
    }
    let mut locs = HashMap::new();
    for src in [vs, fs] {
        for n in uniform_names(src) {
            if let Some(l) = gl.get_uniform_location(prog, &n) {
                locs.insert(n, l);
            }
        }
    }
    Ok(Prog { id: prog, locs })
}

unsafe fn make_tex(gl: &glow::Context, internal: u32, fmt: u32, ty: u32, w: usize, h: usize, data: &[u8], aniso: f32) -> Result<Tx, String> {
    let id = gl.create_texture()?;
    gl.bind_texture(glow::TEXTURE_2D, Some(id));
    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
    gl.tex_image_2d(glow::TEXTURE_2D, 0, internal as i32, w as i32, h as i32, 0, fmt, ty, glow::PixelUnpackData::Slice(Some(data)));
    gl.generate_mipmap(glow::TEXTURE_2D);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::REPEAT as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
    if aniso > 1.0 {
        gl.tex_parameter_f32(glow::TEXTURE_2D, ANISO_PARAM, aniso);
    }
    Ok(Tx { id, w, h, internal, hash: 0 })
}

/// Sampling state of one texture: filtering, mip maps, wrapping, LOD bias and anisotropy.
unsafe fn sample_params(gl: &glow::Context, id: glow::Texture, o: &GpuOpts, max_aniso: f32) {
    const LOD_BIAS: u32 = 0x8501;
    gl.bind_texture(glow::TEXTURE_2D, Some(id));
    let (min, mag) = match (o.filter, o.mipmaps) {
        (0, false) => (glow::NEAREST, glow::NEAREST),
        (0, true) => (glow::NEAREST_MIPMAP_NEAREST, glow::NEAREST),
        (1, false) | (2, false) => (glow::LINEAR, glow::LINEAR),
        (1, true) => (glow::LINEAR_MIPMAP_NEAREST, glow::LINEAR),
        _ => (glow::LINEAR_MIPMAP_LINEAR, glow::LINEAR),
    };
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, min as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, mag as i32);
    let wrap = match o.wrap {
        1 => glow::MIRRORED_REPEAT,
        2 => glow::CLAMP_TO_EDGE,
        _ => glow::REPEAT,
    };
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, wrap as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, wrap as i32);
    gl.tex_parameter_f32(glow::TEXTURE_2D, LOD_BIAS, o.lod_bias);
    let a = if o.aniso <= 0.0 { max_aniso } else { o.aniso.min(max_aniso) };
    if max_aniso > 1.0 {
        gl.tex_parameter_f32(glow::TEXTURE_2D, ANISO_PARAM, if o.filter == 0 || !o.mipmaps { 1.0 } else { a.max(1.0) });
    }
}

fn bytes_of<T>(v: &[T]) -> &[u8] {
    // plain-old-data (f32, u32, [u32; 3]) only
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

unsafe fn float_target(gl: &glow::Context, w: usize, h: usize, mips: bool) -> Result<(glow::Framebuffer, glow::Texture), String> {
    let tex = gl.create_texture()?;
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA16F as i32, w as i32, h as i32, 0, glow::RGBA, glow::HALF_FLOAT, glow::PixelUnpackData::Slice(None));
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, if mips { glow::LINEAR_MIPMAP_LINEAR } else { glow::LINEAR } as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
    let fbo = gl.create_framebuffer()?;
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
    gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
    if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
        return Err("the floating point render target is not supported by this driver".into());
    }
    Ok((fbo, tex))
}

unsafe fn make_targets(gl: &glow::Context, w: usize, h: usize, max_samples: i32) -> Result<Targets, String> {
    // resolved scene target: colour + depth textures
    let (res_fbo, hdr) = float_target(gl, w, h, false)?;
    let depth = gl.create_texture()?;
    gl.bind_texture(glow::TEXTURE_2D, Some(depth));
    gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::DEPTH_COMPONENT32F as i32, w as i32, h as i32, 0, glow::DEPTH_COMPONENT, glow::FLOAT, glow::PixelUnpackData::Slice(None));
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(res_fbo));
    gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::TEXTURE_2D, Some(depth), 0);
    if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
        return Err("the depth render target is not supported by this driver".into());
    }
    // multisampled twin
    let samples = max_samples.min(4);
    let (mut ms_fbo, mut ms_color, mut ms_depth) = (None, None, None);
    if samples >= 2 {
        let attempt = || -> Result<(glow::Framebuffer, glow::Renderbuffer, glow::Renderbuffer), String> {
            let c = gl.create_renderbuffer()?;
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(c));
            gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, samples, glow::RGBA16F, w as i32, h as i32);
            let d = gl.create_renderbuffer()?;
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(d));
            gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, samples, glow::DEPTH_COMPONENT32F, w as i32, h as i32);
            let f = gl.create_framebuffer()?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(f));
            gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(c));
            gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(d));
            if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
                gl.delete_framebuffer(f);
                gl.delete_renderbuffer(c);
                gl.delete_renderbuffer(d);
                return Err("multisampled target incomplete".into());
            }
            Ok((f, c, d))
        };
        if let Ok((f, c, d)) = attempt() {
            ms_fbo = Some(f);
            ms_color = Some(c);
            ms_depth = Some(d);
        }
    }
    let ping = [float_target(gl, w, h, false)?, float_target(gl, w, h, false)?];
    let bright = float_target(gl, w, h, true)?;
    Ok(Targets { w, h, ms_fbo, ms_color, ms_depth, res_fbo, hdr, depth, ping, bright })
}

unsafe fn free_targets(gl: &glow::Context, t: &Targets) {
    if let Some(f) = t.ms_fbo {
        gl.delete_framebuffer(f);
    }
    if let Some(r) = t.ms_color {
        gl.delete_renderbuffer(r);
    }
    if let Some(r) = t.ms_depth {
        gl.delete_renderbuffer(r);
    }
    gl.delete_framebuffer(t.res_fbo);
    gl.delete_texture(t.hdr);
    gl.delete_texture(t.depth);
    for (f, x) in &t.ping {
        gl.delete_framebuffer(*f);
        gl.delete_texture(*x);
    }
    gl.delete_framebuffer(t.bright.0);
    gl.delete_texture(t.bright.1);
}

unsafe fn upload_env(gl: &glow::Context, env: &Env, key: String) -> Result<EnvTex, String> {
    // the chain must be a proper mip chain (each level half the previous); keep the prefix that is
    let mut levels = 0;
    let (w0, h0) = (env.levels[0].w, env.levels[0].h);
    for (k, l) in env.levels.iter().enumerate() {
        if l.w == (w0 >> k).max(1) && l.h == (h0 >> k).max(1) {
            levels = k + 1;
        } else {
            break;
        }
    }
    let tex = gl.create_texture()?;
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
    for (k, l) in env.levels.iter().take(levels).enumerate() {
        let flat: Vec<f32> = l.px.iter().flat_map(|p| [p[0].min(60000.0), p[1].min(60000.0), p[2].min(60000.0)]).collect();
        gl.tex_image_2d(glow::TEXTURE_2D, k as i32, glow::RGB16F as i32, l.w as i32, l.h as i32, 0, glow::RGB, glow::FLOAT, glow::PixelUnpackData::Slice(Some(bytes_of(&flat))));
    }
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_BASE_LEVEL, 0);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAX_LEVEL, levels as i32 - 1);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
    let sh: Vec<f32> = env.sh.iter().flat_map(|c| [c[0], c[1], c[2]]).collect();
    Ok(EnvTex { key, tex, levels, sh })
}

unsafe fn make_shadow_maps(gl: &glow::Context, size: i32) -> Result<ShadowMaps, String> {
    let tex = gl.create_texture()?;
    gl.bind_texture(glow::TEXTURE_2D_ARRAY, Some(tex));
    gl.tex_image_3d(glow::TEXTURE_2D_ARRAY, 0, glow::DEPTH_COMPONENT32F as i32, size, size, 3, 0, glow::DEPTH_COMPONENT, glow::FLOAT, glow::PixelUnpackData::Slice(None));
    gl.tex_parameter_i32(glow::TEXTURE_2D_ARRAY, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D_ARRAY, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D_ARRAY, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D_ARRAY, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
    let fbo = gl.create_framebuffer()?;
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
    gl.framebuffer_texture_layer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, Some(tex), 0, 0);
    gl.draw_buffers(&[glow::NONE]);
    gl.read_buffer(glow::NONE);
    if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
        return Err("the shadow map target is not supported by this driver".into());
    }
    Ok(ShadowMaps { fbo, tex, size })
}

unsafe fn init(gl: &glow::Context, maps: &GpuMaps, aniso_req: f32) -> Result<Res, String> {
    let prog = compile(gl, SCENE_VS, SCENE_FS)?;
    let depth_prog = compile(gl, SCENE_VS, DEPTH_FS)?;
    let wire_prog = compile(gl, SCENE_VS, WIRE_FS)?;
    let bg_prog = compile(gl, BG_VS, BG_FS)?;
    let post_prog = compile(gl, FS_VS, POST_FS)?;
    let ext = gl.supported_extensions();
    let has_aniso = ext.contains("GL_EXT_texture_filter_anisotropic") || ext.contains("GL_ARB_texture_filter_anisotropic");
    let max_aniso = if has_aniso { gl.get_parameter_f32(ANISO_MAX).max(1.0) } else { 1.0 };
    let aniso = if aniso_req <= 0.0 { max_aniso } else { aniso_req.min(max_aniso) };
    let max_samples = gl.get_parameter_i32(glow::MAX_SAMPLES);
    let empty_vao = gl.create_vertex_array()?;
    let s = maps.size;
    let mut base = make_tex(gl, glow::SRGB8_ALPHA8, glow::RGBA, glow::UNSIGNED_BYTE, s, s, &maps.base, aniso)?;
    base.hash = maps.hashes[0];
    let mut normal = make_tex(gl, glow::RGBA8, glow::RGBA, glow::UNSIGNED_BYTE, s, s, &maps.normal, aniso)?;
    normal.hash = maps.hashes[1];
    let mut rma = make_tex(gl, glow::RGBA8, glow::RGBA, glow::UNSIGNED_BYTE, s, s, &maps.rma, aniso)?;
    rma.hash = maps.hashes[2];
    let emis = match &maps.emissive {
        Some(e) => {
            let mut t = make_tex(gl, glow::SRGB8_ALPHA8, glow::RGBA, glow::UNSIGNED_BYTE, s, s, e, aniso)?;
            t.hash = maps.hashes[3];
            Some(t)
        }
        None => None,
    };
    let height = match (&maps.height, &maps.height_half) {
        (Some((w, h, _)), Some(v)) => {
            let mut t = make_tex(gl, glow::R16F, glow::RED, glow::HALF_FLOAT, *w, *h, bytes_of(&v[..]), aniso)?;
            t.hash = maps.hashes[4];
            Some(t)
        }
        _ => None,
    };
    let dummy_h = make_tex(gl, glow::R16F, glow::RED, glow::FLOAT, 1, 1, bytes_of(&[0.0f32]), 1.0)?;
    // 1x1 stand-in so the shadow sampler is always complete
    let dummy_shadow = gl.create_texture()?;
    gl.bind_texture(glow::TEXTURE_2D_ARRAY, Some(dummy_shadow));
    gl.tex_image_3d(glow::TEXTURE_2D_ARRAY, 0, glow::DEPTH_COMPONENT32F as i32, 1, 1, 1, 0, glow::DEPTH_COMPONENT, glow::FLOAT, glow::PixelUnpackData::Slice(Some(bytes_of(&[1.0f32]))));
    gl.tex_parameter_i32(glow::TEXTURE_2D_ARRAY, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D_ARRAY, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
    Ok(Res {
        prog,
        depth_prog,
        wire_prog,
        bg_prog,
        post_prog,
        mesh: None,
        empty_vao,
        base,
        normal,
        rma,
        emis,
        height,
        dummy_h,
        dummy_shadow,
        max_aniso,
        max_samples,
        targets: None,
        env: None,
        shadows: None,
    })
}

unsafe fn free_tex(gl: &glow::Context, t: &Tx) {
    gl.delete_texture(t.id);
}

/// Puts a picture into a texture slot. The texture already there is kept when it holds the same picture, refilled
/// in place when only the picture changed, and replaced when its size or format changed.
#[allow(clippy::too_many_arguments)]
unsafe fn put_tex(gl: &glow::Context, slot: &mut Tx, internal: u32, fmt: u32, ty: u32, w: usize, h: usize, data: &[u8], hash: u64, aniso: f32) -> Result<(), String> {
    if slot.w == w && slot.h == h && slot.internal == internal {
        if slot.hash == 0 || slot.hash != hash {
            gl.bind_texture(glow::TEXTURE_2D, Some(slot.id));
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            gl.tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, w as i32, h as i32, fmt, ty, glow::PixelUnpackData::Slice(Some(data)));
            gl.generate_mipmap(glow::TEXTURE_2D);
            slot.hash = hash;
        }
        return Ok(());
    }
    let mut t = make_tex(gl, internal, fmt, ty, w, h, data, aniso)?;
    t.hash = hash;
    free_tex(gl, slot);
    *slot = t;
    Ok(())
}

/// Same for a slot that may be empty.
#[allow(clippy::too_many_arguments)]
unsafe fn put_opt(gl: &glow::Context, slot: &mut Option<Tx>, internal: u32, fmt: u32, ty: u32, size: Option<(usize, usize)>, data: Option<&[u8]>, hash: u64, aniso: f32) -> Result<(), String> {
    match (slot.as_mut(), size, data) {
        (Some(t), Some((w, h)), Some(d)) => put_tex(gl, t, internal, fmt, ty, w, h, d, hash, aniso),
        (None, Some((w, h)), Some(d)) => {
            let mut t = make_tex(gl, internal, fmt, ty, w, h, d, aniso)?;
            t.hash = hash;
            *slot = Some(t);
            Ok(())
        }
        _ => {
            if let Some(t) = slot.take() {
                free_tex(gl, &t);
            }
            Ok(())
        }
    }
}

unsafe fn upload_maps(gl: &glow::Context, r: &mut Res, maps: &GpuMaps, aniso_req: f32) -> Result<(), String> {
    let aniso = if aniso_req <= 0.0 { r.max_aniso } else { aniso_req.min(r.max_aniso) };
    let s = maps.size;
    put_tex(gl, &mut r.base, glow::SRGB8_ALPHA8, glow::RGBA, glow::UNSIGNED_BYTE, s, s, &maps.base, maps.hashes[0], aniso)?;
    put_tex(gl, &mut r.normal, glow::RGBA8, glow::RGBA, glow::UNSIGNED_BYTE, s, s, &maps.normal, maps.hashes[1], aniso)?;
    put_tex(gl, &mut r.rma, glow::RGBA8, glow::RGBA, glow::UNSIGNED_BYTE, s, s, &maps.rma, maps.hashes[2], aniso)?;
    put_opt(gl, &mut r.emis, glow::SRGB8_ALPHA8, glow::RGBA, glow::UNSIGNED_BYTE, Some((s, s)).filter(|_| maps.emissive.is_some()), maps.emissive.as_deref(), maps.hashes[3], aniso)?;
    let hs = maps.height.as_ref().map(|(w, h, _)| (*w, *h));
    put_opt(gl, &mut r.height, glow::R16F, glow::RED, glow::HALF_FLOAT, hs, maps.height_half.as_deref().map(bytes_of), maps.hashes[4], aniso)?;
    Ok(())
}

fn sample_height(h: &(usize, usize, Arc<Vec<f32>>), u: f32, v: f32) -> f32 {
    let (w, hh, d) = (h.0 as i64, h.1 as i64, &h.2);
    let x = u * w as f32 - 0.5;
    let y = v * hh as f32 - 0.5;
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let at = |xi: i64, yi: i64| d[(yi.rem_euclid(hh) as usize) * w as usize + xi.rem_euclid(w) as usize];
    let (xi, yi) = (x0 as i64, y0 as i64);
    let a = at(xi, yi) + (at(xi + 1, yi) - at(xi, yi)) * fx;
    let b = at(xi, yi + 1) + (at(xi + 1, yi + 1) - at(xi, yi + 1)) * fx;
    a + (b - a) * fy
}

unsafe fn upload_mesh(gl: &glow::Context, r: &mut Res, v: &View, height: &Option<(usize, usize, Arc<Vec<f32>>)>, _height_gen: u64) {
    let displacing = v.relief == RELIEF_DISPLACE && v.height_scale > 0.0 && height.is_some();
    let moves_floor = displacing && v.scene != scene::SceneKind::Single;
    let mut key = MeshKey {
        shape: v.shape,
        tess: v.tess.clamp(8, 768),
        tiling: (v.tiling.max(0.1) * 1000.0) as u32,
        scene: v.scene,
        custom: v.custom.as_ref().map(|c| std::sync::Arc::as_ptr(c) as usize).unwrap_or(0),
        floor: 0,
        disp: if moves_floor { (v.height_scale.to_bits(), v.height_level.to_bits()) } else { (0, 0) },
    };
    // lowest point of the displaced object for this height map, from the bottom vertices of a built scene
    let lowest = |bottom: &[(f32, [f32; 2], f32, f32)], ymin0: f32, hm: &(usize, usize, Arc<Vec<f32>>)| -> f32 {
        bottom.iter().fold(ymin0, |m, &(y, uv, dy, fade)| m.min(y + dy * (sample_height(hm, uv[0], uv[1]) - v.height_level) * v.height_scale * fade))
    };
    // the floor follows the displaced object in steps of 1/2048 (a smaller move is invisible and not worth a rebuild)
    let quant = |y: f32| (y * 2048.0).round() as i64;
    if moves_floor {
        if let (Some(old), Some(hm)) = (r.mesh.as_ref(), height.as_ref()) {
            let same_mesh = MeshKey { floor: old.key.floor, ..key.clone() } == old.key;
            if same_mesh {
                let y = lowest(&old.bottom, old.ymin0, hm);
                key.floor = if (y - old.ymin0).abs() > 1e-5 { quant(y) } else { 0 };
                if key.floor == old.key.floor {
                    return;
                }
            }
        }
    }
    if r.mesh.as_ref().map(|m| m.key == key).unwrap_or(false) {
        return;
    }
    let mut sm = objects::build_scene_ex(v, None, None);
    let (mut bottom, mut ymin0) = (Vec::new(), 0.0f32);
    if moves_floor {
        // the vertex shader displaces, so work out the lowest point of the displaced object here and put the floor
        // under it, like the software renderer does
        let hm = height.as_ref().unwrap();
        let m = &sm.mesh;
        let n0 = m.obj.iter().position(|&o| o != 0).map(|t| m.tris[t][0] as usize).unwrap_or(m.pos.len());
        ymin0 = (0..n0).map(|i| m.pos[i][1]).fold(f32::MAX, f32::min);
        bottom = (0..n0).filter(|&i| m.pos[i][1] < ymin0 + v.height_scale.abs() * 1.05 + 1e-4).map(|i| (m.pos[i][1], m.uv[i], m.disp[i][1], m.fade[i])).collect();
        let ymin = lowest(&bottom, ymin0, hm);
        if (ymin - ymin0).abs() > 1e-5 {
            key.floor = quant(ymin);
            sm = objects::build_scene_ex(v, None, Some(ymin));
        }
    }
    let m = &sm.mesh;
    let nv = m.pos.len();
    let mut data: Vec<f32> = Vec::with_capacity(nv * 18);
    let mut vert_obj = vec![0u32; nv];
    for (ti, t) in m.tris.iter().enumerate() {
        let o = m.object_of(ti) as u32;
        for &i in t {
            vert_obj[i as usize] = o;
        }
    }
    let mut radius = 1.0f32;
    for i in 0..nv {
        data.extend_from_slice(&m.pos[i]);
        data.extend_from_slice(&m.nrm[i]);
        data.extend_from_slice(&m.tan[i]);
        data.extend_from_slice(&m.uv[i]);
        data.extend_from_slice(&[m.su[i], m.sv[i], m.fade[i]]);
        data.extend_from_slice(&m.disp[i]);
        data.push(vert_obj[i] as f32);
        // planes (floor, backdrop) only receive shadows
        if sm.objects[vert_obj[i] as usize].shape != 2 {
            radius = radius.max(len(m.pos[i]));
        }
    }
    let displace_pad = if displacing { v.height_scale.abs() } else { 0.0 };
    let (vao, vbo, ebo) = match &r.mesh {
        Some(old) => (old.vao, old.vbo, old.ebo),
        None => (gl.create_vertex_array().unwrap(), gl.create_buffer().unwrap(), gl.create_buffer().unwrap()),
    };
    gl.bind_vertex_array(Some(vao));
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes_of(&data), glow::STATIC_DRAW);
    let stride = 18 * 4;
    for (loc, size, off) in [(0u32, 3, 0), (1, 3, 3), (2, 3, 6), (3, 2, 9), (4, 3, 11), (5, 3, 14), (6, 1, 17)] {
        gl.enable_vertex_attrib_array(loc);
        gl.vertex_attrib_pointer_f32(loc, size, glow::FLOAT, false, stride, off * 4);
    }
    gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
    gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, bytes_of(&m.tris), glow::STATIC_DRAW);
    r.mesh = Some(MeshGl { key, vao, vbo, ebo, index_count: (m.tris.len() * 3) as i32, objects: sm.objects, radius: radius + displace_pad, bottom, ymin0 });
}

// ------------------------------------------------------------------------------------------------ paint
fn fb_from(id: i32) -> Option<glow::Framebuffer> {
    std::num::NonZeroU32::new(id as u32).map(glow::NativeFramebuffer)
}

struct LightGl {
    axis: Vec<f32>,
    info: Vec<f32>,
    col: Vec<f32>,
    shadow: Vec<i32>,
    /// (view-projection, index of the light) per shadow layer
    vps: Vec<(M4, usize)>,
}

fn light_uniforms(v: &View, radius: f32, shadows_on: bool) -> LightGl {
    let cf = v.cam_frame();
    let mut o = LightGl { axis: vec![], info: vec![], col: vec![], shadow: vec![], vps: vec![] };
    for l in v.lights.iter().filter(|l| l.enabled && l.intensity > 0.0).take(scene::MAX_LIGHTS) {
        let axis = scene::light_axis(l, &cf);
        let dist = l.distance.max(0.2);
        let kind = match l.kind {
            LightKind::Directional => 0.0,
            LightKind::Point => 1.0,
            LightKind::Spot => 2.0,
        };
        let cos_out = l.cone.clamp(0.05, 1.5).cos();
        let cos_in = (l.cone * (1.0 - 0.6 * l.softness.clamp(0.0, 1.0))).clamp(0.02, 1.5).cos();
        o.axis.extend_from_slice(&[axis[0], axis[1], axis[2], dist]);
        o.info.extend_from_slice(&[kind, cos_out, cos_in, l.softness]);
        o.col.extend_from_slice(&[l.color[0] * l.intensity, l.color[1] * l.intensity, l.color[2] * l.intensity]);
        let idx = o.shadow.len();
        let mut layer = -1;
        if shadows_on && l.shadows && o.vps.len() < 3 {
            let r = radius * 1.05 + 0.3;
            let vp = match l.kind {
                LightKind::Directional => {
                    let eye = mul(axis, r * 3.0);
                    m_mul(&ortho(r + 1.0, 0.1, r * 6.0), &look_at(eye, [0.0; 3], [0.0, 1.0, 0.0]))
                }
                LightKind::Point | LightKind::Spot => {
                    let eye = mul(axis, dist);
                    let fov = if l.kind == LightKind::Spot { (2.0 * l.cone + 0.25).clamp(0.35, 2.1) } else { 1.9 };
                    m_mul(&perspective((fov * 0.5).tan(), 1.0, 0.3, dist + r * 3.0), &look_at(eye, [0.0; 3], [0.0, 1.0, 0.0]))
                }
            };
            layer = o.vps.len() as i32;
            o.vps.push((vp, idx));
        }
        o.shadow.push(layer);
    }
    o
}

/// Draw the viewport into the rectangle of the paint callback. Call from an `egui_glow::CallbackFn`.
pub fn paint(shared: &Shared, info: &PaintCallbackInfo, painter: &egui_glow::Painter, view: &View) {
    let mut g = match shared.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    if g.error.is_some() {
        return;
    }
    let gl: &glow::Context = painter.gl();
    let aniso_req = g.opts.aniso;
    // first use: create everything from the pending maps (or neutral defaults)
    if g.res.is_none() {
        let maps = g.pending.take().unwrap_or_else(|| prepare(&MapInputs { base: None, normal: None, rough: None, metal: None, ao: None, emissive: None, height: None }));
        match unsafe { init(gl, &maps, aniso_req) } {
            Ok(r) => {
                g.renderer = unsafe { gl.get_parameter_string(glow::RENDERER) };
                g.res = Some(r);
                g.applied = None;
            }
            Err(e) => {
                g.error = Some(e);
                return;
            }
        }
    } else if let Some(maps) = g.pending.take() {
        let mut r = g.res.take().unwrap();
        let res = unsafe { upload_maps(gl, &mut r, &maps, aniso_req) };
        g.res = Some(r);
        g.applied = None;
        if let Err(e) = res {
            g.error = Some(e);
            return;
        }
    }
    // texture sampling options (filtering, wrapping, mip maps, bias, anisotropy) follow the settings
    if g.applied.as_ref() != Some(&g.opts) {
        let o = g.opts.clone();
        if let Some(r) = g.res.as_ref() {
            unsafe {
                for t in [Some(&r.base), Some(&r.normal), Some(&r.rma), r.emis.as_ref(), r.height.as_ref()].into_iter().flatten() {
                    sample_params(gl, t.id, &o, r.max_aniso);
                }
            }
        }
        g.applied = Some(o);
    }
    // the lighting environment
    let want_key = view.env.image_key();
    let need_env = g.res.as_ref().unwrap().env.as_ref().map(|e| e.key != want_key).unwrap_or(true);
    if need_env {
        let loaded = crate::hdri::load_env(&view.env.preset, view.env.file.as_deref());
        let env = match loaded {
            Ok(e) => {
                g.env_error = None;
                e
            }
            Err(msg) => {
                g.env_error = Some(msg);
                crate::hdri::load_env("studio", None).expect("built-in environment")
            }
        };
        let tex = unsafe { upload_env(gl, &env, want_key.clone()) };
        match tex {
            Ok(t) => {
                let r = g.res.as_mut().unwrap();
                if let Some(old) = r.env.take() {
                    unsafe { gl.delete_texture(old.tex) };
                }
                r.env = Some(t);
            }
            Err(e) => {
                g.error = Some(e);
                return;
            }
        }
    }
    let self_shadow = g.opts.self_shadow;
    let want_shot = std::mem::take(&mut g.want_shot);
    let height_cpu = g.height_cpu.clone();
    let height_gen = g.height_gen;
    let r = g.res.as_mut().unwrap();
    let vp_px = info.viewport_in_pixels();
    let clip_px = info.clip_rect_in_pixels();
    if vp_px.width_px <= 0 || vp_px.height_px <= 0 {
        return;
    }
    let (w, h) = (vp_px.width_px as usize, vp_px.height_px as usize);
    let cam = view.cam();
    let aspect = w as f32 / h as f32;
    let view_m = look_at(cam.eye, add(cam.eye, cam.fwd), [0.0, 1.0, 0.0]);
    let vp = m_mul(&perspective(cam.tan_half, aspect, NEAR, FAR), &view_m);
    let has_h = r.height.is_some();
    let disp = if has_h && view.relief == RELIEF_DISPLACE { view.height_scale } else { 0.0 };
    let mesh_n = {
        let n = view.tess.clamp(8, 768);
        match view.shape {
            1 | 4 | 5 | 9 => (n * 5 / 8).max(8),
            6 => (n * 2).min(1024),
            _ => n,
        }
    } as f32;
    let (hw, hh) = r.height.as_ref().map(|t| (t.w, t.h)).unwrap_or((1, 1));
    let vtx_lod = ((hw as f32 * view.tiling.max(0.1) / mesh_n).max(1.0)).log2();
    let mode = if view.relief == RELIEF_BUMP {
        1
    } else if view.relief == RELIEF_PARALLAX {
        2
    } else if view.relief == RELIEF_DISPLACE {
        3
    } else {
        0
    };
    let lit = view.channel == Channel::Lit;
    let mut result: Result<(), String> = Ok(());
    unsafe {
        // remember what egui had set up
        let saved_fb = gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING);
        let scissor_was_on = gl.is_enabled(glow::SCISSOR_TEST);
        upload_mesh(gl, r, view, &height_cpu, height_gen);
        // render targets
        if r.targets.as_ref().map(|t| t.w != w || t.h != h).unwrap_or(true) {
            if let Some(t) = r.targets.take() {
                free_targets(gl, &t);
            }
            match make_targets(gl, w, h, r.max_samples) {
                Ok(t) => r.targets = Some(t),
                Err(e) => {
                    g.error = Some(e);
                    gl.bind_framebuffer(glow::FRAMEBUFFER, fb_from(saved_fb));
                    return;
                }
            }
        }
        let radius = r.mesh.as_ref().map(|m| m.radius).unwrap_or(1.0);
        let lg = light_uniforms(view, radius, lit);
        if !lg.vps.is_empty() && r.shadows.is_none() {
            match make_shadow_maps(gl, 2048) {
                Ok(s) => r.shadows = Some(s),
                Err(e) => {
                    g.error = Some(e);
                    gl.bind_framebuffer(glow::FRAMEBUFFER, fb_from(saved_fb));
                    return;
                }
            }
        }
        let mesh = r.mesh.as_ref().unwrap();
        let tg = r.targets.as_ref().unwrap();
        gl.disable(glow::SCISSOR_TEST);
        gl.disable(glow::BLEND);
        gl.disable(glow::CULL_FACE);
        // per-object material table
        let mut obj_a = [0.0f32; 32];
        let mut obj_c = [0.0f32; 24];
        for (i, o) in mesh.objects.iter().take(8).enumerate() {
            let (kind, ro, me, col) = match o.material {
                scene::Material::Graph => (0.0, 0.5, 0.0, [0.0; 3]),
                scene::Material::Fixed(c, ro, me) => (1.0, ro, me, c),
            };
            let cull = if o.material == scene::Material::Graph && crate::render3d::shape_culls(view.shape) { 1.0 } else { 0.0 };
            obj_a[i * 4..i * 4 + 4].copy_from_slice(&[kind, ro, me, cull]);
            obj_c[i * 3..i * 3 + 3].copy_from_slice(&col);
        }
        let bind_maps = |gl: &glow::Context, r: &Res| {
            let units: [glow::Texture; 5] = [r.base.id, r.normal.id, r.rma.id, r.emis.as_ref().map(|t| t.id).unwrap_or(r.base.id), r.height.as_ref().map(|t| t.id).unwrap_or(r.dummy_h.id)];
            for (i, tex) in units.iter().enumerate() {
                gl.active_texture(glow::TEXTURE0 + i as u32);
                gl.bind_texture(glow::TEXTURE_2D, Some(*tex));
            }
        };
        // ---------------------------------------------------------------- shadow maps
        if let Some(sm) = &r.shadows {
            if !lg.vps.is_empty() {
                let p = &r.depth_prog;
                gl.use_program(Some(p.id));
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(sm.fbo));
                gl.viewport(0, 0, sm.size, sm.size);
                gl.enable(glow::DEPTH_TEST);
                gl.depth_func(glow::LESS);
                gl.depth_mask(true);
                gl.enable(glow::POLYGON_OFFSET_FILL);
                gl.polygon_offset(1.5, 2.0);
                gl.uniform_1_f32(p.loc("uDisp"), disp);
                gl.uniform_1_f32(p.loc("uLevel"), view.height_level);
                gl.uniform_1_f32(p.loc("uVtxLod"), vtx_lod);
                gl.uniform_4_f32_slice(p.loc("uObjA"), &obj_a);
                bind_maps(gl, r);
                gl.uniform_1_i32(p.loc("uHeight"), 4);
                gl.bind_vertex_array(Some(mesh.vao));
                for (layer, (lvp, _)) in lg.vps.iter().enumerate() {
                    gl.framebuffer_texture_layer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, Some(sm.tex), 0, layer as i32);
                    gl.clear_depth_f32(1.0);
                    gl.clear(glow::DEPTH_BUFFER_BIT);
                    gl.uniform_matrix_4_f32_slice(p.loc("uVP"), false, lvp);
                    gl.draw_elements(glow::TRIANGLES, mesh.index_count, glow::UNSIGNED_INT, 0);
                }
                gl.disable(glow::POLYGON_OFFSET_FILL);
            }
        }
        // ---------------------------------------------------------------- scene into the HDR target
        let scene_fbo = tg.ms_fbo.unwrap_or(tg.res_fbo);
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(scene_fbo));
        gl.viewport(0, 0, w as i32, h as i32);
        gl.clear_color(0.0, 0.0, 0.0, 1.0);
        gl.depth_mask(true);
        gl.clear_depth_f32(1.0);
        gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
        let env = r.env.as_ref().unwrap();
        let (sr, cr) = view.env.rotation.sin_cos();
        let bg_gain = view.env.exposure.exp2();
        // background (no depth test, no depth write)
        gl.disable(glow::DEPTH_TEST);
        gl.depth_mask(false);
        {
            let p = &r.bg_prog;
            gl.use_program(Some(p.id));
            gl.active_texture(glow::TEXTURE5);
            gl.bind_texture(glow::TEXTURE_2D, Some(env.tex));
            gl.uniform_1_i32(p.loc("uEnv"), 5);
            gl.uniform_1_i32(p.loc("uBgMode"), match view.env.background {
                Background::Gradient => 0,
                Background::Color => 1,
                Background::Environment => 2,
            });
            gl.uniform_1_i32(p.loc("uChannel"), view.channel.index());
            let bc = view.env.bg_color;
            gl.uniform_3_f32(p.loc("uBgColor"), bc[0].max(0.0).powf(2.2), bc[1].max(0.0).powf(2.2), bc[2].max(0.0).powf(2.2));
            gl.uniform_2_f32(p.loc("uEnvRot"), cr, sr);
            gl.uniform_1_f32(p.loc("uBgGain"), bg_gain * view.exposure);
            gl.uniform_1_f32(p.loc("uBgLod"), view.env.blur.clamp(0.0, 1.0) * (env.levels.max(1) - 1) as f32);
            gl.uniform_1_f32(p.loc("uTanHalf"), cam.tan_half);
            gl.uniform_1_f32(p.loc("uAspect"), aspect);
            gl.uniform_3_f32(p.loc("uCamR"), cam.right[0], cam.right[1], cam.right[2]);
            gl.uniform_3_f32(p.loc("uCamU"), cam.up[0], cam.up[1], cam.up[2]);
            gl.uniform_3_f32(p.loc("uCamF"), cam.fwd[0], cam.fwd[1], cam.fwd[2]);
            gl.bind_vertex_array(Some(r.empty_vao));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
        }
        gl.enable(glow::DEPTH_TEST);
        gl.depth_func(glow::LESS);
        gl.depth_mask(true);
        {
            let p = &r.prog;
            gl.use_program(Some(p.id));
            gl.uniform_matrix_4_f32_slice(p.loc("uVP"), false, &vp);
            gl.uniform_3_f32(p.loc("uEye"), cam.eye[0], cam.eye[1], cam.eye[2]);
            gl.uniform_1_f32(p.loc("uExposure"), view.exposure);
            gl.uniform_1_f32(p.loc("uNormalAmt"), view.normal_amt.clamp(0.0, 1.0));
            gl.uniform_1_f32(p.loc("uBump"), view.bump);
            gl.uniform_1_f32(p.loc("uDepth"), view.height_scale);
            gl.uniform_1_f32(p.loc("uLevel"), view.height_level);
            gl.uniform_1_f32(p.loc("uDisp"), disp);
            gl.uniform_1_f32(p.loc("uVtxLod"), vtx_lod);
            gl.uniform_1_f32(p.loc("uTexel"), 1.0 / hw.max(hh).max(1) as f32);
            gl.uniform_1_i32(p.loc("uMode"), mode);
            gl.uniform_1_i32(p.loc("uHasHeight"), has_h as i32);
            gl.uniform_1_i32(p.loc("uHasEmis"), r.emis.is_some() as i32);
            gl.uniform_1_i32(p.loc("uShadow"), self_shadow as i32);
            gl.uniform_1_i32(p.loc("uChannel"), view.channel.index());
            gl.uniform_4_f32_slice(p.loc("uObjA"), &obj_a);
            gl.uniform_3_f32_slice(p.loc("uObjC"), &obj_c);
            // lights and environment
            let nl = lg.shadow.len();
            gl.uniform_1_i32(p.loc("uNLights"), nl as i32);
            let pad4 = |v: &Vec<f32>| -> Vec<f32> {
                let mut o = v.clone();
                o.resize(scene::MAX_LIGHTS * 4, 0.0);
                o
            };
            gl.uniform_4_f32_slice(p.loc("uLAxis"), &pad4(&lg.axis));
            gl.uniform_4_f32_slice(p.loc("uLInfo"), &pad4(&lg.info));
            let mut col = lg.col.clone();
            col.resize(scene::MAX_LIGHTS * 3, 0.0);
            gl.uniform_3_f32_slice(p.loc("uLCol"), &col);
            let mut sh = lg.shadow.clone();
            sh.resize(scene::MAX_LIGHTS, -1);
            gl.uniform_1_i32_slice(p.loc("uLShadow"), &sh);
            let mut vps = [0.0f32; 48];
            for (i, (m, _)) in lg.vps.iter().enumerate() {
                vps[i * 16..i * 16 + 16].copy_from_slice(m);
            }
            gl.uniform_matrix_4_f32_slice(p.loc("uLVP"), false, &vps);
            gl.uniform_3_f32_slice(p.loc("uSH"), &env.sh);
            gl.uniform_2_f32(p.loc("uEnvRot"), cr, sr);
            gl.uniform_1_f32(p.loc("uEnvGain"), bg_gain * view.ambient);
            gl.uniform_1_f32(p.loc("uEnvLevels"), env.levels as f32);
            bind_maps(gl, r);
            for (i, n) in ["uBase", "uNormal", "uRma", "uEmis", "uHeight"].iter().enumerate() {
                gl.uniform_1_i32(p.loc(n), i as i32);
            }
            gl.active_texture(glow::TEXTURE5);
            gl.bind_texture(glow::TEXTURE_2D, Some(env.tex));
            gl.uniform_1_i32(p.loc("uEnv"), 5);
            gl.active_texture(glow::TEXTURE6);
            gl.bind_texture(glow::TEXTURE_2D_ARRAY, Some(r.shadows.as_ref().map(|s| s.tex).unwrap_or(r.dummy_shadow)));
            gl.uniform_1_i32(p.loc("uShadowMap"), 6);
            gl.bind_vertex_array(Some(mesh.vao));
            gl.draw_elements(glow::TRIANGLES, mesh.index_count, glow::UNSIGNED_INT, 0);
        }
        // resolve the multisampled target
        if let Some(ms) = tg.ms_fbo {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(ms));
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(tg.res_fbo));
            gl.blit_framebuffer(0, 0, w as i32, h as i32, 0, 0, w as i32, h as i32, glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT, glow::NEAREST);
        }
        // ---------------------------------------------------------------- post chain
        gl.disable(glow::DEPTH_TEST);
        gl.depth_mask(false);
        let pp = &r.post_prog;
        gl.use_program(Some(pp.id));
        gl.bind_vertex_array(Some(r.empty_vao));
        gl.uniform_2_f32(pp.loc("uSize"), w as f32, h as f32);
        gl.uniform_2_f32(pp.loc("uOrigin"), 0.0, 0.0);
        gl.uniform_1_f32(pp.loc("uNear"), NEAR);
        gl.uniform_1_f32(pp.loc("uFar"), FAR);
        gl.uniform_1_f32(pp.loc("uTanHalf"), cam.tan_half);
        gl.uniform_1_f32(pp.loc("uAspect"), aspect);
        gl.uniform_1_f32(pp.loc("uCentreDist"), cam.dist);
        let po = &view.post;
        let (ssao_on, dof_on, bloom_on) = (lit && po.ssao > 0.0, lit && po.dof > 0.0, lit && po.bloom > 0.0);
        gl.uniform_1_f32(pp.loc("uSsao"), po.ssao);
        gl.uniform_1_f32(pp.loc("uSsaoRadius"), po.ssao_radius);
        gl.uniform_1_f32(pp.loc("uDof"), po.dof);
        gl.uniform_1_f32(pp.loc("uFocus"), po.focus);
        gl.uniform_1_f32(pp.loc("uBloom"), po.bloom);
        gl.uniform_1_f32(pp.loc("uBloomThreshold"), po.bloom_threshold);
        gl.uniform_1_f32(pp.loc("uVignette"), po.vignette);
        gl.uniform_1_f32(pp.loc("uContrast"), po.contrast);
        gl.uniform_1_f32(pp.loc("uSaturation"), po.saturation);
        gl.uniform_3_f32(pp.loc("uWb"), 1.0 + 0.25 * po.temperature - 0.1 * po.tint, 1.0 + 0.2 * po.tint, 1.0 - 0.25 * po.temperature - 0.1 * po.tint);
        gl.uniform_1_i32(pp.loc("uTone"), po.tonemap.index());
        gl.uniform_1_i32(pp.loc("uBypass"), (!lit) as i32);
        // bloom weights, as in post.rs
        let mut nb = 1usize;
        let (mut bw, mut bh) = (w, h);
        for _ in 0..6 {
            if bw <= 2 || bh <= 2 {
                break;
            }
            bw = (bw / 2).max(1);
            bh = (bh / 2).max(1);
            nb += 1;
        }
        let rr = po.bloom_radius.clamp(0.0, 1.0);
        let mut wts = [0.0f32; 8];
        for (k, wk) in wts.iter_mut().enumerate().take(nb) {
            let centre = 1.5 + rr * 3.0;
            *wk = (-((k as f32 - centre) / 1.1).powi(2)).exp();
        }
        let wsum: f32 = wts.iter().sum::<f32>().max(1e-6);
        for wk in wts.iter_mut() {
            *wk /= wsum;
        }
        gl.uniform_1_i32(pp.loc("uBloomLevels"), nb as i32);
        gl.uniform_1_f32_slice(pp.loc("uBloomW"), &wts);
        for (i, n) in ["uSrc", "uDepthTex", "uBright"].iter().enumerate() {
            gl.uniform_1_i32(pp.loc(n), i as i32);
        }
        let pass = |gl: &glow::Context, stage: i32, src: glow::Texture, dst: Option<glow::Framebuffer>| {
            gl.bind_framebuffer(glow::FRAMEBUFFER, dst);
            gl.uniform_1_i32(pp.loc("uStage"), stage);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(src));
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(tg.depth));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
        };
        let mut cur = tg.hdr;
        if ssao_on {
            pass(gl, 0, cur, Some(tg.ping[0].0));
            cur = tg.ping[0].1;
        }
        if dof_on {
            let dst = if cur == tg.ping[0].1 { 1 } else { 0 };
            pass(gl, 1, cur, Some(tg.ping[dst].0));
            cur = tg.ping[dst].1;
        }
        if bloom_on {
            pass(gl, 2, cur, Some(tg.bright.0));
            gl.bind_texture(glow::TEXTURE_2D, Some(tg.bright.1));
            gl.generate_mipmap(glow::TEXTURE_2D);
            gl.active_texture(glow::TEXTURE2);
            gl.bind_texture(glow::TEXTURE_2D, Some(tg.bright.1));
        }
        // final pass into the window, inside the rectangle egui gave us
        gl.viewport(vp_px.left_px, vp_px.from_bottom_px, w as i32, h as i32);
        gl.enable(glow::SCISSOR_TEST);
        gl.scissor(clip_px.left_px, clip_px.from_bottom_px, clip_px.width_px, clip_px.height_px);
        // the shader addresses pixels relative to the target, so the final pass reads gl_FragCoord offset by the viewport
        gl.uniform_2_f32(pp.loc("uOrigin"), vp_px.left_px as f32, vp_px.from_bottom_px as f32);
        pass(gl, 3, cur, fb_from(saved_fb));
        // wireframe overlay, hidden lines removed with the resolved depth
        if view.wireframe {
            let p = &r.wire_prog;
            gl.use_program(Some(p.id));
            gl.uniform_matrix_4_f32_slice(p.loc("uVP"), false, &vp);
            gl.uniform_1_f32(p.loc("uDisp"), disp);
            gl.uniform_1_f32(p.loc("uLevel"), view.height_level);
            gl.uniform_1_f32(p.loc("uVtxLod"), vtx_lod);
            gl.uniform_4_f32_slice(p.loc("uObjA"), &obj_a);
            gl.uniform_1_f32(p.loc("uNear"), NEAR);
            gl.uniform_1_f32(p.loc("uFar"), FAR);
            bind_maps(gl, r);
            gl.uniform_1_i32(p.loc("uHeight"), 4);
            gl.active_texture(glow::TEXTURE7);
            gl.bind_texture(glow::TEXTURE_2D, Some(tg.depth));
            gl.uniform_1_i32(p.loc("uDepthTex"), 7);
            gl.uniform_2_f32(p.loc("uOrigin"), vp_px.left_px as f32, vp_px.from_bottom_px as f32);
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.polygon_mode(glow::FRONT_AND_BACK, glow::LINE);
            gl.bind_vertex_array(Some(mesh.vao));
            gl.draw_elements(glow::TRIANGLES, mesh.index_count, glow::UNSIGNED_INT, 0);
            gl.polygon_mode(glow::FRONT_AND_BACK, glow::FILL);
            gl.disable(glow::BLEND);
        }
        if !scissor_was_on {
            gl.disable(glow::SCISSOR_TEST);
        }
        // leave the state the way egui expects it
        for u in [7u32, 6, 5, 4, 3, 2, 1] {
            gl.active_texture(glow::TEXTURE0 + u);
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
        gl.active_texture(glow::TEXTURE6);
        gl.bind_texture(glow::TEXTURE_2D_ARRAY, None);
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.bind_vertex_array(None);
        gl.bind_framebuffer(glow::FRAMEBUFFER, fb_from(saved_fb));
        gl.disable(glow::DEPTH_TEST);
        gl.depth_mask(true);
        let e = gl.get_error();
        if e != glow::NO_ERROR {
            result = Err(format!("OpenGL error 0x{e:x} while drawing the 3D view"));
        }
    }
    if want_shot && result.is_ok() {
        let buf_w = vp_px.width_px as usize;
        let buf_h = vp_px.height_px as usize;
        let mut buf = vec![0u8; buf_w * buf_h * 4];
        unsafe {
            gl.read_pixels(vp_px.left_px, vp_px.from_bottom_px, buf_w as i32, buf_h as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut buf)));
        }
        // flip: GL rows start at the bottom
        let mut top = vec![0u8; buf_w * buf_h * 4];
        for y in 0..buf_h {
            top[y * buf_w * 4..(y + 1) * buf_w * 4].copy_from_slice(&buf[(buf_h - 1 - y) * buf_w * 4..(buf_h - y) * buf_w * 4]);
        }
        g.shot = Some((buf_w, buf_h, top));
    }
    g.frames += 1;
    if let Err(e) = result {
        g.error = Some(e);
    }
}

/// Free all GL objects (call when the viewer is closed or the GL context goes away).
pub fn release(shared: &Shared, gl: &glow::Context) {
    if let Ok(mut g) = shared.lock() {
        if let Some(r) = g.res.take() {
            unsafe {
                for p in [&r.prog, &r.depth_prog, &r.wire_prog, &r.bg_prog, &r.post_prog] {
                    gl.delete_program(p.id);
                }
                if let Some(m) = &r.mesh {
                    gl.delete_vertex_array(m.vao);
                    gl.delete_buffer(m.vbo);
                    gl.delete_buffer(m.ebo);
                }
                gl.delete_vertex_array(r.empty_vao);
                if let Some(t) = &r.targets {
                    free_targets(gl, t);
                }
                if let Some(e) = &r.env {
                    gl.delete_texture(e.tex);
                }
                if let Some(s) = &r.shadows {
                    gl.delete_framebuffer(s.fbo);
                    gl.delete_texture(s.tex);
                }
                gl.delete_texture(r.dummy_shadow);
                free_tex(gl, &r.base);
                free_tex(gl, &r.normal);
                free_tex(gl, &r.rma);
                free_tex(gl, &r.dummy_h);
                if let Some(t) = &r.emis {
                    free_tex(gl, t);
                }
                if let Some(t) = &r.height {
                    free_tex(gl, t);
                }
            }
        }
    }
}
