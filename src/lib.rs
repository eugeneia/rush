use std::cmp;
use std::ptr;
use regex::Regex;

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

pub fn comma_value(n: u64) -> String { // credit http://richard.warburton.it
    let s = format!("{}", n);
    let re = Regex::new(r"^(\d\d?\d?)(\d{3}*)$").unwrap();
    if let Some(cap) = re.captures(&s) {
        let (left, num) = (&cap[1], &cap[2]);
        let re = Regex::new(r"(\d{3})").unwrap();
        let rev = |s: &str| { s.chars().rev().collect::<String>() };
        format!("{}{}", left, rev(&re.replace_all(&rev(&num), "$1,").to_string()))
    } else { s }
}
