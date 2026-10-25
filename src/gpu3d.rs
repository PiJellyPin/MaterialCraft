// SPDX-License-Identifier: MIT OR Apache-2.0
//! GPU PBR viewport on wgpu.
//!
//! The 3D view renders on the same device as the graph kernels. The material maps are read from the graph's device
//! buffers, packed into sampled textures (with their mip chains) by compute and render passes, and stay in device
//! memory: a camera or light move never touches them, and nothing but the finished frame comes back to the host.
//!
//! The scene (the shape, optionally with floor, reference balls or a pedestal) is one tessellated triangle mesh. The
//! height map displaces the vertices in the vertex shader (true displacement); the fragment shader does GGX PBR with
//! normal mapping, outward parallax relief, a light rig with shadow maps and image based lighting from an HDR
//! environment (prefiltered levels plus spherical harmonics). The frame is rendered 4x multisampled into a floating
//! point target and then goes through the post chain (ambient occlusion, depth of field, bloom, grading, tone
//! mapping), a line by line port of `post.rs`, so the CPU renderer (`raster.rs`) shows the same picture.
use crate::gpu::GTex;
use crate::hdri::Env;
use crate::objects;
use crate::render3d::{View, RELIEF_BUMP, RELIEF_DISPLACE, RELIEF_PARALLAX};
use crate::scene::{self, Background, Channel, LightKind};
use crate::tex::{lum, par_map, Tex};
use crate::v3::*;
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
    /// maps that still sit in device memory (the packing into textures happens there); the byte vectors above are
    /// empty then
    pub dev: Option<DevMaps>,
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
    GpuMaps { size, base, normal, rma, emissive, height, height_half, hashes, dev: None }
}

/// The graph outputs as pictures that are still in device memory.
pub struct DevInputs<'a> {
    pub base: Option<&'a crate::gpu::GTex>,
    pub normal: Option<&'a crate::gpu::GTex>,
    /// the normal map is DirectX style: flip its green channel for the viewer
    pub flip_normal_g: bool,
    pub rough: Option<&'a crate::gpu::GTex>,
    /// `rough` is a glossiness map: use `1 - gloss`
    pub rough_invert: bool,
    pub metal: Option<&'a crate::gpu::GTex>,
    pub ao: Option<&'a crate::gpu::GTex>,
    pub emissive: Option<&'a crate::gpu::GTex>,
    pub height: Option<&'a crate::gpu::GTex>,
}

/// Graph outputs that are still in device memory; the viewer packs them into its textures there.
#[derive(Clone)]
pub struct DevMaps {
    base: Option<GTex>,
    normal: Option<GTex>,
    flip_normal_g: bool,
    rough: Option<GTex>,
    rough_invert: bool,
    metal: Option<GTex>,
    ao: Option<GTex>,
    emissive: Option<GTex>,
    height: Option<GTex>,
}

fn mix_ids(parts: &[u64]) -> u64 {
    let mut h: u64 = 0x9e37_79b9_7f4a_7c15;
    for p in parts {
        h = (h ^ p).wrapping_mul(0x2545_f491_4f6c_dd1d);
        h ^= h >> 31;
    }
    h | 1
}

/// `prepare` for maps that are already on the device: nothing is read back. The maps are packed into the viewer's
/// textures on the device when the next frame is drawn. `None` when the host code has to do it: a map is missing from
/// the device, the maps differ in size, something is larger than the viewer's texture limit or there is no adapter.
pub fn prepare_dev(m: &DevInputs) -> Option<GpuMaps> {
    let (_, _) = crate::gpu::viewer_parts()?;
    let lim = crate::gpu::viewer_limits()?;
    let colour = [m.base, m.normal, m.rough, m.metal, m.ao, m.emissive];
    let size = colour.iter().flatten().map(|t| t.w().max(t.h())).max().unwrap_or(4).clamp(1, 4096);
    if colour.iter().flatten().any(|t| t.w() != size || t.h() != size) {
        return None;
    }
    if let Some(h) = m.height {
        if h.w() as u32 > lim.max_texture_dimension_2d || h.h() as u32 > lim.max_texture_dimension_2d {
            return None;
        }
    }
    if size as u32 > lim.max_texture_dimension_2d {
        return None;
    }
    let id = |t: Option<&GTex>| t.map(|t| t.id() as u64).unwrap_or(0);
    let hashes = [
        mix_ids(&[1, id(m.base)]),
        mix_ids(&[2, id(m.normal), m.flip_normal_g as u64]),
        mix_ids(&[3, id(m.rough), m.rough_invert as u64, id(m.metal), id(m.ao)]),
        if m.emissive.is_some() { mix_ids(&[4, id(m.emissive)]) } else { 0 },
        if let Some(h) = m.height { mix_ids(&[5, id(Some(h)), (h.w() * 65536 + h.h()) as u64]) } else { 0 },
    ];
    let dev = DevMaps {
        base: m.base.cloned(),
        normal: m.normal.cloned(),
        flip_normal_g: m.flip_normal_g,
        rough: m.rough.cloned(),
        rough_invert: m.rough_invert,
        metal: m.metal.cloned(),
        ao: m.ao.cloned(),
        emissive: m.emissive.cloned(),
        height: m.height.cloned(),
    };
    Some(GpuMaps { size, base: Vec::new(), normal: Vec::new(), rma: Vec::new(), emissive: None, height: None, height_half: None, hashes, dev: Some(dev) })
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

/// Perspective with the depth range 0..1 (wgpu clip space).
fn perspective(tan_half: f32, aspect: f32, n: f32, f: f32) -> M4 {
    [
        1.0 / (aspect * tan_half), 0.0, 0.0, 0.0, //
        0.0, 1.0 / tan_half, 0.0, 0.0, //
        0.0, 0.0, f / (n - f), -1.0, //
        0.0, 0.0, n * f / (n - f), 0.0,
    ]
}

/// Orthographic with the depth range 0..1.
fn ortho(half: f32, n: f32, f: f32) -> M4 {
    [
        1.0 / half, 0.0, 0.0, 0.0, //
        0.0, 1.0 / half, 0.0, 0.0, //
        0.0, 0.0, 1.0 / (n - f), 0.0, //
        0.0, 0.0, n / (n - f), 1.0,
    ]
}

const NEAR: f32 = 0.03;
const FAR: f32 = 80.0;
const SHADOW_SIZE: u32 = 2048;
const SHADOW_LAYERS: u32 = 3;

fn bytes_of<T>(v: &[T]) -> &[u8] {
    // plain-old-data (f32, u32, u16, [u32; 3]) only
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

fn align_up(v: u32, a: u32) -> u32 {
    v.div_ceil(a) * a
}

/// Writer for uniform blocks (everything is a multiple of 16 bytes, like the WGSL structs).
#[derive(Default)]
struct Ub(Vec<u8>);

impl Ub {
    fn f(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn v4(&mut self, a: [f32; 4]) {
        for x in a {
            self.f(x);
        }
    }
    fn u4(&mut self, a: [u32; 4]) {
        for x in a {
            self.u(x);
        }
    }
    fn m4(&mut self, m: &M4) {
        for x in m {
            self.f(*x);
        }
    }
}

// ------------------------------------------------------------------------------------------------ device objects
type Dev = &'static wgpu::Device;
type Queue = &'static wgpu::Queue;

/// Runs `f` and returns the first validation / out of memory error the device reported meanwhile. Errors found here
/// stay inside the 3D view (they switch it to the software renderer) and never reach the compute engine.
fn guard<T>(dev: Dev, f: impl FnOnce() -> T) -> Result<T, String> {
    dev.push_error_scope(wgpu::ErrorFilter::Validation);
    dev.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let v = f();
    let oom = pollster::block_on(dev.pop_error_scope());
    let val = pollster::block_on(dev.pop_error_scope());
    match (val, oom) {
        (Some(e), _) => Err(format!("3D view: {e}")),
        (_, Some(e)) => Err(format!("3D view: {e}")),
        _ => Ok(v),
    }
}

/// A sampled picture.
struct Tx {
    tex: wgpu::Texture,
    view: wgpu::TextureView,
    w: u32,
    h: u32,
    mips: u32,
    fmt: wgpu::TextureFormat,
    /// fingerprint of the picture it holds (0 = unknown)
    hash: u64,
}

fn mip_count(w: u32, h: u32) -> u32 {
    32 - w.max(h).max(1).leading_zeros()
}

fn make_tx(dev: Dev, w: u32, h: u32, fmt: wgpu::TextureFormat, mips: bool, label: &str) -> Tx {
    let n = if mips { mip_count(w, h) } else { 1 };
    let tex = dev.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: n,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: fmt,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    Tx { tex, view, w, h, mips: n, fmt, hash: 0 }
}

fn bpp(fmt: wgpu::TextureFormat) -> u32 {
    match fmt {
        wgpu::TextureFormat::R16Float => 2,
        wgpu::TextureFormat::Rgba16Float => 8,
        _ => 4,
    }
}

fn bgl_uniform(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: vis, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None }
}
fn bgl_tex(binding: u32, vis: wgpu::ShaderStages, filterable: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: vis,
        ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
        count: None,
    }
}
fn bgl_samp(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: vis, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None }
}
fn bgl_storage(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only }, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    }
}

fn make_bgl(dev: Dev, label: &str, entries: &[wgpu::BindGroupLayoutEntry]) -> wgpu::BindGroupLayout {
    dev.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some(label), entries })
}

fn shader(dev: Dev, label: &str, src: &str) -> wgpu::ShaderModule {
    dev.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(label), source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Owned(src.to_string())) })
}

const SCENE_WGSL: &str = include_str!("shaders/v_scene.wgsl");
const DEPTH_WGSL: &str = include_str!("shaders/v_depth.wgsl");
const WIRE_WGSL: &str = include_str!("shaders/v_wire.wgsl");
const BG_WGSL: &str = include_str!("shaders/v_bg.wgsl");
const POST_WGSL: &str = include_str!("shaders/v_post.wgsl");
const MIP_WGSL: &str = include_str!("shaders/v_mip.wgsl");
const DRES_WGSL: &str = include_str!("shaders/v_dres.wgsl");
const PACK_WGSL: &str = include_str!("shaders/v_pack.wgsl");
const LOWEST_WGSL: &str = include_str!("shaders/v_lowest.wgsl");

const HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const LDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// the depth picture the post passes read: depth packed into four 8-bit channels
const ZFMT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Bind group layouts and pipelines (built once).
struct Pipes {
    bgl_scene: wgpu::BindGroupLayout,
    bgl_depth: wgpu::BindGroupLayout,
    bgl_wire: wgpu::BindGroupLayout,
    bgl_bg: wgpu::BindGroupLayout,
    bgl_post: wgpu::BindGroupLayout,
    bgl_dres: wgpu::BindGroupLayout,
    bgl_mip: wgpu::BindGroupLayout,
    bgl_pack: wgpu::BindGroupLayout,
    bgl_lowest: wgpu::BindGroupLayout,
    shadow: wgpu::RenderPipeline,
    scene: wgpu::RenderPipeline,
    bg: wgpu::RenderPipeline,
    wire: wgpu::RenderPipeline,
    dres: wgpu::RenderPipeline,
    ssao: wgpu::RenderPipeline,
    dof: wgpu::RenderPipeline,
    bright: wgpu::RenderPipeline,
    fin: wgpu::RenderPipeline,
    mips: Vec<(wgpu::TextureFormat, wgpu::RenderPipeline)>,
    pack: wgpu::ComputePipeline,
    lowest: wgpu::ComputePipeline,
}

fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRS: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3, 3 => Float32x2, 4 => Float32x3, 5 => Float32x3, 6 => Float32];
    wgpu::VertexBufferLayout { array_stride: 18 * 4, step_mode: wgpu::VertexStepMode::Vertex, attributes: &ATTRS }
}

fn build_pipes(dev: Dev, msaa: u32) -> Pipes {
    use wgpu::ShaderStages as S;
    let vf = S::VERTEX | S::FRAGMENT;
    let bgl_scene = make_bgl(
        dev,
        "scene",
        &[
            bgl_uniform(0, vf),
            bgl_samp(1, vf),
            bgl_samp(2, S::FRAGMENT),
            bgl_tex(3, S::FRAGMENT, true),
            bgl_tex(4, S::FRAGMENT, true),
            bgl_tex(5, S::FRAGMENT, true),
            bgl_tex(6, S::FRAGMENT, true),
            bgl_tex(7, vf, true),
            bgl_tex(8, S::FRAGMENT, true),
            wgpu::BindGroupLayoutEntry {
                binding: 9,
                visibility: S::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: wgpu::TextureViewDimension::D2Array, multisampled: false },
                count: None,
            },
            wgpu::BindGroupLayoutEntry { binding: 10, visibility: S::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison), count: None },
        ],
    );
    let bgl_depth = make_bgl(dev, "depth", &[bgl_uniform(0, S::VERTEX), bgl_samp(1, S::VERTEX), bgl_tex(2, S::VERTEX, true)]);
    let bgl_wire = make_bgl(dev, "wire", &[bgl_uniform(0, vf), bgl_samp(1, S::VERTEX), bgl_tex(2, S::VERTEX, true), bgl_tex(3, S::FRAGMENT, false)]);
    let bgl_bg = make_bgl(dev, "bg", &[bgl_uniform(0, S::FRAGMENT), bgl_tex(1, S::FRAGMENT, true), bgl_samp(2, S::FRAGMENT)]);
    let bgl_post = make_bgl(dev, "post", &[bgl_uniform(0, S::FRAGMENT), bgl_tex(1, S::FRAGMENT, true), bgl_tex(2, S::FRAGMENT, false), bgl_tex(3, S::FRAGMENT, true), bgl_samp(4, S::FRAGMENT)]);
    let ms_tex = |binding: u32| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: S::FRAGMENT,
        ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: true },
        count: None,
    };
    let bgl_dres = make_bgl(dev, "dres", &[ms_tex(0), ms_tex(1)]);
    let bgl_mip = make_bgl(dev, "mip", &[bgl_tex(0, S::FRAGMENT, true), bgl_samp(1, S::FRAGMENT)]);
    let bgl_pack = make_bgl(dev, "pack", &[bgl_uniform(0, S::COMPUTE), bgl_storage(1, true), bgl_storage(2, true), bgl_storage(3, true), bgl_storage(4, false)]);
    let bgl_lowest = make_bgl(
        dev,
        "lowest",
        &[
            bgl_uniform(0, S::COMPUTE),
            bgl_storage(1, true),
            wgpu::BindGroupLayoutEntry { binding: 2, visibility: S::COMPUTE, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 3, visibility: S::COMPUTE, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            bgl_storage(4, false),
        ],
    );
    let pl = |bgl: &wgpu::BindGroupLayout| dev.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[bgl], push_constant_ranges: &[] });
    let ms = wgpu::MultisampleState { count: msaa, mask: !0, alpha_to_coverage_enabled: false };
    let ms1 = wgpu::MultisampleState::default();
    let tri_list = wgpu::PrimitiveState::default();
    let depth_state = |write: bool, cmp: wgpu::CompareFunction, bias: wgpu::DepthBiasState| wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: write, depth_compare: cmp, stencil: wgpu::StencilState::default(), bias };
    let no_bias = wgpu::DepthBiasState::default();
    let target = |fmt: wgpu::TextureFormat, blend: Option<wgpu::BlendState>| [Some(wgpu::ColorTargetState { format: fmt, blend, write_mask: wgpu::ColorWrites::ALL })];

    // shadow maps: depth only
    let m_depth = shader(dev, "depth", DEPTH_WGSL);
    let lay = pl(&bgl_depth);
    let shadow = dev.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("shadow"),
        layout: Some(&lay),
        vertex: wgpu::VertexState { module: &m_depth, entry_point: "vs_main", compilation_options: Default::default(), buffers: &[vertex_layout()] },
        primitive: tri_list,
        depth_stencil: Some(depth_state(true, wgpu::CompareFunction::Less, wgpu::DepthBiasState { constant: 2, slope_scale: 1.5, clamp: 0.0 })),
        multisample: ms1,
        fragment: None,
        multiview: None,
        cache: None,
    });
    // scene
    let m_scene = shader(dev, "scene", SCENE_WGSL);
    let lay = pl(&bgl_scene);
    let t = [Some(wgpu::ColorTargetState { format: HDR, blend: None, write_mask: wgpu::ColorWrites::ALL }), Some(wgpu::ColorTargetState { format: ZFMT, blend: None, write_mask: wgpu::ColorWrites::ALL })];
    let scene = dev.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("scene"),
        layout: Some(&lay),
        vertex: wgpu::VertexState { module: &m_scene, entry_point: "vs_main", compilation_options: Default::default(), buffers: &[vertex_layout()] },
        primitive: tri_list,
        depth_stencil: Some(depth_state(true, wgpu::CompareFunction::Less, no_bias)),
        multisample: ms,
        fragment: Some(wgpu::FragmentState { module: &m_scene, entry_point: "fs_main", compilation_options: Default::default(), targets: &t }),
        multiview: None,
        cache: None,
    });
    // background
    let m_bg = shader(dev, "bg", BG_WGSL);
    let lay = pl(&bgl_bg);
    let bg = dev.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("bg"),
        layout: Some(&lay),
        vertex: wgpu::VertexState { module: &m_bg, entry_point: "vs_main", compilation_options: Default::default(), buffers: &[] },
        primitive: tri_list,
        depth_stencil: Some(depth_state(false, wgpu::CompareFunction::Always, no_bias)),
        multisample: ms,
        fragment: Some(wgpu::FragmentState { module: &m_bg, entry_point: "fs_main", compilation_options: Default::default(), targets: &t }),
        multiview: None,
        cache: None,
    });
    // wireframe
    let m_wire = shader(dev, "wire", WIRE_WGSL);
    let lay = pl(&bgl_wire);
    let blend = wgpu::BlendState {
        color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
        alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
    };
    let t_ldr_blend = target(LDR, Some(blend));
    let wire = dev.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("wire"),
        layout: Some(&lay),
        vertex: wgpu::VertexState { module: &m_wire, entry_point: "vs_main", compilation_options: Default::default(), buffers: &[vertex_layout()] },
        primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::LineList, ..Default::default() },
        depth_stencil: None,
        multisample: ms1,
        fragment: Some(wgpu::FragmentState { module: &m_wire, entry_point: "fs_main", compilation_options: Default::default(), targets: &t_ldr_blend }),
        multiview: None,
        cache: None,
    });
    // multisample resolve (only used with multisampling)
    let m_dres = shader(dev, "dres", DRES_WGSL);
    let lay = pl(&bgl_dres);
    let t_two = [Some(wgpu::ColorTargetState { format: HDR, blend: None, write_mask: wgpu::ColorWrites::ALL }), Some(wgpu::ColorTargetState { format: ZFMT, blend: None, write_mask: wgpu::ColorWrites::ALL })];
    let dres = dev.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("dres"),
        layout: Some(&lay),
        vertex: wgpu::VertexState { module: &m_dres, entry_point: "vs_main", compilation_options: Default::default(), buffers: &[] },
        primitive: tri_list,
        depth_stencil: None,
        multisample: ms1,
        fragment: Some(wgpu::FragmentState { module: &m_dres, entry_point: "fs_main", compilation_options: Default::default(), targets: &t_two }),
        multiview: None,
        cache: None,
    });
    // post
    let m_post = shader(dev, "post", POST_WGSL);
    let lay = pl(&bgl_post);
    let post = |entry: &'static str, fmt: wgpu::TextureFormat| {
        let t = target(fmt, None);
        dev.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(entry),
            layout: Some(&lay),
            vertex: wgpu::VertexState { module: &m_post, entry_point: "vs_main", compilation_options: Default::default(), buffers: &[] },
            primitive: tri_list,
            depth_stencil: None,
            multisample: ms1,
            fragment: Some(wgpu::FragmentState { module: &m_post, entry_point: entry, compilation_options: Default::default(), targets: &t }),
            multiview: None,
            cache: None,
        })
    };
    let ssao = post("fs_ssao", HDR);
    let dof = post("fs_dof", HDR);
    let bright = post("fs_bright", HDR);
    let fin = post("fs_final", LDR);
    // mip chains
    let m_mip = shader(dev, "mip", MIP_WGSL);
    let lay = pl(&bgl_mip);
    let mips = [wgpu::TextureFormat::Rgba8UnormSrgb, wgpu::TextureFormat::Rgba8Unorm, wgpu::TextureFormat::R16Float, HDR]
        .into_iter()
        .map(|fmt| {
            let t = target(fmt, None);
            let p = dev.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("mip"),
                layout: Some(&lay),
                vertex: wgpu::VertexState { module: &m_mip, entry_point: "vs_main", compilation_options: Default::default(), buffers: &[] },
                primitive: tri_list,
                depth_stencil: None,
                multisample: ms1,
                fragment: Some(wgpu::FragmentState { module: &m_mip, entry_point: "fs_main", compilation_options: Default::default(), targets: &t }),
                multiview: None,
                cache: None,
            });
            (fmt, p)
        })
        .collect();
    // compute
    let m_pack = shader(dev, "viewer pack", PACK_WGSL);
    let lay = pl(&bgl_pack);
    let pack = dev.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some("viewer pack"), layout: Some(&lay), module: &m_pack, entry_point: "main", compilation_options: Default::default(), cache: None });
    let m_low = shader(dev, "viewer lowest", LOWEST_WGSL);
    let lay = pl(&bgl_lowest);
    let lowest = dev.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some("viewer lowest"), layout: Some(&lay), module: &m_low, entry_point: "main", compilation_options: Default::default(), cache: None });
    Pipes { bgl_scene, bgl_depth, bgl_wire, bgl_bg, bgl_post, bgl_dres, bgl_mip, bgl_pack, bgl_lowest, shadow, scene, bg, wire, dres, ssao, dof, bright, fin, mips, pack, lowest }
}

struct EnvTex {
    key: String,
    tx: Tx,
    levels: usize,
    sh: Vec<[f32; 4]>,
}

struct Shadows {
    /// all layers, for sampling
    array: wgpu::TextureView,
    /// one view per layer, for rendering
    layers: Vec<wgpu::TextureView>,
}

/// Render targets of one viewport size.
struct Targets {
    w: u32,
    h: u32,
    ms_color: Option<wgpu::TextureView>,
    /// multisampled depth picture (only with multisampling)
    ms_z: Option<wgpu::TextureView>,
    depth: wgpu::TextureView,
    hdr: wgpu::TextureView,
    zres: wgpu::TextureView,
    ping: [wgpu::TextureView; 2],
    bright: wgpu::Texture,
    bright_levels: u32,
    out: wgpu::Texture,
    out_view: wgpu::TextureView,
    staging: wgpu::Buffer,
    row_bytes: u32,
}

struct MeshW {
    key: MeshKey,
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    index_count: u32,
    /// line list of the wireframe, built when it is first wanted
    wire: Option<(wgpu::Buffer, u32)>,
    tris: Vec<[u32; 3]>,
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
    dev: Dev,
    q: Queue,
    pipes: Pipes,
    msaa: u32,
    lin_clamp: wgpu::Sampler,
    lin_repeat: wgpu::Sampler,
    env_samp: wgpu::Sampler,
    maps_samp: wgpu::Sampler,
    shadow_samp: wgpu::Sampler,
    dummy_buf: wgpu::Buffer,
    dummy_rgba: Tx,
    dummy_h: Tx,
    dummy_shadow: wgpu::TextureView,
    base: Option<Tx>,
    normal: Option<Tx>,
    rma: Option<Tx>,
    emis: Option<Tx>,
    height: Option<Tx>,
    mesh: Option<MeshW>,
    targets: Option<Targets>,
    env: Option<EnvTex>,
    shadows: Option<Shadows>,
    shadow_u: Vec<wgpu::Buffer>,
    scene_u: wgpu::Buffer,
    bg_u: wgpu::Buffer,
    post_u: wgpu::Buffer,
    wire_u: wgpu::Buffer,
    lowest_cache: std::cell::RefCell<Option<((u64, u32, u32, usize, u32), f32)>>,
}

#[derive(Default)]
pub struct Gpu {
    pub opts: GpuOpts,
    /// Set when a device call failed; the UI then falls back to the CPU renderer.
    pub error: Option<String>,
    /// Maps waiting to be put into textures on the next frame.
    pending: Option<GpuMaps>,
    height_gen: u64,
    /// Request the next frame as a picture in `shot` (screenshots, tests).
    pub want_shot: bool,
    /// Result: (width, height, RGBA8 rows from the TOP).
    pub shot: Option<(usize, usize, Vec<u8>)>,
    /// GPU name, for the UI.
    pub renderer: String,
    /// Why the chosen environment file could not be used (the studio is shown instead).
    pub env_error: Option<String>,
    res: Option<Res>,
    /// The sampling options the sampler carries (it is made again when the options change).
    applied: Option<GpuOpts>,
    /// frames drawn
    pub frames: u64,
    /// how often maps were put into textures (a camera or light move never does)
    pub pack_count: u64,
    /// how often the mesh was built
    pub mesh_builds: u64,
    /// generation of the maps: counts up with every `set_maps`
    map_gen: u64,
    last: Option<(String, Frame)>,
    seq: u64,
}

/// One finished picture of the 3D view.
#[derive(Clone)]
pub struct Frame {
    pub w: usize,
    pub h: usize,
    /// RGBA8, rows from the top
    pub rgba: Arc<Vec<u8>>,
    /// grows with every new picture: a picture with the same `seq` as the one shown is the same picture
    pub seq: u64,
}

impl Gpu {
    pub fn set_maps(&mut self, maps: GpuMaps) {
        self.height_gen += 1;
        self.map_gen += 1;
        self.pending = Some(maps);
    }
    pub fn ready(&self) -> bool {
        self.error.is_none()
    }
}

fn sampler_for(dev: Dev, o: &GpuOpts) -> wgpu::Sampler {
    let (min, mag, mip) = match o.filter {
        0 => (wgpu::FilterMode::Nearest, wgpu::FilterMode::Nearest, wgpu::FilterMode::Nearest),
        1 => (wgpu::FilterMode::Linear, wgpu::FilterMode::Linear, wgpu::FilterMode::Nearest),
        _ => (wgpu::FilterMode::Linear, wgpu::FilterMode::Linear, wgpu::FilterMode::Linear),
    };
    let wants_aniso = o.filter >= 1 && o.mipmaps;
    let aniso = if wants_aniso { (if o.aniso <= 0.0 { 16.0 } else { o.aniso.clamp(1.0, 16.0) }) as u16 } else { 1 };
    // anisotropic filtering blends between mip levels itself
    let (min, mag, mip) = if aniso > 1 { (wgpu::FilterMode::Linear, wgpu::FilterMode::Linear, wgpu::FilterMode::Linear) } else { (min, mag, mip) };
    let mode = match o.wrap {
        1 => wgpu::AddressMode::MirrorRepeat,
        2 => wgpu::AddressMode::ClampToEdge,
        _ => wgpu::AddressMode::Repeat,
    };
    dev.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("maps"),
        address_mode_u: mode,
        address_mode_v: mode,
        address_mode_w: mode,
        mag_filter: mag,
        min_filter: min,
        mipmap_filter: mip,
        lod_min_clamp: 0.0,
        lod_max_clamp: if o.mipmaps { 32.0 } else { 0.0 },
        compare: None,
        anisotropy_clamp: aniso,
        border_color: None,
    })
}

fn plain_sampler(dev: Dev, mode_u: wgpu::AddressMode, mode_v: wgpu::AddressMode, label: &str) -> wgpu::Sampler {
    dev.create_sampler(&wgpu::SamplerDescriptor {
        label: Some(label),
        address_mode_u: mode_u,
        address_mode_v: mode_v,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    })
}

fn uni_buf(dev: Dev, size: u64, label: &str) -> wgpu::Buffer {
    dev.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
}

fn create_res(dev: Dev, q: Queue) -> Result<Res, String> {
    // 4x multisampling when the adapter does it for the formats used here, plain otherwise
    let msaa = {
        let test = |fmt: wgpu::TextureFormat| {
            guard(dev, || {
                dev.create_texture(&wgpu::TextureDescriptor {
                    label: Some("msaa probe"),
                    size: wgpu::Extent3d { width: 4, height: 4, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 4,
                    dimension: wgpu::TextureDimension::D2,
                    format: fmt,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
            })
            .is_ok()
        };
        // the GL backend of wgpu cannot read multisampled textures back, which the resolve pass needs
        let gl = crate::gpu::adapter_name().map(|n| n.contains("(Gl)")).unwrap_or(false);
        if !gl && std::env::var("MC_NOMSAA").is_err() && test(HDR) && test(DEPTH) {
            4
        } else {
            1
        }
    };
    let pipes = guard(dev, || build_pipes(dev, msaa))?;
    let dummy_rgba = make_tx(dev, 1, 1, wgpu::TextureFormat::Rgba8Unorm, false, "dummy rgba");
    let dummy_h = make_tx(dev, 1, 1, wgpu::TextureFormat::R16Float, false, "dummy height");
    let dummy_shadow_tex = dev.create_texture(&wgpu::TextureDescriptor {
        label: Some("dummy shadow"),
        size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let dummy_shadow = dummy_shadow_tex.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
    let dummy_buf = dev.create_buffer(&wgpu::BufferDescriptor { label: Some("dummy"), size: 16, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    let res = Res {
        dev,
        q,
        pipes,
        msaa,
        lin_clamp: plain_sampler(dev, wgpu::AddressMode::ClampToEdge, wgpu::AddressMode::ClampToEdge, "linear clamp"),
        lin_repeat: plain_sampler(dev, wgpu::AddressMode::Repeat, wgpu::AddressMode::Repeat, "linear repeat"),
        env_samp: plain_sampler(dev, wgpu::AddressMode::Repeat, wgpu::AddressMode::ClampToEdge, "environment"),
        maps_samp: sampler_for(dev, &GpuOpts::default()),
        shadow_samp: dev.create_sampler(&wgpu::SamplerDescriptor { label: Some("shadow compare"), address_mode_u: wgpu::AddressMode::ClampToEdge, address_mode_v: wgpu::AddressMode::ClampToEdge, mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, compare: Some(wgpu::CompareFunction::LessEqual), ..Default::default() }),
        dummy_buf,
        dummy_rgba,
        dummy_h,
        dummy_shadow,
        base: None,
        normal: None,
        rma: None,
        emis: None,
        height: None,
        mesh: None,
        targets: None,
        env: None,
        shadows: None,
        shadow_u: (0..SHADOW_LAYERS).map(|_| uni_buf(dev, 208, "shadow uniforms")).collect(),
        scene_u: uni_buf(dev, 1040, "scene uniforms"),
        bg_u: uni_buf(dev, 112, "background uniforms"),
        post_u: uni_buf(dev, 128, "post uniforms"),
        wire_u: uni_buf(dev, 224, "wire uniforms"),
        lowest_cache: std::cell::RefCell::new(None),
    };
    // the dummies hold neutral values
    q.write_texture(
        wgpu::ImageCopyTexture { texture: &res.dummy_rgba.tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        &[0u8, 0, 0, 255],
        wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(4), rows_per_image: Some(1) },
        wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
    );
    q.write_texture(
        wgpu::ImageCopyTexture { texture: &res.dummy_h.tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        &[0u8, 0],
        wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(2), rows_per_image: Some(1) },
        wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
    );
    // clear the 1x1 shadow stand-in to "far"
    let mut enc = dev.create_command_encoder(&Default::default());
    {
        let _pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("dummy shadow clear"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &dummy_shadow_tex.create_view(&Default::default()),
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
    }
    q.submit(Some(enc.finish()));
    Ok(res)
}

// ------------------------------------------------------------------------------------------------ maps
/// Fills the levels below 0 of `tx` by averaging 2x2 blocks, on the device.
fn gen_mips(r: &Res, enc: &mut wgpu::CommandEncoder, tex: &wgpu::Texture, fmt: wgpu::TextureFormat, mips: u32) {
    let Some((_, pipe)) = r.pipes.mips.iter().find(|(f, _)| *f == fmt) else { return };
    for l in 1..mips {
        let src = tex.create_view(&wgpu::TextureViewDescriptor { base_mip_level: l - 1, mip_level_count: Some(1), ..Default::default() });
        let dst = tex.create_view(&wgpu::TextureViewDescriptor { base_mip_level: l, mip_level_count: Some(1), ..Default::default() });
        let bg = r.dev.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mip"),
            layout: &r.pipes.bgl_mip,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&src) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&r.lin_clamp) }],
        });
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("mip"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &dst, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(pipe);
        pass.set_bind_group(0, &bg, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// Where a map comes from.
enum Src<'a> {
    /// packed bytes from the host code (4 per pixel, 2 for the height)
    Bytes(&'a [u8]),
    /// packed on the device from graph pictures
    Pack(PackJob<'a>),
}

fn pack_src<'a>(mode: u32, s: [Option<&'a GTex>; 3], flip_g: bool, inv_r: bool) -> Src<'a> {
    Src::Pack(PackJob { mode, s, flip_g, inv_r })
}

#[derive(Clone, Copy)]
struct PackJob<'a> {
    mode: u32,
    s: [Option<&'a GTex>; 3],
    flip_g: bool,
    inv_r: bool,
}

/// Packs graph pictures into a padded buffer (rows of 256 bytes' multiples) on the device.
fn pack_run(r: &Res, enc: &mut wgpu::CommandEncoder, job: &PackJob, w: u32, h: u32) -> (wgpu::Buffer, u32) {
    let words_per_row = if job.mode == 2 { w.div_ceil(2) } else { w };
    let stride = align_up(words_per_row, 64);
    let size = stride as u64 * h as u64 * 4;
    let dst = r.dev.create_buffer(&wgpu::BufferDescriptor { label: Some("packed map"), size, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
    let mut ub = Ub::default();
    let mut has = 0u32;
    for (i, s) in job.s.iter().enumerate() {
        if s.is_some() {
            has |= 1 << i;
        }
    }
    for v in [w, h, stride, job.mode, job.flip_g as u32, has, job.inv_r as u32, 0] {
        ub.u(v);
    }
    ub.v4([0.5, 0.0, 1.0, 0.0]);
    let uni = uni_buf(r.dev, 48, "pack uniforms");
    r.q.write_buffer(&uni, 0, &ub.0);
    let bind = |i: usize| -> &wgpu::Buffer { job.s[i].map(|t| t.buffer()).unwrap_or(&r.dummy_buf) };
    let bg = r.dev.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("pack"),
        layout: &r.pipes.bgl_pack,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: uni.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: bind(0).as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: bind(1).as_entire_binding() },
            wgpu::BindGroupEntry { binding: 3, resource: bind(2).as_entire_binding() },
            wgpu::BindGroupEntry { binding: 4, resource: dst.as_entire_binding() },
        ],
    });
    {
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("pack"), timestamp_writes: None });
        pass.set_pipeline(&r.pipes.pack);
        pass.set_bind_group(0, &bg, &[]);
        pass.dispatch_workgroups(stride.div_ceil(8), h.div_ceil(8), 1);
    }
    (dst, stride * 4)
}

/// Puts a picture into a texture slot. The texture already there is kept when it holds the same picture, refilled in
/// place when only the picture changed, and replaced when its size or format changed. Returns true when it was filled.
fn put_tx(r: &Res, enc: &mut wgpu::CommandEncoder, slot: &mut Option<Tx>, fmt: wgpu::TextureFormat, w: u32, h: u32, src: Src, hash: u64) -> bool {
    if let Some(t) = slot.as_ref() {
        if t.w == w && t.h == h && t.fmt == fmt && t.hash != 0 && t.hash == hash {
            return false;
        }
    }
    let reuse = slot.as_ref().map(|t| t.w == w && t.h == h && t.fmt == fmt).unwrap_or(false);
    if !reuse {
        *slot = Some(make_tx(r.dev, w, h, fmt, true, "map"));
    }
    let t = slot.as_mut().unwrap();
    let extent = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
    match src {
        Src::Bytes(bytes) => {
            r.q.write_texture(
                wgpu::ImageCopyTexture { texture: &t.tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                bytes,
                wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(w * bpp(fmt)), rows_per_image: Some(h) },
                extent,
            );
        }
        Src::Pack(job) => {
            let (buf, row_bytes) = pack_run(r, enc, &job, w, h);
            enc.copy_buffer_to_texture(
                wgpu::ImageCopyBuffer { buffer: &buf, layout: wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(row_bytes), rows_per_image: Some(h) } },
                wgpu::ImageCopyTexture { texture: &t.tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                extent,
            );
        }
    }
    gen_mips(r, enc, &t.tex, fmt, t.mips);
    t.hash = hash;
    true
}

/// Makes the textures hold `maps`. Only maps that differ from what the textures hold are packed again.
fn upload_maps(r: &mut Res, maps: &GpuMaps) -> u64 {
    let s = maps.size as u32;
    let mut enc = r.dev.create_command_encoder(&Default::default());
    let mut filled = 0u64;
    // the slots are moved out so `r` can be borrowed shared while they are filled
    let (mut base, mut normal, mut rma) = (r.base.take(), r.normal.take(), r.rma.take());
    let (mut emis, mut height) = (r.emis.take(), r.height.take());
    {
        let this: &Res = r;
        let srgb = wgpu::TextureFormat::Rgba8UnormSrgb;
        let unorm = wgpu::TextureFormat::Rgba8Unorm;
        let half = wgpu::TextureFormat::R16Float;
        match &maps.dev {
            Some(d) => {
                let job = pack_src;
                filled += put_tx(this, &mut enc, &mut base, srgb, s, s, job(0, [d.base.as_ref(), None, None], false, false), maps.hashes[0]) as u64;
                filled += put_tx(this, &mut enc, &mut normal, unorm, s, s, job(3, [d.normal.as_ref(), None, None], d.flip_normal_g, false), maps.hashes[1]) as u64;
                filled += put_tx(this, &mut enc, &mut rma, unorm, s, s, job(1, [d.rough.as_ref(), d.metal.as_ref(), d.ao.as_ref()], false, d.rough_invert), maps.hashes[2]) as u64;
                match &d.emissive {
                    Some(e) => filled += put_tx(this, &mut enc, &mut emis, srgb, s, s, job(0, [Some(e), None, None], false, false), maps.hashes[3]) as u64,
                    None => emis = None,
                }
                match &d.height {
                    Some(hm) => filled += put_tx(this, &mut enc, &mut height, half, hm.w() as u32, hm.h() as u32, job(2, [Some(hm), None, None], false, false), maps.hashes[4]) as u64,
                    None => height = None,
                }
            }
            None => {
                filled += put_tx(this, &mut enc, &mut base, srgb, s, s, Src::Bytes(&maps.base), maps.hashes[0]) as u64;
                filled += put_tx(this, &mut enc, &mut normal, unorm, s, s, Src::Bytes(&maps.normal), maps.hashes[1]) as u64;
                filled += put_tx(this, &mut enc, &mut rma, unorm, s, s, Src::Bytes(&maps.rma), maps.hashes[2]) as u64;
                match &maps.emissive {
                    Some(e) => filled += put_tx(this, &mut enc, &mut emis, srgb, s, s, Src::Bytes(e), maps.hashes[3]) as u64,
                    None => emis = None,
                }
                match (&maps.height, &maps.height_half) {
                    (Some((w, h, _)), Some(v)) => filled += put_tx(this, &mut enc, &mut height, half, *w as u32, *h as u32, Src::Bytes(bytes_of(&v[..])), maps.hashes[4]) as u64,
                    _ => height = None,
                }
            }
        }
    }
    r.base = base;
    r.normal = normal;
    r.rma = rma;
    r.emis = emis;
    r.height = height;
    r.q.submit(Some(enc.finish()));
    filled
}

/// Test helper: runs the viewer's packing kernels on the device maps of `maps` and reads the packed bytes back in the
/// layout of [`prepare`]. `None` when `maps` does not hold device maps or the device refuses.
pub fn read_packed(maps: &GpuMaps) -> Option<GpuMaps> {
    let d = maps.dev.as_ref()?;
    let (dev, q) = crate::gpu::viewer_parts()?;
    let r = create_res(dev, q).ok()?;
    let s = maps.size as u32;
    let mut enc = dev.create_command_encoder(&Default::default());
    let mk = |enc: &mut wgpu::CommandEncoder, mode, srcs: [Option<&GTex>; 3], flip_g, inv_r, w: u32, h: u32| {
        let job = PackJob { mode, s: srcs, flip_g, inv_r };
        let (buf, row) = pack_run(&r, enc, &job, w, h);
        (buf, row, w, h)
    };
    let mut jobs: Vec<(Option<(wgpu::Buffer, u32, u32, u32)>, u32)> = Vec::new();
    jobs.push((Some(mk(&mut enc, 0, [d.base.as_ref(), None, None], false, false, s, s)), 4));
    jobs.push((Some(mk(&mut enc, 3, [d.normal.as_ref(), None, None], d.flip_normal_g, false, s, s)), 4));
    jobs.push((Some(mk(&mut enc, 1, [d.rough.as_ref(), d.metal.as_ref(), d.ao.as_ref()], false, d.rough_invert, s, s)), 4));
    jobs.push((d.emissive.as_ref().map(|e| mk(&mut enc, 0, [Some(e), None, None], false, false, s, s)), 4));
    jobs.push((d.height.as_ref().map(|hm| mk(&mut enc, 2, [Some(hm), None, None], false, false, hm.w() as u32, hm.h() as u32)), 2));
    // copy every buffer to a mappable twin
    let mut stagings = Vec::new();
    for (j, _) in &jobs {
        stagings.push(j.as_ref().map(|(buf, ..)| {
            let st = dev.create_buffer(&wgpu::BufferDescriptor { label: Some("read back"), size: buf.size(), usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            enc.copy_buffer_to_buffer(buf, 0, &st, 0, buf.size());
            st
        }));
    }
    q.submit(Some(enc.finish()));
    let mut out: Vec<Option<Vec<u8>>> = Vec::new();
    for ((j, bytes), st) in jobs.iter().zip(&stagings) {
        out.push(match (j, st) {
            (Some((_, row, w, h)), Some(st)) => {
                let (tx, rx) = std::sync::mpsc::channel();
                st.slice(..).map_async(wgpu::MapMode::Read, move |res| {
                    let _ = tx.send(res);
                });
                dev.poll(wgpu::Maintain::Wait);
                rx.recv().ok()?.ok()?;
                let data = st.slice(..).get_mapped_range();
                let mut v = Vec::with_capacity((*w * *h * bytes) as usize);
                for y in 0..*h {
                    let o = (y * row) as usize;
                    v.extend_from_slice(&data[o..o + (*w * bytes) as usize]);
                }
                drop(data);
                st.unmap();
                Some(v)
            }
            _ => None,
        });
    }
    let mut it = out.into_iter();
    let (base, normal, rma, emissive, half) = (it.next()?, it.next()?, it.next()?, it.next()?, it.next()?);
    let height_half: Option<Vec<u16>> = half.map(|b| b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect());
    let height = match (&d.height, &height_half) {
        (Some(hm), Some(hh)) => Some((hm.w(), hm.h(), Arc::new(hh.iter().map(|x| half_to_f32(*x)).collect::<Vec<f32>>()))),
        _ => None,
    };
    Some(GpuMaps { size: maps.size, base: base?, normal: normal?, rma: rma?, emissive, height, height_half, hashes: maps.hashes, dev: None })
}

/// 16-bit float to 32-bit float (exact).
fn half_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) & 1) as u32;
    let exp = ((h >> 10) & 0x1f) as u32;
    let man = (h & 0x3ff) as u32;
    let bits = if exp == 0 {
        if man == 0 {
            sign << 31
        } else {
            let mut e = 127 - 15 + 1;
            let mut m = man;
            while m & 0x400 == 0 {
                m <<= 1;
                e -= 1;
            }
            (sign << 31) | ((e as u32) << 23) | ((m & 0x3ff) << 13)
        }
    } else if exp == 0x1f {
        (sign << 31) | 0x7f80_0000 | (man << 13)
    } else {
        (sign << 31) | ((exp + 127 - 15) << 23) | (man << 13)
    };
    f32::from_bits(bits)
}

// ------------------------------------------------------------------------------------------------ environment
fn upload_env(r: &Res, env: &Env, key: String) -> EnvTex {
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
    let tex = r.dev.create_texture(&wgpu::TextureDescriptor {
        label: Some("environment"),
        size: wgpu::Extent3d { width: w0 as u32, height: h0 as u32, depth_or_array_layers: 1 },
        mip_level_count: levels as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: HDR,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (k, l) in env.levels.iter().take(levels).enumerate() {
        let halves: Vec<u16> = l.px.iter().flat_map(|p| [f32_to_f16(p[0].min(60000.0)), f32_to_f16(p[1].min(60000.0)), f32_to_f16(p[2].min(60000.0)), f32_to_f16(1.0)]).collect();
        r.q.write_texture(
            wgpu::ImageCopyTexture { texture: &tex, mip_level: k as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            bytes_of(&halves[..]),
            wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(l.w as u32 * 8), rows_per_image: Some(l.h as u32) },
            wgpu::Extent3d { width: l.w as u32, height: l.h as u32, depth_or_array_layers: 1 },
        );
    }
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let sh: Vec<[f32; 4]> = env.sh.iter().map(|c| [c[0], c[1], c[2], 0.0]).collect();
    EnvTex { key, tx: Tx { tex, view, w: w0 as u32, h: h0 as u32, mips: levels as u32, fmt: HDR, hash: 1 }, levels, sh }
}

// ------------------------------------------------------------------------------------------------ mesh
/// Lowest point of the displaced object, found on the device where the height map is.
fn lowest_gpu(r: &Res, bottom: &[(f32, [f32; 2], f32, f32)], ymin0: f32, scale: f32, level: f32, height_gen: u64) -> f32 {
    let Some(h) = r.height.as_ref() else { return ymin0 };
    if bottom.is_empty() {
        return ymin0;
    }
    let key = (height_gen, scale.to_bits(), level.to_bits(), bottom.len(), ymin0.to_bits());
    if let Some((k, v)) = r.lowest_cache.borrow().as_ref() {
        if *k == key {
            return *v;
        }
    }
    let verts: Vec<[f32; 4]> = bottom.iter().map(|&(y, uv, dy, fade)| [y, uv[0], uv[1], dy * fade]).collect();
    let vbuf = r.dev.create_buffer(&wgpu::BufferDescriptor { label: Some("bottom vertices"), size: (verts.len() * 16) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    r.q.write_buffer(&vbuf, 0, bytes_of(&verts[..]));
    let out = r.dev.create_buffer(&wgpu::BufferDescriptor { label: Some("lowest"), size: 4, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
    let stage = r.dev.create_buffer(&wgpu::BufferDescriptor { label: Some("lowest read"), size: 4, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    let mut ub = Ub::default();
    ub.u(verts.len() as u32);
    ub.f(scale);
    ub.f(level);
    ub.f(ymin0);
    let uni = uni_buf(r.dev, 16, "lowest uniforms");
    r.q.write_buffer(&uni, 0, &ub.0);
    let bg = r.dev.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("lowest"),
        layout: &r.pipes.bgl_lowest,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: uni.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: vbuf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&h.view) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&r.lin_repeat) },
            wgpu::BindGroupEntry { binding: 4, resource: out.as_entire_binding() },
        ],
    });
    let mut enc = r.dev.create_command_encoder(&Default::default());
    {
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("lowest"), timestamp_writes: None });
        pass.set_pipeline(&r.pipes.lowest);
        pass.set_bind_group(0, &bg, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
    enc.copy_buffer_to_buffer(&out, 0, &stage, 0, 4);
    r.q.submit(Some(enc.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    stage.slice(..).map_async(wgpu::MapMode::Read, move |res| {
        let _ = tx.send(res);
    });
    r.dev.poll(wgpu::Maintain::Wait);
    let y = match rx.recv() {
        Ok(Ok(())) => {
            let d = stage.slice(..).get_mapped_range();
            let v = f32::from_le_bytes([d[0], d[1], d[2], d[3]]);
            drop(d);
            stage.unmap();
            if v.is_finite() {
                v
            } else {
                ymin0
            }
        }
        _ => ymin0,
    };
    *r.lowest_cache.borrow_mut() = Some((key, y));
    y
}

/// Builds (or keeps) the scene mesh for `v`. Returns true when the mesh was built again.
fn upload_mesh(r: &mut Res, v: &View, height_gen: u64) -> bool {
    let has_height = r.height.is_some();
    let displacing = v.relief == RELIEF_DISPLACE && v.height_scale > 0.0 && has_height;
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
    // the floor follows the displaced object in steps of 1/2048 (a smaller move is invisible and not worth a rebuild)
    let quant = |y: f32| (y * 2048.0).round() as i64;
    if moves_floor {
        if let Some(old) = r.mesh.as_ref() {
            let same_mesh = MeshKey { floor: old.key.floor, ..key.clone() } == old.key;
            if same_mesh {
                let y = lowest_gpu(r, &old.bottom, old.ymin0, v.height_scale, v.height_level, height_gen);
                key.floor = if (y - old.ymin0).abs() > 1e-5 { quant(y) } else { 0 };
                if key.floor == old.key.floor {
                    return false;
                }
            }
        }
    }
    if r.mesh.as_ref().map(|m| m.key == key).unwrap_or(false) {
        return false;
    }
    let mut sm = objects::build_scene_ex(v, None, None);
    let (mut bottom, mut ymin0) = (Vec::new(), 0.0f32);
    if moves_floor {
        // the vertex shader displaces, so work out the lowest point of the displaced object here and put the floor
        // under it, like the software renderer does
        let m = &sm.mesh;
        let n0 = m.obj.iter().position(|&o| o != 0).map(|t| m.tris[t][0] as usize).unwrap_or(m.pos.len());
        ymin0 = (0..n0).map(|i| m.pos[i][1]).fold(f32::MAX, f32::min);
        bottom = (0..n0).filter(|&i| m.pos[i][1] < ymin0 + v.height_scale.abs() * 1.05 + 1e-4).map(|i| (m.pos[i][1], m.uv[i], m.disp[i][1], m.fade[i])).collect();
        let ymin = lowest_gpu(r, &bottom, ymin0, v.height_scale, v.height_level, height_gen);
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
    let vbytes = bytes_of(&data[..]);
    let vbuf = r.dev.create_buffer(&wgpu::BufferDescriptor { label: Some("scene vertices"), size: vbytes.len().max(4) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    r.q.write_buffer(&vbuf, 0, vbytes);
    let ibytes = bytes_of(&m.tris[..]);
    let ibuf = r.dev.create_buffer(&wgpu::BufferDescriptor { label: Some("scene indices"), size: ibytes.len().max(4) as u64, usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    r.q.write_buffer(&ibuf, 0, ibytes);
    r.mesh = Some(MeshW { key, vbuf, ibuf, index_count: (m.tris.len() * 3) as u32, wire: None, tris: m.tris.clone(), objects: sm.objects, radius: radius + displace_pad, bottom, ymin0 });
    true
}

/// The edges of the mesh as a line list (each edge once).
fn build_wire(r: &mut Res) {
    let Some(m) = r.mesh.as_mut() else { return };
    if m.wire.is_some() {
        return;
    }
    let mut seen = std::collections::HashSet::new();
    let mut lines: Vec<u32> = Vec::new();
    for t in &m.tris {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            if seen.insert((a.min(b), a.max(b))) {
                lines.push(a);
                lines.push(b);
            }
        }
    }
    let bytes = bytes_of(&lines[..]);
    let buf = r.dev.create_buffer(&wgpu::BufferDescriptor { label: Some("wireframe"), size: bytes.len().max(4) as u64, usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    r.q.write_buffer(&buf, 0, bytes);
    m.wire = Some((buf, lines.len() as u32));
}

// ------------------------------------------------------------------------------------------------ frame
struct LightGl {
    axis: Vec<[f32; 4]>,
    info: Vec<[f32; 4]>,
    col: Vec<[f32; 4]>,
    /// view-projection of every shadow layer
    vps: Vec<M4>,
}

fn light_uniforms(v: &View, radius: f32, shadows_on: bool) -> LightGl {
    let cf = v.cam_frame();
    let mut o = LightGl { axis: vec![], info: vec![], col: vec![], vps: vec![] };
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
        o.axis.push([axis[0], axis[1], axis[2], dist]);
        o.info.push([kind, cos_out, cos_in, l.softness]);
        let mut layer = -1.0;
        if shadows_on && l.shadows && o.vps.len() < SHADOW_LAYERS as usize {
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
            layer = o.vps.len() as f32;
            o.vps.push(vp);
        }
        o.col.push([l.color[0] * l.intensity, l.color[1] * l.intensity, l.color[2] * l.intensity, layer]);
    }
    o
}

fn make_targets(r: &Res, w: u32, h: u32) -> Targets {
    let dev = r.dev;
    let size = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
    let tex = |label: &str, fmt: wgpu::TextureFormat, samples: u32, mips: u32, usage: wgpu::TextureUsages| {
        dev.create_texture(&wgpu::TextureDescriptor { label: Some(label), size, mip_level_count: mips, sample_count: samples, dimension: wgpu::TextureDimension::D2, format: fmt, usage, view_formats: &[] })
    };
    let ra = wgpu::TextureUsages::RENDER_ATTACHMENT;
    let tb = wgpu::TextureUsages::TEXTURE_BINDING;
    let view = |t: &wgpu::Texture| t.create_view(&wgpu::TextureViewDescriptor::default());
    let hdr = view(&tex("hdr", HDR, 1, 1, ra | tb));
    let ms_color = if r.msaa > 1 { Some(view(&tex("hdr ms", HDR, r.msaa, 1, ra | tb))) } else { None };
    let ms_z = if r.msaa > 1 { Some(view(&tex("depth picture ms", ZFMT, r.msaa, 1, ra | tb))) } else { None };
    let depth = view(&tex("depth", DEPTH, r.msaa, 1, ra));
    let zres = view(&tex("depth resolved", ZFMT, 1, 1, ra | tb));
    let ping = [view(&tex("ping 0", HDR, 1, 1, ra | tb)), view(&tex("ping 1", HDR, 1, 1, ra | tb))];
    let bright_levels = mip_count(w, h);
    let bright = tex("bright", HDR, 1, bright_levels, ra | tb);
    let out = tex("frame", LDR, 1, 1, ra | wgpu::TextureUsages::COPY_SRC);
    let out_view = view(&out);
    let row_bytes = align_up(w * 4, 256);
    let staging = dev.create_buffer(&wgpu::BufferDescriptor { label: Some("frame read back"), size: row_bytes as u64 * h as u64, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    Targets { w, h, ms_color, ms_z, depth, hdr, zres, ping, bright, bright_levels, out, out_view, staging, row_bytes }
}

fn make_shadows(dev: Dev) -> Shadows {
    let tex = dev.create_texture(&wgpu::TextureDescriptor {
        label: Some("shadow maps"),
        size: wgpu::Extent3d { width: SHADOW_SIZE, height: SHADOW_SIZE, depth_or_array_layers: SHADOW_LAYERS },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let array = tex.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
    let layers = (0..SHADOW_LAYERS)
        .map(|l| tex.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2), base_array_layer: l, array_layer_count: Some(1), ..Default::default() }))
        .collect();
    Shadows { array, layers }
}

fn clear_color() -> wgpu::LoadOp<wgpu::Color> {
    wgpu::LoadOp::Clear(wgpu::Color::BLACK)
}

/// Draws one picture of the scene: shadow maps, the scene into a multisampled float target, the post chain and the
/// wireframe. Returns the RGBA8 rows (top row first).
#[allow(clippy::too_many_arguments)]
fn render(r: &mut Res, opts: &GpuOpts, view: &View, w: u32, h: u32, height_gen: u64, builds: &mut u64) -> Result<Vec<u8>, String> {
    if upload_mesh(r, view, height_gen) {
        *builds += 1;
    }
    if view.wireframe {
        build_wire(r);
    }
    if r.targets.as_ref().map(|t| t.w != w || t.h != h).unwrap_or(true) {
        r.targets = Some(make_targets(r, w, h));
    }
    let lit = view.channel == Channel::Lit;
    let radius = r.mesh.as_ref().map(|m| m.radius).unwrap_or(1.0);
    let lg = light_uniforms(view, radius, lit);
    if !lg.vps.is_empty() && r.shadows.is_none() {
        r.shadows = Some(make_shadows(r.dev));
    }
    let r: &Res = r;
    let (dev, q) = (r.dev, r.q);
    let mesh = r.mesh.as_ref().ok_or("3D view: no mesh")?;
    let tg = r.targets.as_ref().ok_or("3D view: no render targets")?;
    let env = r.env.as_ref().ok_or("3D view: no environment")?;
    let (base, normal, rma) = (r.base.as_ref().ok_or("3D view: no maps")?, r.normal.as_ref().ok_or("3D view: no maps")?, r.rma.as_ref().ok_or("3D view: no maps")?);

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
        1u32
    } else if view.relief == RELIEF_PARALLAX {
        2
    } else if view.relief == RELIEF_DISPLACE {
        3
    } else {
        0
    };
    let (sr, cr) = view.env.rotation.sin_cos();
    let bg_gain = view.env.exposure.exp2();
    let channel = view.channel.index() as u32;

    // per-object material table
    let mut obj_a = [[0.0f32; 4]; 8];
    let mut obj_c = [[0.0f32; 4]; 8];
    for (i, o) in mesh.objects.iter().take(8).enumerate() {
        let (kind, ro, me, col) = match o.material {
            scene::Material::Graph => (0.0, 0.5, 0.0, [0.0; 3]),
            scene::Material::Fixed(c, ro, me) => (1.0, ro, me, c),
        };
        let cull = if o.material == scene::Material::Graph && crate::render3d::shape_culls(view.shape) { 1.0 } else { 0.0 };
        obj_a[i] = [kind, ro, me, cull];
        obj_c[i] = [col[0], col[1], col[2], 0.0];
    }

    // ---------------------------------------------------------------- uniforms
    // scene
    let mut ub = Ub::default();
    ub.m4(&vp);
    ub.v4([cam.eye[0], cam.eye[1], cam.eye[2], view.exposure]);
    ub.v4([view.normal_amt.clamp(0.0, 1.0), view.bump, view.height_scale, view.height_level]);
    ub.v4([1.0 / hw.max(hh).max(1) as f32, disp, vtx_lod, opts.lod_bias.exp2()]);
    ub.u4([mode, has_h as u32, r.emis.is_some() as u32, opts.self_shadow as u32]);
    ub.u4([channel, lg.axis.len() as u32, 0, 0]);
    ub.v4([cr, sr, bg_gain * view.ambient, env.levels as f32]);
    for a in &obj_a {
        ub.v4(*a);
    }
    for c in &obj_c {
        ub.v4(*c);
    }
    let pad = |v: &Vec<[f32; 4]>, fill: [f32; 4]| -> Vec<[f32; 4]> {
        let mut o = v.clone();
        o.resize(scene::MAX_LIGHTS, fill);
        o
    };
    for a in pad(&lg.axis, [0.0, 1.0, 0.0, 1.0]) {
        ub.v4(a);
    }
    for a in pad(&lg.info, [0.0; 4]) {
        ub.v4(a);
    }
    for a in pad(&lg.col, [0.0, 0.0, 0.0, -1.0]) {
        ub.v4(a);
    }
    for i in 0..SHADOW_LAYERS as usize {
        ub.m4(lg.vps.get(i).unwrap_or(&[0.0; 16]));
    }
    for s in &env.sh {
        ub.v4(*s);
    }
    debug_assert_eq!(ub.0.len(), 1040);
    q.write_buffer(&r.scene_u, 0, &ub.0);
    // shadow passes
    for (i, lvp) in lg.vps.iter().enumerate() {
        let mut ub = Ub::default();
        ub.m4(lvp);
        ub.v4([disp, view.height_level, vtx_lod, 0.0]);
        for a in &obj_a {
            ub.v4(*a);
        }
        q.write_buffer(&r.shadow_u[i], 0, &ub.0);
    }
    // background
    let mut ub = Ub::default();
    ub.v4([cam.right[0], cam.right[1], cam.right[2], 0.0]);
    ub.v4([cam.up[0], cam.up[1], cam.up[2], 0.0]);
    ub.v4([cam.fwd[0], cam.fwd[1], cam.fwd[2], 0.0]);
    let bc = view.env.bg_color;
    ub.v4([bc[0].max(0.0).powf(2.2), bc[1].max(0.0).powf(2.2), bc[2].max(0.0).powf(2.2), 0.0]);
    ub.v4([cr, sr, bg_gain * view.exposure, view.env.blur.clamp(0.0, 1.0) * (env.levels.max(1) - 1) as f32]);
    ub.v4([cam.tan_half, aspect, 0.0, 0.0]);
    ub.u4([
        match view.env.background {
            Background::Gradient => 0,
            Background::Color => 1,
            Background::Environment => 2,
        },
        channel,
        0,
        0,
    ]);
    q.write_buffer(&r.bg_u, 0, &ub.0);
    // post
    let po = &view.post;
    let (ssao_on, dof_on, bloom_on) = (lit && po.ssao > 0.0, lit && po.dof > 0.0, lit && po.bloom > 0.0);
    // bloom weights, as in post.rs
    let mut nb = 1usize;
    let (mut bw, mut bh) = (w as usize, h as usize);
    for _ in 0..6 {
        if bw <= 2 || bh <= 2 {
            break;
        }
        bw = (bw / 2).max(1);
        bh = (bh / 2).max(1);
        nb += 1;
    }
    let nb = nb.min(tg.bright_levels as usize).min(8);
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
    let mut ub = Ub::default();
    ub.v4([w as f32, h as f32, NEAR, FAR]);
    ub.v4([cam.tan_half, aspect, cam.dist, (!lit) as u32 as f32]);
    ub.v4([po.ssao, po.ssao_radius, po.dof, po.focus]);
    ub.v4([po.bloom, po.bloom_threshold, po.vignette, po.contrast]);
    ub.v4([po.saturation, po.tonemap.index() as f32, nb as f32, 0.0]);
    ub.v4([1.0 + 0.25 * po.temperature - 0.1 * po.tint, 1.0 + 0.2 * po.tint, 1.0 - 0.25 * po.temperature - 0.1 * po.tint, 0.0]);
    ub.v4([wts[0], wts[1], wts[2], wts[3]]);
    ub.v4([wts[4], wts[5], wts[6], wts[7]]);
    q.write_buffer(&r.post_u, 0, &ub.0);
    // wireframe
    if view.wireframe {
        let mut ub = Ub::default();
        ub.m4(&vp);
        ub.v4([disp, view.height_level, vtx_lod, NEAR]);
        ub.v4([FAR, 0.0, 0.0, 0.0]);
        for a in &obj_a {
            ub.v4(*a);
        }
        q.write_buffer(&r.wire_u, 0, &ub.0);
    }

    // ---------------------------------------------------------------- bind groups
    let height_view = r.height.as_ref().map(|t| &t.view).unwrap_or(&r.dummy_h.view);
    let emis_view = r.emis.as_ref().map(|t| &t.view).unwrap_or(&r.dummy_rgba.view);
    let shadow_view = r.shadows.as_ref().map(|s| &s.array).unwrap_or(&r.dummy_shadow);
    let scene_bg = dev.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("scene"),
        layout: &r.pipes.bgl_scene,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: r.scene_u.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&r.maps_samp) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&r.env_samp) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&base.view) },
            wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&normal.view) },
            wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&rma.view) },
            wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(emis_view) },
            wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(height_view) },
            wgpu::BindGroupEntry { binding: 8, resource: wgpu::BindingResource::TextureView(&env.tx.view) },
            wgpu::BindGroupEntry { binding: 9, resource: wgpu::BindingResource::TextureView(shadow_view) },
            wgpu::BindGroupEntry { binding: 10, resource: wgpu::BindingResource::Sampler(&r.shadow_samp) },
        ],
    });
    let bg_bg = dev.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("background"),
        layout: &r.pipes.bgl_bg,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: r.bg_u.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&env.tx.view) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&r.env_samp) },
        ],
    });
    let post_bg = |src: &wgpu::TextureView, bright: &wgpu::TextureView| {
        dev.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post"),
            layout: &r.pipes.bgl_post,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: r.post_u.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(src) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&tg.zres) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(bright) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&r.lin_clamp) },
            ],
        })
    };

    let mut enc = dev.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("3D view") });
    // ---------------------------------------------------------------- shadow maps
    if let Some(sm) = &r.shadows {
        for (layer, _) in lg.vps.iter().enumerate() {
            let bg = dev.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("shadow"),
                layout: &r.pipes.bgl_depth,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: r.shadow_u[layer].as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&r.maps_samp) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(height_view) },
                ],
            });
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &sm.layers[layer],
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&r.pipes.shadow);
            pass.set_bind_group(0, &bg, &[]);
            pass.set_vertex_buffer(0, mesh.vbuf.slice(..));
            pass.set_index_buffer(mesh.ibuf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.index_count, 0, 0..1);
        }
    }
    // ---------------------------------------------------------------- scene into the float target
    {
        let color_view = tg.ms_color.as_ref().unwrap_or(&tg.hdr);
        let resolve: Option<&wgpu::TextureView> = None;
        let z_view = tg.ms_z.as_ref().unwrap_or(&tg.zres);
        let far = clear_color();
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    resolve_target: resolve,
                    ops: wgpu::Operations { load: clear_color(), store: wgpu::StoreOp::Store },
                }),
                Some(wgpu::RenderPassColorAttachment { view: z_view, resolve_target: None, ops: wgpu::Operations { load: far, store: wgpu::StoreOp::Store } }),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &tg.depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&r.pipes.bg);
        pass.set_bind_group(0, &bg_bg, &[]);
        pass.draw(0..3, 0..1);
        pass.set_pipeline(&r.pipes.scene);
        pass.set_bind_group(0, &scene_bg, &[]);
        pass.set_vertex_buffer(0, mesh.vbuf.slice(..));
        pass.set_index_buffer(mesh.ibuf.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..mesh.index_count, 0, 0..1);
    }
    // resolve the multisampled depth picture (without multisampling the scene pass wrote it directly)
    if let (Some(ms_c), Some(ms_z)) = (&tg.ms_color, &tg.ms_z) {
        let bg = dev.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("resolve"),
            layout: &r.pipes.bgl_dres,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(ms_c) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(ms_z) }],
        });
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("resolve"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment { view: &tg.hdr, resolve_target: None, ops: wgpu::Operations { load: clear_color(), store: wgpu::StoreOp::Store } }),
                Some(wgpu::RenderPassColorAttachment { view: &tg.zres, resolve_target: None, ops: wgpu::Operations { load: clear_color(), store: wgpu::StoreOp::Store } }),
            ],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&r.pipes.dres);
        pass.set_bind_group(0, &bg, &[]);
        pass.draw(0..3, 0..1);
    }
    // ---------------------------------------------------------------- post chain
    let post_pass = |enc: &mut wgpu::CommandEncoder, pipe: &wgpu::RenderPipeline, src: &wgpu::TextureView, bright: &wgpu::TextureView, dst: &wgpu::TextureView| {
        let bg = post_bg(src, bright);
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("post"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: dst, resolve_target: None, ops: wgpu::Operations { load: clear_color(), store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(pipe);
        pass.set_bind_group(0, &bg, &[]);
        pass.draw(0..3, 0..1);
    };
    let mut cur: &wgpu::TextureView = &tg.hdr;
    let mut cur_ping: Option<usize> = None;
    if ssao_on {
        post_pass(&mut enc, &r.pipes.ssao, cur, &tg.hdr, &tg.ping[0]);
        cur = &tg.ping[0];
        cur_ping = Some(0);
    }
    if dof_on {
        let dst = if cur_ping == Some(0) { 1 } else { 0 };
        post_pass(&mut enc, &r.pipes.dof, cur, &tg.hdr, &tg.ping[dst]);
        cur = &tg.ping[dst];
    }
    let bright_full = tg.bright.create_view(&wgpu::TextureViewDescriptor::default());
    if bloom_on {
        let b0 = tg.bright.create_view(&wgpu::TextureViewDescriptor { base_mip_level: 0, mip_level_count: Some(1), ..Default::default() });
        post_pass(&mut enc, &r.pipes.bright, cur, &tg.hdr, &b0);
        gen_mips(r, &mut enc, &tg.bright, HDR, tg.bright_levels);
    }
    post_pass(&mut enc, &r.pipes.fin, cur, &bright_full, &tg.out_view);
    // ---------------------------------------------------------------- wireframe, hidden lines removed with the depth
    if view.wireframe {
        if let Some((wbuf, wn)) = &mesh.wire {
            let bg = dev.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("wire"),
                layout: &r.pipes.bgl_wire,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: r.wire_u.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&r.maps_samp) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(height_view) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&tg.zres) },
                ],
            });
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("wire"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &tg.out_view, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&r.pipes.wire);
            pass.set_bind_group(0, &bg, &[]);
            pass.set_vertex_buffer(0, mesh.vbuf.slice(..));
            pass.set_index_buffer(wbuf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..*wn, 0, 0..1);
        }
    }
    // ---------------------------------------------------------------- the finished frame comes back as bytes
    enc.copy_texture_to_buffer(
        wgpu::ImageCopyTexture { texture: &tg.out, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::ImageCopyBuffer { buffer: &tg.staging, layout: wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(tg.row_bytes), rows_per_image: Some(h) } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    q.submit(Some(enc.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    tg.staging.slice(..).map_async(wgpu::MapMode::Read, move |res| {
        let _ = tx.send(res);
    });
    dev.poll(wgpu::Maintain::Wait);
    rx.recv().map_err(|e| format!("3D view: {e}"))?.map_err(|e| format!("3D view: {e}"))?;
    let data = tg.staging.slice(..).get_mapped_range();
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let o = (y * tg.row_bytes) as usize;
        out.extend_from_slice(&data[o..o + (w * 4) as usize]);
    }
    drop(data);
    tg.staging.unmap();
    Ok(out)
}

impl Gpu {
    fn draw(&mut self, dev: Dev, q: Queue, w: usize, h: usize, view: &View) -> Result<Frame, String> {
        let mut r = match self.res.take() {
            Some(r) => r,
            None => {
                let r = guard(dev, || create_res(dev, q))??;
                self.renderer = crate::gpu::adapter_name().unwrap_or_default();
                self.applied = None;
                if self.pending.is_none() {
                    self.pending = Some(prepare(&MapInputs { base: None, normal: None, rough: None, metal: None, ao: None, emissive: None, height: None }));
                }
                r
            }
        };
        let out = self.draw_with(&mut r, dev, w, h, view);
        self.res = Some(r);
        out
    }

    fn draw_with(&mut self, r: &mut Res, dev: Dev, w: usize, h: usize, view: &View) -> Result<Frame, String> {
        if let Some(maps) = self.pending.take() {
            let filled = guard(dev, || upload_maps(r, &maps))?;
            self.pack_count += filled;
            self.applied = None;
        }
        if self.applied.as_ref() != Some(&self.opts) {
            r.maps_samp = guard(dev, || sampler_for(dev, &self.opts))?;
            self.applied = Some(self.opts.clone());
        }
        // the whole picture follows from these; when none of them changed the last picture is still right
        let mut vc = view.clone();
        vc.custom = None;
        let custom = view.custom.as_ref().map(|c| std::sync::Arc::as_ptr(c) as usize).unwrap_or(0);
        let key = format!("{:?}|{custom}|{w}x{h}|{}|{:?}|{}", vc, self.map_gen, self.opts, view.env.image_key());
        if let Some((k, f)) = &self.last {
            if *k == key {
                return Ok(f.clone());
            }
        }
        // the lighting environment
        let want_key = view.env.image_key();
        if r.env.as_ref().map(|e| e.key != want_key).unwrap_or(true) {
            let env = match crate::hdri::load_env(&view.env.preset, view.env.file.as_deref()) {
                Ok(e) => {
                    self.env_error = None;
                    e
                }
                Err(msg) => {
                    self.env_error = Some(msg);
                    crate::hdri::load_env("studio", None).map_err(|e| format!("3D view: {e}"))?
                }
            };
            let tex = guard(dev, || upload_env(r, &env, want_key.clone()))?;
            r.env = Some(tex);
        }
        let opts = self.opts.clone();
        let (gen, mut builds) = (self.height_gen, 0u64);
        let rgba = guard(dev, || render(r, &opts, view, w as u32, h as u32, gen, &mut builds))??;
        self.mesh_builds += builds;
        self.seq += 1;
        let f = Frame { w, h, rgba: Arc::new(rgba), seq: self.seq };
        self.last = Some((key, f.clone()));
        self.frames += 1;
        Ok(f)
    }
}

/// The 3D view as a picture of `w` x `h` pixels. The textures stay in device memory, the picture is the only thing that
/// comes back. When nothing that matters changed since the last call the same picture is returned (same `seq`), so a
/// caller that only repaints when `seq` changes costs nothing while the view is still. `None` when the device cannot
/// do it (see `Gpu::error`): the caller shows the software renderer then.
pub fn frame(shared: &Shared, w: usize, h: usize, view: &View) -> Option<Frame> {
    let mut g = shared.lock().ok()?;
    if g.error.is_some() || w == 0 || h == 0 {
        return None;
    }
    let Some((dev, q)) = crate::gpu::viewer_parts() else {
        g.error = Some("no usable graphics adapter for the 3D view".to_string());
        return None;
    };
    match g.draw(dev, q, w, h, view) {
        Ok(f) => {
            if std::mem::take(&mut g.want_shot) {
                g.shot = Some((f.w, f.h, f.rgba.to_vec()));
            }
            Some(f)
        }
        Err(e) => {
            g.error = Some(e);
            None
        }
    }
}
