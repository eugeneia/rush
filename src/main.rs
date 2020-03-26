mod packet;
mod link;
mod engine;
mod config;
mod basic_apps;

fn main() {
    init();
    allocate();
    link();
    config();
    engine();
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
    println!("link: rxpackets={} rxbytes={} txpackets={} txbytes={} txdrop={}",
             r.rxpackets, r.rxbytes, r.txpackets, r.txbytes, r.txdrop);
    // Failing to drain the link would cause panic
}

fn config () {
    let mut c = config::new();
    println!("Created an empty configuration");
    config::app(&mut c, "source", &basic_apps::Source {size: 60});
    println!("Added an app");
    config::link(&mut c, "source.output -> sink.input");
    println!("Added an link");
}

fn engine() {
    let mut s = engine::init();
    println!("Initialized engine");
    let mut c = config::new();
    config::app(&mut c, "source", &basic_apps::Source {size: 60});
    config::app(&mut c, "sink", &basic_apps::Sink {});
    config::link(&mut c, "source.output -> sink.input");
    engine::configure(&mut s, &c);
    println!("Configured the app network: source(60).output -> sink.input");
    engine::breathe(&s);
    println!("Performed a single breath");
    { let output =
          s.link_table.get("source.output -> sink.input").unwrap().borrow();
      println!("link: rxpackets={} rxbytes={} txdrop={}",
               output.rxpackets, output.rxbytes, output.txdrop); }
    let mut c = c.clone();
    config::app(&mut c, "source", &basic_apps::Source {size: 120});
    engine::configure(&mut s, &c);
    println!("Cloned, mutated, and applied new configuration:");
    println!("source(120).output -> sink.input");
    engine::breathe(&s);
    println!("Performed a single breath");
    { let output =
          s.link_table.get("source.output -> sink.input").unwrap().borrow();
      println!("link: rxpackets={} rxbytes={} txdrop={}",
               output.rxpackets, output.rxbytes, output.txdrop); }
    let stats = engine::stats();
    println!("engine: frees={} freebytes={} freebits={}",
             stats.frees, stats.freebytes, stats.freebits);
}
