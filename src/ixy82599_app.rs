use super::engine;
use super::ixy82599;
use std::cell::RefCell;

// Ixy82599 app: drive an Intel 82599 network adapter

#[derive(Clone,Debug)]
pub struct Ixy82599 { pub pci: String }
impl engine::AppConfig for Ixy82599 {
    fn new(&self) -> Box<dyn engine::App> {
        let ixy = ixy82599::ixy_init(&self.pci, 1, 1, 0).unwrap();
        Box::new(Ixy82599App {ixy: RefCell::new(ixy)})
    }
}
pub struct Ixy82599App { ixy: RefCell<Box<dyn ixy82599::IxyDevice>> }
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
    fn has_stop(&self) -> bool { true }
    fn stop(&self) { panic!("NYI"); }
}
