//! Backend-independent mesh and affine instance inputs. Matrices are column-major.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Vertex {
    pub p: [f32; 3],
    pub c: [f32; 3],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Instance {
    pub model: [f32; 16],
    pub tint: [f32; 4],
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gpu_input_layout() {
        assert_eq!(std::mem::size_of::<Vertex>(), 24);
        assert_eq!(std::mem::size_of::<Instance>(), 80);
    }
}
