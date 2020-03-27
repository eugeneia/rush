use super::engine;

use std::cmp::max;

// PACKET STRUCT AND FREELIST
//
// This module defines a struct to represent packets of network data, and
// implements a global freelist from which packets can be allocated.
//
//   Packet - packet structure with length and data fields
//   PAYLOAD_SIZE - size of packet’s data field
//   init() - initializes the freelist with FREELIST_SIZE packets
//   allocate() -> Box<Packet> - take a packet off the freelist for use
//   free(Box<Packet>) - return a packet to the freelist

// The maximum amount of payload in any given packet.
pub const PAYLOAD_SIZE: usize = 1024*10;

// Packet of network data, with associated metadata.
// XXX - should be #[repr(C, packed)], however that would require unsafe{} to
// access members. Is the memory layout in repr(rust) equivalent?
pub struct Packet {
    pub length: u16, // data payload length
    pub data: [u8; PAYLOAD_SIZE]
}

// A packet may never go out of scope. It is either on the freelist, a link, or
// in active use (in-scope).
// XXX - Could free() packets automatically in Drop, and obsolete manual free.
impl Drop for Packet { fn drop(&mut self) { panic!("Packet leaked"); } }

// Allocate a packet struct on the heap (initialized all-zero).
// NB: Box is how we heap-allocate in Rust.
// XXX - This is a stub. Eventually packets may need to be allocated in DMA
// pages, and follow strict alignment invariants.
fn new_packet() -> Box<Packet> {
    Box::new(Packet { length: 0, data: [0; PAYLOAD_SIZE] })
}

// Number of packets initially on the freelist.
const FREELIST_SIZE: usize = 100_000;

// Freelist consists of an array of mutable raw pointers to Packet,
// and a fill counter.
struct Freelist {
    list: [*mut Packet; FREELIST_SIZE],
    nfree: usize
}

// FL: global freelist (initially empty, populated with null ptrs).
static mut FL: Freelist = Freelist {
    list: [std::ptr::null_mut(); FREELIST_SIZE],
    nfree: 0
};

// Fill up FL with freshly allocated packets.
// NB: using FL is unsafe because it is a mutable static (we have to ensure
// thread safety).
// NB: we can cast a mutable reference of the boxed packet (&mut *p) to a raw
// pointer.
// NB: we std::mem::forget the Box p before it exits scope to avoid the heap
// allocated packet from being Dropped (i.e., we intentionally leak
// FREELIST_SIZE packets onto the static FL).
// XXX - eventually, new memory needs to be allocated on-demand dynamically.
pub fn init() {
    while unsafe { FL.nfree < FREELIST_SIZE } {
        let mut p = new_packet();
        unsafe { FL.list[FL.nfree] = &mut *p; } std::mem::forget(p);
        unsafe { FL.nfree += 1; }
    }
}

// Allocate an empty Boxed Packet from FL.
// NB: we can use Box::from_raw safely on the packets "leaked" onto
// the static FL. We can also be sure that the Box does not alias another
// packet (see free).
pub fn allocate() -> Box<Packet> {
    if unsafe { FL.nfree == 0 } { panic!("Packet freelist underflow"); }
    unsafe { FL.nfree -= 1; }
    unsafe { Box::from_raw(FL.list[FL.nfree]) }
}

// Return Boxed Packet to FL.
// NB: because p is mutable and Box does not implement the Copy trait free
// effectively consumes the Box. Once a packet is freed it can no longer be
// referenced, and hence can not me mutated once it has been returned to the
// freelist.
// NB: we std::mem::forget the Box p to inhibit Dropping of the packet once it
// is on the freelist. If a packet goes out of scope without being freed, the
// attempt to Drop it will trigger a panic (see Packet). Hence we ensure that
// all allocated packets are eventually freed.
fn free_internal(mut p: Box<Packet>) {
    if unsafe { FL.nfree } == FREELIST_SIZE { panic!("Packet freelist overflow"); }
    p.length = 0;
    unsafe { FL.list[FL.nfree] = &mut *p; } std::mem::forget(p);
    unsafe { FL.nfree += 1; }
}
pub fn free (p: Box<Packet>) {
    engine::add_frees();
    engine::add_freebytes(p.length as u64);
    // Calculate bits of physical capacity required for packet on 10GbE
    // Account for minimum data size and overhead of CRC and inter-packet gap
    engine::add_freebits((max(p.length as u64, 46) + 4 + 5) * 8);
    free_internal(p);
}

// pub fn debug() {
//    unsafe {
//        println!("FL.nfree: {}", FL.nfree);
//        println!("FL.list[FL.nfree].data[0]: {}",
//                 FL.list[FL.nfree-1].as_mut().unwrap().data[0]);
//    }
// }
