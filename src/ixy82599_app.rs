use super::engine;
use super::lib;
use super::ixy82599;

use std::cell::RefCell;

// Ixy82599 app: drive an Intel 82599 network adapter

#[derive(Clone,Debug)]
pub struct Ixy82599 { pub pci: String }
impl engine::AppConfig for Ixy82599 {
    fn new(&self) -> Box<dyn engine::App> {
        assert!(unsafe { libc::getuid() } == 0,
                "Need to be root to drive PCI devices");
        let ixy = ixy82599::ixy_init(&self.pci, 1, 1, 0).unwrap();
        Box::new(Ixy82599App {
            ixy: RefCell::new(ixy),
            stats: RefCell::new(Default::default())
        })
    }
}
pub struct Ixy82599App {
    ixy: RefCell<Box<dyn ixy82599::IxyDevice>>,
    stats: RefCell<Box<ixy82599::DeviceStats>>
}
impl engine::App for Ixy82599App {
    fn has_pull(&self) -> bool { true }
    fn pull(&self, app: &engine::AppState) {
        if let Some(output) = app.output.get("output") {
            let mut output = output.borrow_mut();
            let mut ixy = self.ixy.borrow_mut();
            ixy.rx_batch(0, &mut output, engine::PULL_NPACKETS);
        }
    }
    fn has_push(&self) -> bool { true }
    fn push(&self, app: &engine::AppState) {
        if let Some(input) = app.input.get("input") {
            let mut input = input.borrow_mut();
            let mut ixy = self.ixy.borrow_mut();
            ixy.tx_batch(0, &mut input);
        }
    }
    fn has_report(&self) -> bool { true }
    fn report(&self) {
        let ixy = self.ixy.borrow_mut();
        let mut stats = self.stats.borrow_mut();
        let last_rx_pkts = stats.rx_pkts;
        let last_rx_bytes = stats.rx_bytes;
        let last_tx_pkts = stats.tx_pkts;
        let last_tx_bytes = stats.tx_bytes;
        ixy.read_stats(&mut stats);
        println!("  Device stats for {} since last report:",
                 ixy.get_pci_addr());
        println!("     rxpackets:\t{:10}", lib::comma_value(stats.rx_pkts - last_rx_pkts));
        println!("     rxbytes:\t{:10}", lib::comma_value(stats.rx_bytes - last_rx_bytes));
        println!("     txpackets:\t{:10}", lib::comma_value(stats.tx_pkts - last_tx_pkts));
        println!("     txbytes:\t{:10}", lib::comma_value(stats.tx_bytes - last_tx_bytes));
    }
    fn has_stop(&self) -> bool { true }
    fn stop(&self) { panic!("NYI"); }
}

#[cfg(test)]
mod selftest {
    use super::*;
    use crate::packet;
    use crate::link;
    use crate::config;
    use crate::engine;
    use crate::basic_apps;
    use crate::header;
    use crate::ethernet;
    use crate::ethernet::Ethernet;

    use std::time::Duration;

    #[test]
    fn ixy_send_recv() {
        let nic0 = if let Ok(pci) = std::env::var("RUSH_INTEL10G0") { pci }
        else { println!("Skipping test (need RUSH_INTEL10G0)");
               return };
        let nic1 = if let Ok(pci) = std::env::var("RUSH_INTEL10G1") { pci }
        else { println!("Skipping test (need RUSH_INTEL10G1)");
               return };
        if unsafe { libc::getuid() } != 0 {
            println!("Skipping test (need to be root)");
            return
        }

        let mut c = config::new();
        config::app(&mut c, "nic0", &Ixy82599 {pci: nic0});
        config::app(&mut c, "nic1", &Ixy82599 {pci: nic1});
        config::app(&mut c, "source", &PacketGen {
            dst: String::from("52:54:00:00:00:01"),
            src: String::from("52:54:00:00:00:02"),
            size: 60
        });
        config::app(&mut c, "sink", &basic_apps::Sink {});
        config::link(&mut c, "source.output -> nic0.input");
        config::link(&mut c, "nic1.output -> sink.input");
        engine::configure(&c);
        for _ in 0..3 {
            engine::main(Some(engine::Options {
                duration: Some(Duration::new(1, 0)),
                report_load: true,
                report_apps: true,
                report_links: true,
                ..Default::default()
            }));
        }
    }

    #[derive(Clone,Debug)]
    pub struct PacketGen { pub dst: String, src: String, size: u16 }
    impl engine::AppConfig for PacketGen {
        fn new(&self) -> Box<dyn engine::App> {
            let mut p = packet::allocate();
            p.length = self.size;
            let mut eth = header::from_mem::<Ethernet>(&mut p.data);
            eth.set_dst(&ethernet::pton(&self.dst));
            eth.set_src(&ethernet::pton(&self.src));
            eth.set_ethertype(self.size - header::size_of::<Ethernet>() as u16);
            Box::new(PacketGenApp {packet: p})
        }
    }
    pub struct PacketGenApp { packet: Box<packet::Packet> }
    impl engine::App for PacketGenApp {
        fn has_pull(&self) -> bool { true }
        fn pull(&self, app: &engine::AppState) {
            if let Some(output) = app.output.get("output") {
                let mut output = output.borrow_mut();
                while !link::full(&output) {
                    link::transmit(&mut output, packet::clone(&self.packet));
                }
            }
        }
    }

}
