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

// Increase value to be a multiple of size (if it is not already).
pub fn align(value: usize, size: usize) -> usize {
   if value % size == 0 {
       value
   } else {
       value + size - (value % size)
   }
}

#[cfg(target_endian = "little")]
pub fn htonl (l: u32) -> u32 { l.swap_bytes() }
#[cfg(target_endian = "little")]
pub fn ntohl (l: u32) -> u32 { l.swap_bytes() }
#[cfg(target_endian = "little")]
pub fn htons (s: u16) -> u16 { s.swap_bytes() }
#[cfg(target_endian = "little")]
pub fn ntohs (s: u16) -> u16 { s.swap_bytes() }
#[cfg(target_endian = "big")]
pub fn htonl (l: u32) -> u32 { l }
#[cfg(target_endian = "big")]
pub fn ntohl (l: u32) -> u32 { l }
#[cfg(target_endian = "big")]
pub fn htons (s: u16) -> u16 { s }
#[cfg(target_endian = "big")]
pub fn ntohs (s: u16) -> u16 { s }

