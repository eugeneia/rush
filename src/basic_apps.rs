use super::packet;
use super::link;
use super::engine;

// Source app: generate synthetic packets

#[derive(Debug)]
pub struct Source { pub size: u16 }
impl engine::AppConfig for Source {
    fn new(&self) -> Box<dyn engine::App> {
        Box::new(SourceApp {size: self.size})
    }
}
pub struct SourceApp { size: u16 }
impl engine::App for SourceApp {
    fn pull(&self, app: &engine::AppState) {
        for output in app.output.values() {
            let mut output = output.borrow_mut();
            for _ in 0..engine::PULL_NPACKETS {
                let mut p = packet::allocate();
                p.length = self.size;
                link::transmit(&mut output, p);
            }
        }
    }
}

// Sink app: Receive and discard packets

#[derive(Debug)]
pub struct Sink {}
impl engine::AppConfig for Sink {
    fn new(&self) -> Box<dyn engine::App> {
        Box::new(SinkApp {})
    }
}
pub struct SinkApp {}
impl engine::App for SinkApp {
    fn push(&self, app: &engine::AppState) {
        for input in app.input.values() {
            let mut input = input.borrow_mut();
            while !link::empty(&input) {
                packet::free(link::receive(&mut input));
            }
        }
    }
}
