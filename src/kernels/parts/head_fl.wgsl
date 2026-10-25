struct U {
    w: u32, h: u32, op: u32, n: u32,
    mode: u32, lvl: u32, cnt: u32, lbase: u32,
    nbase: u32, off_ox: u32, off_sx: u32, off_oy: u32,
    off_sy: u32, wwx: u32, wwy: u32, total: u32,
    thr: f32, a: f32, b: f32, c: f32,
};
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dst: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> par: array<atomic<u32>>;
@group(0) @binding(4) var<storage, read_write> scn: array<atomic<u32>>;
@group(0) @binding(5) var<storage, read_write> slt: array<atomic<u32>>;
@group(0) @binding(6) var<storage, read_write> bbx: array<atomic<u32>>;
@group(0) @binding(7) var<storage, read_write> msc: array<atomic<u32>>;
