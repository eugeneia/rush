mod packet;
mod link;

fn main() {
    init();
    allocate();
    link();
}

fn init() {
    packet::init();
    println!("Initialized packet freelist");
}

fn allocate() {
    let mut p = packet::allocate();
    println!("Allocated a packet of length {}", p.length);
    p.length = 1;
    p.data[0] = 42;
    //p.data[100000] = 99; // Would cause compile error
    println!("Mutating packet (length = {}, data[0] = {})",
             p.length, p.data[0]);
    let len = p.length;
    packet::free(p); // Not freeing would cause panic
    println!("Freed a packet of length {}", len);
    //p.length = 2; // Would cause compile error
}

fn link() {
    let mut r = link::new();
    println!("Allocated a link of capacity {}", link::LINK_MAX_PACKETS);
    let to_transmit = 2000;
    if link::full(&r) { panic!("Link should be empty."); }
    for n in 1..=to_transmit {
        let mut p = packet::allocate();
        p.length = n;
        p.data[(n-1) as usize] = 42;
        // Why is &, &mut not automatically inferred?
        link::transmit(&mut r, p);
        //p.data[0] = 13 // Would cause compiler error.
        //link::transmit(&mut r, p); // Would cause compile error
    }
    println!("Transmitted {} packets", to_transmit);
    if link::empty(&r) || !link::full(&r) { panic!("Link should be full."); }
    let mut n = 0;
    while !link::empty(&r) {
        n += 1;
        let p = link::receive(&mut r);
        if p.length != n as u16 || p.data[n-1] != 42 { panic!("Corrupt packet!"); }
        packet::free(p);
    }
    //link::receive(&mut r); // Would cause link underflow panic.
    println!("Received {} packets", n);
    // Failing to drain the link would cause panic
}
