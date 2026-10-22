use crate::{
    geometry::Instance,
    math::{Frustum, V},
};
/// Reusable visibility workspace. Cache transform bounds when objects change; sort compact
/// distance/index pairs rather than repeatedly moving 80-byte instance records.
pub struct Visibility {
    objects: Vec<(Instance, f32)>,
    keys: Vec<(f32, usize)>,
    pub instances: Vec<Instance>,
}
impl Visibility {
    pub fn new(objects: &[Instance], radius: f32) -> Self {
        Self {
            objects: objects
                .iter()
                .map(|obj| {
                    let m = obj.model;
                    let scale = ([0, 1, 2, 4, 5, 6, 8, 9, 10]
                        .iter()
                        .map(|i| m[*i] * m[*i])
                        .sum::<f32>())
                    .sqrt();
                    (*obj, radius * scale)
                })
                .collect(),
            keys: Vec::with_capacity(objects.len()),
            instances: Vec::with_capacity(objects.len()),
        }
    }
    pub fn update(&mut self, vp: [f32; 16], eye: V) {
        self.keys.clear();
        self.instances.clear();
        let frustum = Frustum::new(vp);
        for (i, (obj, radius)) in self.objects.iter().enumerate() {
            let m = obj.model;
            let center = V(m[12], m[13], m[14]);
            if frustum.sphere(center, *radius) {
                let delta = center - eye;
                self.keys.push((delta.dot(delta), i));
            }
        }
        self.keys
            .sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        self.instances
            .extend(self.keys.iter().map(|(_, i)| self.objects[*i].0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn instance(z: f32) -> Instance {
        Instance {
            model: [
                1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., z, 1.,
            ],
            tint: [1.; 4],
        }
    }
    #[test]
    fn culls_behind_and_sorts_front_to_back_without_growing() {
        let objects = [instance(-10.), instance(10.), instance(-3.)];
        let mut v = Visibility::new(&objects, 1.);
        let capacity = v.instances.capacity();
        let vp = crate::math::camera(V(0., 0., 0.), V(0., 0., -1.), 1.);
        v.update(vp, V(0., 0., 0.));
        assert_eq!(v.instances.len(), 2);
        assert_eq!(v.instances[0].model[14], -3.);
        assert_eq!(v.instances[1].model[14], -10.);
        v.update(vp, V(0., 0., 0.));
        assert_eq!(v.instances.capacity(), capacity);
    }
}
