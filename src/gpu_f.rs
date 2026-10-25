// SPDX-License-Identifier: MIT OR Apache-2.0
// Included into `gpu::imp` (the `gpu` feature): multi-pass kernels on raw 32-bit buffers (connected regions, scans,
// histograms). Everything stays on the device; one command buffer runs all passes of a node.

/// A raw device buffer of 32-bit words (counted like pictures against the memory budget).
pub struct RBuf {
    pub buf: wgpu::Buffer,
    bytes: u64,
}

impl Drop for RBuf {
    fn drop(&mut self) {
        LIVE.fetch_sub(self.bytes, Ordering::Relaxed);
    }
}

impl Gpu {
    /// Zero-filled raw buffer of `words` 32-bit words (at least four).
    fn rbuf(&self, words: usize) -> RBuf {
        let bytes = (words.max(4) * 4) as u64;
        LIVE.fetch_add(bytes, Ordering::Relaxed);
        RBuf { buf: self.buf(bytes, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST), bytes }
    }

    /// True when `extra` more bytes next to `pics` pictures of the given size still fit the budget.
    fn fits_raw(&self, w: usize, h: usize, extra: u64) -> bool {
        self.fits(w, h) && LIVE.load(Ordering::Relaxed) + extra + (w * h * 16) as u64 <= self.max_live
    }

    /// One pass with `gx * gy` workgroups.
    fn pass_xy(&self, enc: &mut wgpu::CommandEncoder, pipe: &wgpu::ComputePipeline, entries: &[wgpu::BindGroupEntry], gx: u32, gy: u32) {
        let bg = self.dev.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &pipe.get_bind_group_layout(0), entries });
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None });
        pass.set_pipeline(pipe);
        pass.set_bind_group(0, &bg, &[]);
        pass.dispatch_workgroups(gx.max(1), gy.max(1), 1);
    }

    /// Connected regions of a picture (4-neighbours, wrapping around the tile) and what the flood-fill nodes make of
    /// them. `mode` 0: pixels with luminance above `thr` form regions; 1: touching pixels with the same region id
    /// (the red channel of a flood-fill result) form regions. `out`: 11 flood fill, 12 index, 13 centre, 14 size.
    pub fn flood_g(&self, t: &GBuf, mode: u32, thr: f32, out: u32) -> Option<GBuf> {
        let (w, h) = (t.w, t.h);
        let n = w * h;
        if n == 0 || n > u32::MAX as usize / 8 {
            return None;
        }
        // scan levels: sizes of the arrays that hold the block totals
        let mut lv = vec![n];
        while *lv.last()? > 1024 {
            lv.push((lv.last()? + 1023) / 1024);
        }
        let nl = lv.len();
        let mut base = vec![0usize; nl + 1];
        for k in 1..nl {
            base[k + 1] = base[k] + lv[k];
        }
        if nl == 1 {
            base[1] = 0;
        }
        let sums = base[nl] + 1;
        let wwx = (w + 31) / 32;
        let wwy = (h + 31) / 32;
        let off_ox = sums;
        let off_oy = off_ox + h * wwx;
        let off_sx = off_oy + w * wwy;
        let off_sy = off_sx + h;
        let words = off_sy + w;
        let raw = ((n * 2 + n * 2 + n * 4 + words) * 4) as u64;
        if !self.fits_raw(w, h, raw) || (n * 16) as u64 > self.max_bytes {
            return None;
        }
        let par = self.rbuf(n);
        let scn = self.rbuf(n);
        let slt = self.rbuf(2 * n);
        let bbx = self.rbuf(4 * n);
        let msc = self.rbuf(words);
        let dst = self.gbuf(w, h);
        let pipe = self.pipeline("k_flood", include_str!("kernels/flood.wgsl"));
        let mut enc = self.dev.create_command_encoder(&Default::default());
        let (gx, gy) = (((w + 7) / 8) as u32, ((h + 7) / 8) as u32);
        // uniform block: w h op n | mode lvl cnt lbase | nbase off_ox off_sx off_oy | off_sy wwx wwy total | thr a b c
        let mk = |op: u32, lvl: usize, cnt: usize, lbase: usize, nbase: usize| -> wgpu::Buffer {
            let u = Uni::default()
                .u(w as u32).u(h as u32).u(op).u(if op == 1 { 0 } else { 1 })
                .u(mode).u(lvl as u32).u(cnt as u32).u(lbase as u32)
                .u(nbase as u32).u(off_ox as u32).u(off_sx as u32).u(off_oy as u32)
                .u(off_sy as u32).u(wwx as u32).u(wwy as u32).u((n - 1) as u32)
                .f(thr).f(0.0).f(0.0).f(0.0);
            self.uniform(&u.done())
        };
        let run = |enc: &mut wgpu::CommandEncoder, ub: &wgpu::Buffer, gx: u32, gy: u32| {
            let entries = [
                wgpu::BindGroupEntry { binding: 0, resource: ub.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: t.buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: dst.buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: par.buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: scn.buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 5, resource: slt.buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 6, resource: bbx.buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 7, resource: msc.buf.as_entire_binding() },
            ];
            self.pass_xy(enc, &pipe, &entries, gx, gy);
        };
        // one-dimensional launches: `count` threads
        let line = |count: usize| -> (u32, u32) {
            let groups = (count + 63) / 64;
            let gx = groups.min(32768).max(1);
            (gx as u32, ((groups + gx - 1) / gx).max(1) as u32)
        };
        let keep: Vec<wgpu::Buffer> = {
            let mut v = Vec::new();
            let mut go = |enc: &mut wgpu::CommandEncoder, op: u32, lvl: usize, cnt: usize, lb: usize, nb: usize, g: (u32, u32)| {
                let ub = mk(op, lvl, cnt, lb, nb);
                run(enc, &ub, g.0, g.1);
                v.push(ub);
            };
            go(&mut enc, 0, 0, 0, 0, 0, (gx, gy));
            go(&mut enc, 1, 0, 0, 0, 0, line(w.max(h)));
            go(&mut enc, 2, 0, 0, 0, 0, (gx, gy));
            go(&mut enc, 3, 0, 0, 0, 0, (gx, gy));
            go(&mut enc, 4, 0, 0, 0, 0, (gx, gy));
            // prefix sums of the root flags: up the levels, then the block offsets back down
            for k in 0..nl {
                let blocks = (lv[k] + 1023) / 1024;
                let g = (blocks.min(32768).max(1) as u32, ((blocks + 32767) / 32768).max(1) as u32);
                go(&mut enc, 5, k, lv[k], if k == 0 { 0 } else { base[k] }, base[k + 1], g);
            }
            for k in (0..nl.saturating_sub(1)).rev() {
                go(&mut enc, 6, k, lv[k], if k == 0 { 0 } else { base[k] }, base[k + 1], line(lv[k]));
            }
            go(&mut enc, 7, 0, 0, 0, 0, (gx, gy));
            go(&mut enc, 8, 0, 0, 0, 0, (gx, gy));
            go(&mut enc, 9, 0, 0, 0, 0, (gx, gy));
            go(&mut enc, 10, 0, 0, 0, 0, (gx, gy));
            go(&mut enc, out, 0, 0, 0, 0, (gx, gy));
            v
        };
        self.submit(enc);
        drop(keep);
        Some(dst)
    }

    /// Whole-picture statistics. `op` 0: equalising curves of the colour channels and the luminance as a `bins` x 1
    /// picture (components r, g, b, luminance; -1 where the curve would do nothing); 1: bounding box (x0, y0, x1, y1) of
    /// the pixels whose brightest colour channel exceeds `thr`, as a 1 x 1 picture (-1 when there are none).
    pub fn stats_g(&self, t: &GBuf, op: u32, thr: f32) -> Option<GBuf> {
        let (w, h) = (t.w, t.h);
        if w * h == 0 {
            return None;
        }
        const BINS: usize = 1024;
        let cnt = self.rbuf(if op == 0 { 4 * BINS } else { 4 });
        let dst = if op == 0 { self.gbuf(BINS, 1) } else { self.gbuf(1, 1) };
        let pipe = self.pipeline("k_stats", include_str!("kernels/stats.wgsl"));
        let mut enc = self.dev.create_command_encoder(&Default::default());
        let (gx, gy) = (((w + 7) / 8) as u32, ((h + 7) / 8) as u32);
        let mut keep = Vec::new();
        let mut go = |enc: &mut wgpu::CommandEncoder, pass_op: u32, g: (u32, u32)| {
            let u = Uni::default().u(w as u32).u(h as u32).u(pass_op).u(BINS as u32).f(thr).f(0.0).f(0.0).f(0.0);
            let ub = self.uniform(&u.done());
            let entries = [
                wgpu::BindGroupEntry { binding: 0, resource: ub.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: t.buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: dst.buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: cnt.buf.as_entire_binding() },
            ];
            self.pass_xy(enc, &pipe, &entries, g.0, g.1);
            keep.push(ub);
        };
        if op == 0 {
            go(&mut enc, 0, (gx, gy));
            go(&mut enc, 1, (1, 1));
        } else {
            go(&mut enc, 3, (1, 1));
            go(&mut enc, 2, (gx, gy));
            go(&mut enc, 4, (1, 1));
        }
        self.submit(enc);
        drop(keep);
        Some(dst)
    }
}
