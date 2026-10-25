struct U { w: u32, h: u32, op: u32, n: u32, p: array<vec4<f32>, 8> };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var<storage, read_write> dst: array<vec4<f32>>;
fn P(i: u32) -> f32 { return u.p[i >> 2u][i & 3u]; }
fn PU(i: u32) -> u32 { return bitcast<u32>(u.p[i >> 2u][i & 3u]); }
