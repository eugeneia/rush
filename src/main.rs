mod packet;
mod link;
mod engine;
mod config;
mod lib;
mod basic_apps;

use std::time::{Duration,Instant};

fn main() {
    init();
    allocate();
    link();
    config();
    let mut s = engine::init();
    println!("Initialized engine");
    engine(&mut s);
    breathe_order(&mut s);
    basic1(&mut s, 10_000_000);
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

fn engine(s: &mut engine::EngineState) {
    let mut c = config::new();
    config::app(&mut c, "source", &basic_apps::Source {size: 60});
    config::app(&mut c, "sink", &basic_apps::Sink {});
    config::link(&mut c, "source.output -> sink.input");
    engine::configure(s, &c);
    println!("Configured the app network: source(60).output -> sink.input");
    engine::main(&s, Some(engine::Options{
        duration: Some(Duration::new(0,0)),
        report_load: true, report_links: true,
        ..Default::default()
    }));
    let mut c = c.clone();
    config::app(&mut c, "source", &basic_apps::Source {size: 120});
    engine::configure(s, &c);
    println!("Cloned, mutated, and applied new configuration:");
    println!("source(120).output -> sink.input");
    engine::main(&s, Some(engine::Options{
        done: Some(Box::new(|_, _| true)),
        report_load: true, report_links: true,
        ..Default::default()
    }));
    let stats = engine::stats();
    println!("engine: frees={} freebytes={} freebits={}",
             stats.frees, stats.freebytes, stats.freebits);
}

fn breathe_order(s: &mut engine::EngineState) {
    println!("Case 1:");
    let mut c = config::new();
    config::app(&mut c, "a_io1", &basic_apps::SourceSink {size: 60});
    config::app(&mut c, "b_t1", &basic_apps::Tee {});
    config::app(&mut c, "c_t2", &basic_apps::Tee {});
    config::app(&mut c, "d_t3", &basic_apps::Tee {});
    config::link(&mut c, "a_io1.output -> b_t1.input");
    config::link(&mut c, "b_t1.output -> c_t2.input");
    config::link(&mut c, "b_t1.output2 -> d_t3.input");
    config::link(&mut c, "d_t3.output -> b_t1.input2");
    engine::configure(s, &c);
    engine::report_links(s);
    for name in &s.inhale { println!("pull {}", &name); }
    for name in &s.exhale { println!("push {}", &name); }
    println!("Case 2:");
    let mut c = config::new();
    config::app(&mut c, "a_io1", &basic_apps::SourceSink {size: 60});
    config::app(&mut c, "b_t1", &basic_apps::Tee {});
    config::app(&mut c, "c_t2", &basic_apps::Tee {});
    config::app(&mut c, "d_t3", &basic_apps::Tee {});
    config::link(&mut c, "a_io1.output -> b_t1.input");
    config::link(&mut c, "b_t1.output -> c_t2.input");
    config::link(&mut c, "b_t1.output2 -> d_t3.input");
    config::link(&mut c, "c_t2.output -> d_t3.input2");
    engine::configure(s, &c);
    engine::report_links(s);
    for name in &s.inhale { println!("pull {}", &name); }
    for name in &s.exhale { println!("push {}", &name); }
    println!("Case 3:");
    let mut c = config::new();
    config::app(&mut c, "a_io1", &basic_apps::SourceSink {size: 60});
    config::app(&mut c, "b_t1", &basic_apps::Tee {});
    config::app(&mut c, "c_t2", &basic_apps::Tee {});
    config::link(&mut c, "a_io1.output -> b_t1.input");
    config::link(&mut c, "a_io1.output2 -> c_t2.input");
    config::link(&mut c, "b_t1.output -> a_io1.input");
    config::link(&mut c, "b_t1.output2 -> c_t2.input2");
    config::link(&mut c, "c_t2.output -> a_io1.input2");
    engine::configure(s, &c);
    engine::report_links(s);
    for name in &s.inhale { println!("pull {}", &name); }
    for name in &s.exhale { println!("push {}", &name); }
}

fn basic1 (s: &mut engine::EngineState, npackets: u64) {
    let mut c = config::new();
    config::app(&mut c, "Source", &basic_apps::Source {size: 60});
    config::app(&mut c, "Tee", &basic_apps::Tee {});
    config::app(&mut c, "Sink", &basic_apps::Sink {});
    config::link(&mut c, "Source.tx -> Tee.rx");
    config::link(&mut c, "Tee.tx1 -> Sink.rx1");
    config::link(&mut c, "Tee.tx2 -> Sink.rx2");
    engine::configure(s, &c);
    let start = Instant::now();
    let output = s.app_table.get("Source").unwrap().output.get("tx").unwrap();
    while output.borrow().txpackets < npackets {
        engine::main(&s, Some(engine::Options{
            duration: Some(Duration::new(0, 10_000_000)), // 0.01s
            no_report: true,
            ..Default::default()
        }));
    }
    let finish = Instant::now();
    let runtime = finish.duration_since(start).as_secs_f64();
    let packets = output.borrow().txpackets as f64;
    println!("Processed {:.1} million packets in {:.2} seconds (rate: {:.1} Mpps).",
             packets / 1e6, runtime, packets / runtime / 1e6);
}
