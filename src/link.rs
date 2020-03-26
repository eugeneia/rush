// LINK STRUCT AND OPERATIONS
//
// This module defines a struct to represent unidirectional network links,
// implemented as circular ring buffers, and link operations.
//
//   Link - opaque link structure
//   LINK_MAX_PACKETS - capacity of a Link
//   new() -> Link - allocate a new empty Link
//   full(&Link) -> bool - predicate to test if Link is full
//   empty(&Link) -> bool - predicate to test if Link is empty
//   receive(&mut Link) -> Box<Packet> - dequeue a packet from the Link
//   transmit(&mut Link, Box<Packet>) - enqueue a packet on the Link

use super::packet;

// Size of the ring buffer.
const LINK_RING_SIZE: usize = 1024;

// Capacity of a Link.
pub const LINK_MAX_PACKETS: usize = LINK_RING_SIZE - 1;

pub struct Link {
    // this is a circular ring buffer, as described at:
    //   http://en.wikipedia.org/wiki/Circular_buffer
    packets: [*mut packet::Packet; LINK_RING_SIZE],
    // Two cursors:
    //   read:  the next element to be read
    //   write: the next element to be written
    read: i32, write: i32,
    // Link stats:
    pub txpackets: u64, pub txbytes: u64, pub txdrop: u64,
    pub rxpackets: u64, pub rxbytes: u64
}

const SIZE: i32 = LINK_RING_SIZE as i32; // shorthand

pub fn new() -> Link {
    Link { packets: [std::ptr::null_mut(); LINK_RING_SIZE],
           read: 0, write: 0,
           txpackets: 0, txbytes: 0, txdrop: 0,
           rxpackets: 0, rxbytes: 0 }
}

pub fn empty(r: &Link) -> bool { r.read == r.write }

pub fn full(r: &Link) -> bool { (r.write + 1) & (SIZE - 1) == r.read }

// NB: non-empty assertion commented out in original Snabb, but since we get a
// bunch of nice safety invariants from the Rust compiler, let’s maintain them.
// Box::from_raw will never alias because receive/transmit ensure any Packet is
// either on a single Link, or on no Link at all.
pub fn receive(r: &mut Link) -> Box<packet::Packet> {
    if empty(r) { panic!("Link underflow."); }
    let p = unsafe { Box::from_raw(r.packets[r.read as usize]) };
    r.read = (r.read + 1) & (SIZE - 1);
    r.rxpackets += 1;
    r.rxbytes += p.length as u64;
    p
}

pub fn transmit(r: &mut Link, mut p: Box<packet::Packet>) {
    if full(r) {
        r.txdrop += 1;
        packet::free(p);
    } else {
        r.txpackets += 1;
        r.txbytes += p.length as u64;
        r.packets[r.write as usize] = &mut *p; std::mem::forget(p);
        r.write = (r.write + 1) & (SIZE - 1);
    }
}

// Ensure that Dropped Links are empty (otherwise Dropping a link would leak
// its remaining enqueued packets).
// NB: a non-empty Link going out of scope will trigger a panic.
impl Drop for Link {
    fn drop(&mut self) {
        if !empty(self) { panic!("Link is not empty."); }
    }
}
