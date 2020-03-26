mod packet;

fn main() {
    init();
    allocate();
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
