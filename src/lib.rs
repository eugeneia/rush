use std::cmp;
use std::ptr;

pub fn fill(dst: &mut [u8], len: usize, val: u8) {
    unsafe {
        ptr::write_bytes(dst.as_mut_ptr(), val, cmp::min(len, dst.len()));
    }
}

pub fn copy(dst: &mut [u8], src: &[u8], len: usize) {
    unsafe {
        ptr::copy(src.as_ptr(), dst.as_mut_ptr(),
                  cmp::min(len, cmp::min(src.len(), dst.len())));
    }
}
