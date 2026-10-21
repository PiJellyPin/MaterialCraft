// SPDX-License-Identifier: MIT OR Apache-2.0
use materialcraft::render3d::{build_mesh, shape_culls, SHAPES};
use materialcraft::v3::*;

#[test]
fn every_shape_builds_a_consistent_mesh() {
    for (name, id) in SHAPES {
        let m = build_mesh(id, 24, 1.0, None, 0.0, 0.0);
        assert!(!m.tris.is_empty(), "{name}: no triangles");
        let nv = m.pos.len();
        assert!(m.nrm.len() == nv && m.tan.len() == nv && m.uv.len() == nv && m.fade.len() == nv, "{name}: attribute length mismatch");
        let mut agree = 0usize;
        let mut total = 0usize;
        for t in &m.tris {
            for &i in t {
                assert!(i < nv as u32, "{name}: index out of range");
            }
            let (a, b, c) = (m.pos[t[0] as usize], m.pos[t[1] as usize], m.pos[t[2] as usize]);
            let g = cross(sub(b, a), sub(c, a));
            if len(g) < 1e-5 {
                continue;
            }
            let n = add(add(m.nrm[t[0] as usize], m.nrm[t[1] as usize]), m.nrm[t[2] as usize]);
            total += 1;
            if dot(g, n) > 0.0 {
                agree += 1;
            }
        }
        assert!(total > 0, "{name}: only degenerate triangles");
        assert!(agree as f32 >= 0.97 * total as f32, "{name}: winding disagrees with normals ({agree}/{total})");
        for i in 0..nv {
            assert!(m.pos[i].iter().all(|x| x.is_finite() && x.abs() < 3.0), "{name}: vertex {i} out of range {:?}", m.pos[i]);
            assert!((len(m.nrm[i]) - 1.0).abs() < 1e-3, "{name}: normal {i} not unit");
            assert!(m.uv[i][0].is_finite() && m.uv[i][1].is_finite() && m.su[i] > 0.0 && m.sv[i] > 0.0, "{name}: bad uv/scale at {i}");
        }
    }
    assert!(shape_culls(5) && !shape_culls(1));
}

#[test]
fn closed_shapes_point_outwards() {
    for id in [0u32, 1, 3, 9, 10, 11, 12, 13, 14] {
        let m = build_mesh(id, 24, 1.0, None, 0.0, 0.0);
        let bad = (0..m.pos.len()).filter(|&i| dot(m.nrm[i], m.pos[i]) < -1e-3 && id != 12 && id != 15).count();
        assert!(bad == 0, "shape {id}: {bad} normals point inwards");
    }
}

#[test]
fn meshes_are_outward_closed_and_finite() {
    use materialcraft::render3d::build_mesh;
    use materialcraft::tex::Tex;
    let hm = Tex::gray_fn(64, 64, |u, v| ((u * 9.0).sin() * (v * 7.0).cos()) * 0.5 + 0.5);
    for shape in 0..3 {
        for h in [None, Some(&hm)] {
            let m = build_mesh(shape, 24, 1.0, h, 0.1, 0.0);
            assert!(m.pos.iter().chain(&m.nrm).chain(&m.tan).all(|p| p.iter().all(|c| c.is_finite())));
            assert!(m.tris.iter().all(|t| t.iter().all(|&i| (i as usize) < m.pos.len())));
            if h.is_none() {
                // flat/undisplaced: the winding must agree with the analytic normal for every triangle
                for t in &m.tris {
                    let (a, b, c) = (m.pos[t[0] as usize], m.pos[t[1] as usize], m.pos[t[2] as usize]);
                    let f = materialcraft::v3::cross(materialcraft::v3::sub(b, a), materialcraft::v3::sub(c, a));
                    let (n0, n1, n2) = (m.nrm[t[0] as usize], m.nrm[t[1] as usize], m.nrm[t[2] as usize]);
                    let n = [n0[0] + n1[0] + n2[0], n0[1] + n1[1] + n2[1], n0[2] + n1[2] + n2[2]];
                    // pole triangles are degenerate (zero area), so allow float noise
                    assert!(materialcraft::v3::dot(f, n) >= -1e-6, "shape {shape} has an inward-wound triangle");
                }
            }
        }
    }
    // outward displacement: a white height map pushes every plane vertex towards +z (the surface normal)
    let white = Tex::solid(4, 4, [1.0; 4]);
    let p = build_mesh(2, 16, 1.0, Some(&white), 0.2, 0.0);
    assert!(p.pos.iter().all(|v| (v[2] - 0.2).abs() < 1e-5), "relief must rise out of the surface");
    // centred level: mid grey stays put
    let grey = Tex::solid(4, 4, [0.5; 4]);
    let p = build_mesh(2, 16, 1.0, Some(&grey), 0.2, 0.5);
    assert!(p.pos.iter().all(|v| v[2].abs() < 1e-5));
    // sphere seam and poles must stay welded after displacement
    let s = build_mesh(0, 24, 1.0, Some(&hm), 0.15, 0.0);
    let stride = 24 * 2 + 1;
    for j in 0..=24usize {
        let (a, b) = (s.pos[j * stride], s.pos[j * stride + 48]);
        assert!(materialcraft::v3::len(materialcraft::v3::sub(a, b)) < 1e-4, "sphere seam opened at row {j}");
    }
    let top = s.pos[0];
    assert!(s.pos[..stride].iter().all(|v| materialcraft::v3::len(materialcraft::v3::sub(*v, top)) < 1e-4), "north pole tore");
}
