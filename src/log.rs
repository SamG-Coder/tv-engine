use std::ffi::c_char;
#[link(name = "log")]
extern "C" {
    fn __android_log_write(priority: i32, tag: *const c_char, text: *const c_char) -> i32;
}
pub fn log(s: &str) {
    if let Ok(s) = std::ffi::CString::new(s) {
        unsafe {
            __android_log_write(4, b"Rust3D\0".as_ptr().cast(), s.as_ptr());
        }
    }
}
