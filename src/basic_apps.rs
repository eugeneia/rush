use super::packet;
use super::link;
use super::engine;
use super::lib;

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
    fn has_pull(&self) -> bool { true }
    fn pull(&self, app: &engine::AppState) {
        for output in app.output.values() {
            let mut output = output.borrow_mut();
            for _ in 0..engine::PULL_NPACKETS {
                let mut p = packet::allocate();
                lib::fill(&mut p.data, self.size as usize, 0);
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
    fn has_push(&self) -> bool { true }
    fn push(&self, app: &engine::AppState) {
        for input in app.input.values() {
            let mut input = input.borrow_mut();
            while !link::empty(&input) {
                packet::free(link::receive(&mut input));
            }
        }
    }
}

// Tee app: Send inputs to all outputs

#[derive(Debug)]
pub struct Tee {}
impl engine::AppConfig for Tee {
    fn new(&self) -> Box<dyn engine::App> {
        Box::new(TeeApp {})
    }
}
pub struct TeeApp {}
impl engine::App for TeeApp {
    fn has_push(&self) -> bool { true }
    fn push(&self, app: &engine::AppState) {
        //let noutputs = app.output.len();
        for input in app.input.values() {
            let mut input = input.borrow_mut();
            while !link::empty(&input) {
                let p = link::receive(&mut input);
                //let mut outn = 0;
                for output in app.output.values() {
                    let mut output = output.borrow_mut();
                    //outn += 1;
                    link::transmit(&mut output, packet::clone(&p));
                    //if outn == noutputs { packet::clone(&p) } else { p }
                }
                packet::free(p);
            }
        }
    }
}

// SourceSink app: pseudo I/O device

#[derive(Debug)]
pub struct SourceSink { pub size: u16 }
impl engine::AppConfig for SourceSink {
    fn new(&self) -> Box<dyn engine::App> {
        Box::new(SourceSinkApp {size: self.size})
    }
}
pub struct SourceSinkApp { size: u16 }
impl engine::App for SourceSinkApp {
    fn has_pull(&self) -> bool { true }
    fn pull(&self, app: &engine::AppState) {
        for output in app.output.values() {
            let mut output = output.borrow_mut();
            for _ in 0..engine::PULL_NPACKETS {
                let mut p = packet::allocate();
                lib::fill(&mut p.data, self.size as usize, 0);
                p.length = self.size;
                link::transmit(&mut output, p);
            }
        }
    }
    fn has_push(&self) -> bool { true }
    fn push(&self, app: &engine::AppState) {
        for input in app.input.values() {
            let mut input = input.borrow_mut();
            while !link::empty(&input) {
                packet::free(link::receive(&mut input));
            }
        }
    }
}
