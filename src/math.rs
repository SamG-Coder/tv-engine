use std::ops::{Add, Mul, Sub};
#[derive(Clone, Copy, Debug, Default)]
pub struct V(pub f32, pub f32, pub f32);
impl Add for V {
    type Output = V;
    fn add(self, b: V) -> V {
        V(self.0 + b.0, self.1 + b.1, self.2 + b.2)
    }
}
impl Sub for V {
    type Output = V;
    fn sub(self, b: V) -> V {
        V(self.0 - b.0, self.1 - b.1, self.2 - b.2)
    }
}
impl Mul<f32> for V {
    type Output = V;
    fn mul(self, b: f32) -> V {
        V(self.0 * b, self.1 * b, self.2 * b)
    }
}
impl V {
    pub fn dot(self, b: V) -> f32 {
        self.0 * b.0 + self.1 * b.1 + self.2 * b.2
    }
    pub fn cross(self, b: V) -> V {
        V(
            self.1 * b.2 - self.2 * b.1,
            self.2 * b.0 - self.0 * b.2,
            self.0 * b.1 - self.1 * b.0,
        )
    }
    pub fn norm(self) -> V {
        self * (1.0 / self.dot(self).sqrt().max(0.00001))
    }
}
pub fn multiply(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    let mut c = [0.; 16];
    for col in 0..4 {
        for row in 0..4 {
            for k in 0..4 {
                c[col * 4 + row] += a[k * 4 + row] * b[col * 4 + k];
            }
        }
    }
    c
}
pub fn camera(eye: V, forward: V, aspect: f32) -> [f32; 16] {
    let f = forward.norm();
    let r = f.cross(V(0., 1., 0.)).norm();
    let u = r.cross(f);
    let view = [
        r.0,
        u.0,
        -f.0,
        0.,
        r.1,
        u.1,
        -f.1,
        0.,
        r.2,
        u.2,
        -f.2,
        0.,
        -r.dot(eye),
        -u.dot(eye),
        f.dot(eye),
        1.,
    ];
    let scale = 1.0 / (65f32.to_radians() / 2.).tan();
    let near = 0.2;
    let far = 110.;
    let proj = [
        scale / aspect,
        0.,
        0.,
        0.,
        0.,
        scale,
        0.,
        0.,
        0.,
        0.,
        (far + near) / (near - far),
        -1.,
        0.,
        0.,
        2. * far * near / (near - far),
        0.,
    ];
    multiply(proj, view)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projection_points_forward() {
        let m = camera(V(0., 0., 0.), V(0., 0., -1.), 16. / 9.);
        let z = -5.;
        let clipz = m[10] * z + m[14];
        let w = -z;
        assert!(clipz / w < 1. && clipz / w > -1.);
        assert!(m[0] > 0. && m[5] > 0.);
    }
}
/// Six normalized clip planes, extracted from a column-major view-projection matrix.
pub struct Frustum(pub [[f32; 4]; 6]);
impl Frustum {
    pub fn new(m: [f32; 16]) -> Self {
        let mut p = [[0.; 4]; 6];
        for axis in 0..3 {
            for side in 0..2 {
                let sign = if side == 0 { 1. } else { -1. };
                let i = axis * 2 + side;
                for col in 0..4 {
                    p[i][col] = m[col * 4 + 3] + sign * m[col * 4 + axis];
                }
                let n = (p[i][0] * p[i][0] + p[i][1] * p[i][1] + p[i][2] * p[i][2]).sqrt();
                for col in 0..4 {
                    p[i][col] /= n;
                }
            }
        }
        Self(p)
    }
    pub fn sphere(&self, p: V, r: f32) -> bool {
        self.0
            .iter()
            .all(|v| v[0] * p.0 + v[1] * p.1 + v[2] * p.2 + v[3] >= -r)
    }
}
#[cfg(test)]
mod frustum_tests {
    use super::*;
    #[test]
    fn visibility() {
        let f = Frustum::new(camera(V(0., 0., 0.), V(0., 0., -1.), 16. / 9.));
        assert!(f.sphere(V(0., 0., -5.), 1.));
        assert!(!f.sphere(V(0., 0., 5.), 1.));
        assert!(!f.sphere(V(100., 0., -5.), 1.));
        assert!(f.sphere(V(0., 0., 0.), 1.));
    }
}
