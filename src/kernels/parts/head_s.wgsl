struct U { w: u32, h: u32, op: u32, n: u32, p: array<vec4<f32>, 8> };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read> ia: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> ib: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> ic: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read> idd: array<vec4<f32>>;
@group(0) @binding(5) var<storage, read_write> dst: array<vec4<f32>>;
fn P(i: u32) -> f32 { return u.p[i >> 2u][i & 3u]; }
fn IW() -> u32 { return u.w; }
fn IH() -> u32 { return u.h; }
