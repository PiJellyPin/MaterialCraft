struct U { w: u32, h: u32, op: u32, n: u32, thr: f32, a: f32, b: f32, c: f32 };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> dst: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> cnt: array<atomic<u32>>;
