use crate::geometry::{Instance, Vertex};
use crate::math::V;
pub struct Chunk {
    pub first: i32,
    pub count: i32,
    pub center: V,
}
pub struct Scene {
    pub vertices: Vec<Vertex>,
    pub chunks: Vec<Chunk>,
    pub water: (i32, i32),
    pub gem: (i32, i32),
    pub gems: [V; 12],
}
pub fn height(x: f32, z: f32) -> f32 {
    let r = (x * x + z * z).sqrt();
    let edge = ((32. - r) / 12.).clamp(0., 1.);
    -2. + edge * (4.4 + (x * 0.18).sin() * (z * 0.14).cos() * 1.7)
}
fn tri(out: &mut Vec<Vertex>, a: V, b: V, c: V, col: V) {
    let n = (b - a).cross(c - a).norm();
    let light = (0.62 + n.dot(V(-0.4, 0.8, 0.25).norm()).max(0.) * 0.38).clamp(0.4, 1.);
    for p in [a, b, c] {
        out.push(Vertex {
            p: [p.0, p.1, p.2],
            c: [col.0 * light, col.1 * light, col.2 * light],
        });
    }
}
fn cone(out: &mut Vec<Vertex>, p: V, r: f32, h: f32, col: V, sides: usize) {
    for i in 0..sides {
        let a = i as f32 * std::f32::consts::TAU / sides as f32;
        let b = (i + 1) as f32 * std::f32::consts::TAU / sides as f32;
        let a = p + V(a.cos() * r, 0., a.sin() * r);
        let b = p + V(b.cos() * r, 0., b.sin() * r);
        let top = p + V(0., h, 0.);
        if h >= 0. {
            tri(out, a, top, b, col);
        } else {
            tri(out, b, top, a, col);
        }
    }
}
fn pillar(out: &mut Vec<Vertex>, p: V, r: f32, h: f32) {
    for i in 0..6 {
        let a = i as f32 * std::f32::consts::TAU / 6.;
        let b = (i + 1) as f32 * std::f32::consts::TAU / 6.;
        let x = p + V(a.cos() * r, 0., a.sin() * r);
        let y = p + V(b.cos() * r, 0., b.sin() * r);
        tri(out, x, x + V(0., h, 0.), y, V(0.68, 0.70, 0.61));
        tri(
            out,
            y,
            x + V(0., h, 0.),
            y + V(0., h, 0.),
            V(0.68, 0.70, 0.61),
        );
    }
}
pub fn build() -> Scene {
    let mut v = Vec::with_capacity(80000);
    let mut chunks = Vec::new();
    for cz in 0..8 {
        for cx in 0..8 {
            let first = v.len() as i32;
            let ox = cx as f32 * 10. - 40.;
            let oz = cz as f32 * 10. - 40.;
            for z in 0..5 {
                for x in 0..5 {
                    let x = ox + x as f32 * 2.;
                    let z = oz + z as f32 * 2.;
                    let a = V(x, height(x, z), z);
                    let b = V(x + 2., height(x + 2., z), z);
                    let c = V(x, height(x, z + 2.), z + 2.);
                    let d = V(x + 2., height(x + 2., z + 2.), z + 2.);
                    let col = if a.1 < 0.9 {
                        V(0.76, 0.67, 0.43)
                    } else {
                        V(0.24, 0.47, 0.28)
                    };
                    tri(&mut v, a, c, b, col);
                    tri(&mut v, b, c, d, col);
                }
            }
            for i in 0..5 {
                let seed = (cx * 131 + cz * 719 + i * 37) as f32;
                let x = ox + 1. + (seed * 1.37).sin().abs() * 8.;
                let z = oz + 1. + (seed * 2.73).cos().abs() * 8.;
                let y = height(x, z);
                if y > 1.3 && (x * x + z * z) > 70. && x.abs() > 2.5 {
                    let p = V(x, y, z);
                    pillar(&mut v, p, 0.16, 1.4);
                    cone(&mut v, p + V(0., 0.8, 0.), 1.2, 3.5, V(0.10, 0.32, 0.25), 5);
                    cone(&mut v, p + V(0., 2.0, 0.), 0.9, 2.8, V(0.16, 0.40, 0.29), 5);
                }
            }
            if cx == 4 && cz == 4 {
                for i in 0..8 {
                    let a = i as f32 * std::f32::consts::TAU / 8.;
                    let x = a.cos() * 5.;
                    let z = a.sin() * 5.;
                    pillar(&mut v, V(x, height(x, z), z), 0.6, 4.8);
                }
                cone(
                    &mut v,
                    V(0., height(0., 0.), 0.),
                    1.4,
                    6.,
                    V(0.52, 0.76, 0.67),
                    6,
                );
            }
            chunks.push(Chunk {
                first,
                count: v.len() as i32 - first,
                center: V(ox + 5., 2., oz + 5.),
            });
        }
    }
    let start = v.len() as i32;
    for z in 0..32 {
        for x in 0..32 {
            let x = x as f32 * 5. - 80.;
            let z = z as f32 * 5. - 80.;
            tri(
                &mut v,
                V(x, 0., z),
                V(x, 0., z + 5.),
                V(x + 5., 0., z),
                V(0.09, 0.46, 0.55),
            );
            tri(
                &mut v,
                V(x + 5., 0., z),
                V(x, 0., z + 5.),
                V(x + 5., 0., z + 5.),
                V(0.09, 0.46, 0.55),
            );
        }
    }
    let water = (start, v.len() as i32 - start);
    let start = v.len() as i32;
    cone(&mut v, V(0., 0., 0.), 0.55, 0.8, V(1., 0.76, 0.27), 4);
    cone(&mut v, V(0., 0., 0.), 0.55, -0.8, V(1., 0.55, 0.16), 4);
    let gem = (start, v.len() as i32 - start);
    let gems = std::array::from_fn(|i| {
        let a = i as f32 * std::f32::consts::TAU / 12.;
        let x = a.cos() * 17.;
        let z = a.sin() * 17.;
        V(x, height(x, z) + 1.4, z)
    });
    Scene {
        vertices: v,
        chunks,
        water,
        gem,
        gems,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_scene() {
        let s = build();
        assert_eq!(s.chunks.len(), 64);
        assert!(s.vertices.len() < 80000);
        for v in s.vertices {
            assert!(v.p.iter().chain(v.c.iter()).all(|x| x.is_finite()));
        }
        for p in s.gems {
            assert!(height(p.0, p.2) > 0.);
        }
    }
}

pub fn stress(count: usize) -> (Vec<Vertex>, Vec<Instance>) {
    let mut mesh = Vec::new();
    let faces = [
        (
            [[-1., -1., 1.], [1., -1., 1.], [1., 1., 1.], [-1., 1., 1.]],
            [0.5, 0.7, 0.9],
        ),
        (
            [
                [1., -1., -1.],
                [-1., -1., -1.],
                [-1., 1., -1.],
                [1., 1., -1.],
            ],
            [0.3, 0.5, 0.7],
        ),
        (
            [
                [-1., -1., -1.],
                [-1., -1., 1.],
                [-1., 1., 1.],
                [-1., 1., -1.],
            ],
            [0.4, 0.6, 0.8],
        ),
        (
            [[1., -1., 1.], [1., -1., -1.], [1., 1., -1.], [1., 1., 1.]],
            [0.6, 0.8, 1.],
        ),
        (
            [[-1., 1., 1.], [1., 1., 1.], [1., 1., -1.], [-1., 1., -1.]],
            [0.7, 0.9, 1.],
        ),
        (
            [
                [-1., -1., -1.],
                [1., -1., -1.],
                [1., -1., 1.],
                [-1., -1., 1.],
            ],
            [0.3, 0.4, 0.5],
        ),
    ];
    for (f, _c) in faces {
        for i in [0, 1, 2, 0, 2, 3] {
            mesh.push(Vertex {
                p: f[i],
                c: [
                    0.4 + f[i][1] * 0.12,
                    0.65 + f[i][1] * 0.12,
                    0.8 + f[i][1] * 0.12,
                ],
            });
        }
    }
    let mut instances = Vec::with_capacity(count);
    for i in 0..count {
        let angle = i as f32 * 2.39996;
        let r = 5. + (i as f32 / count as f32).sqrt() * 22.;
        let x = angle.cos() * r;
        let z = angle.sin() * r;
        let size = 0.18;
        instances.push(crate::scene::Instance {
            model: [
                size,
                0.,
                0.,
                0.,
                0.,
                size,
                0.,
                0.,
                0.,
                0.,
                size,
                0.,
                x,
                height(x, z) + 0.5 + (i % 7) as f32 * 0.4,
                z,
                1.,
            ],
            tint: [1., 1., 1., 1.],
        });
    }

    (mesh, instances)
}
