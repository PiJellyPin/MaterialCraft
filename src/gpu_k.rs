// SPDX-License-Identifier: MIT OR Apache-2.0
// Included into `gpu::imp` (the `gpu` feature): the kernel families driven by `Kern`, so they can use the private
// device plumbing (`pipeline`, `uniform`, `gbuf`, `dispatch`, `submit`).

impl Gpu {
    /// Runs one launch of a kernel family. `ins` are the pictures it reads (missing ones are padded with the first),
    /// `size` the result's size. The uniform block is `w, h, op, n`, the `p` parameters and the `x` table.
    #[allow(clippy::too_many_arguments)]
    pub fn kernel_g(&self, k: super::Kern, op: u32, n: u32, p: &[f32], x: &[f32], ins: &[&GBuf], size: (usize, usize)) -> Option<GBuf> {
        let (w, h) = size;
        let n_in = k.n_inputs();
        if w == 0 || h == 0 || !self.fits(w, h) || ins.len() > n_in || (n_in > 0 && ins.is_empty()) {
            return None;
        }
        if ins.iter().take(k.strict()).any(|t| (t.w, t.h) != (w, h)) {
            return None;
        }
        let mut u = Uni::default().u(w as u32).u(h as u32).u(op).u(n);
        for i in 0..k.p_vec4() * 4 {
            u = u.f(p.get(i).copied().unwrap_or(0.0));
        }
        for i in 0..k.x_vec4() * 4 {
            u = u.f(x.get(i).copied().unwrap_or(0.0));
        }
        let pipe = self.pipeline(k.name(), k.src());
        let ub = self.uniform(&u.done());
        let out = self.gbuf(w, h);
        let mut entries = vec![wgpu::BindGroupEntry { binding: 0, resource: ub.as_entire_binding() }];
        for b in 0..n_in {
            let t = ins.get(b).copied().unwrap_or(ins[0]);
            entries.push(wgpu::BindGroupEntry { binding: 1 + b as u32, resource: t.buf.as_entire_binding() });
        }
        entries.push(wgpu::BindGroupEntry { binding: 1 + n_in as u32, resource: out.buf.as_entire_binding() });
        let mut enc = self.dev.create_command_encoder(&Default::default());
        self.dispatch(&mut enc, &pipe, &entries, w, h);
        self.submit(enc);
        Some(out)
    }

    /// Minimum and maximum of the colour channels (xyz) and of the luminance (w) of a picture: a 2 x 1 picture, the
    /// minimum first. Two passes (256 workgroups, then one), everything stays on the device.
    pub fn minmax_g(&self, t: &GBuf) -> Option<GBuf> {
        if !self.fits(t.w, t.h) {
            return None;
        }
        let total = t.w * t.h;
        let groups = ((total + 255) / 256).clamp(1, 256);
        let partial = self.gbuf(2 * groups, 1);
        let out = self.gbuf(2, 1);
        let pipe = self.pipeline("k_minmax", super::Kern::MINMAX_SRC);
        let u1 = self.uniform(&Uni::default().u(t.w as u32).u(t.h as u32).u(0).u(groups as u32).done());
        let u2 = self.uniform(&Uni::default().u(t.w as u32).u(t.h as u32).u(1).u(groups as u32).done());
        let mut enc = self.dev.create_command_encoder(&Default::default());
        self.dispatch_line(&mut enc, &pipe, &[
            wgpu::BindGroupEntry { binding: 0, resource: u1.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: t.buf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: partial.buf.as_entire_binding() },
        ], groups as u32);
        self.dispatch_line(&mut enc, &pipe, &[
            wgpu::BindGroupEntry { binding: 0, resource: u2.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: partial.buf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: out.buf.as_entire_binding() },
        ], 1);
        self.submit(enc);
        Some(out)
    }
}
