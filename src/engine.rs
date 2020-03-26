// PACKET PROCESSING ENGINE
//
// This module implements configuration and execution of the packet processing
// engine.
//
//   EngineStats - struct containing global engine statistics
//   stats() -> EngineStats - get engine statistics
//   EngineState - struct representing engine state
//   init() -> EngineState - initialize engine (can only be called once)
//   SharedLink - type for shared links (between apps, also in EngineState)   
//   AppState - struct representing an app in the current app network
//   App, AppConfig - traits that defines an app, and its configuration
//   PULL_NPACKETS - number of packets to be inhaled in app’s pull() methods
//   configure(&mut EngineState, &config) - apply configuration to app network

use super::link;
use super::config;

use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::Rc;

// Counters for global engine statistics.
pub struct EngineStats {
    pub breaths: u64,  // Total breaths taken
    pub frees: u64,    // Total packets freed
    pub freebits: u64, // Total packet bits freed (for 10GbE)
    pub freebytes: u64 // Total packet bytes freed
}
static mut STATS: EngineStats = EngineStats {
    breaths: 0, frees: 0, freebits: 0, freebytes: 0
};
pub fn add_frees    ()           { unsafe { STATS.frees += 1 } }
pub fn add_freebytes(bytes: u64) { unsafe { STATS.freebytes += bytes; } }
pub fn add_freebits (bits: u64)  { unsafe { STATS.freebits += bits; } }
pub fn stats() -> &'static EngineStats { unsafe { &STATS } }

// Global engine state; singleton obtained via engine::init()
//
// The set of all active apps and links in the system, indexed by name.
pub struct EngineState<'state> {
    pub link_table: HashMap<String, SharedLink>,
    pub app_table: HashMap<String, AppState<'state>>
}
static mut INIT: bool = false;
pub fn init<'state>() -> EngineState<'state> {
    if unsafe { INIT } { panic!("Engine already initialized"); }
    unsafe { INIT = true; }
    EngineState {
        app_table: HashMap::new(),
        link_table: HashMap::new()
    }
}

// Type for links shared between apps.
//
// Links are borrowed at runtime by apps to perform packet I/O, or via the
// global engine state (to query link statistics etc.)
pub type SharedLink = Rc<RefCell<link::Link>>;

// State for a sigle app instance managed by the engine
//
// Tracks a reference to the AppConfig used to instantiate the app, and maps of
// its active input and output links.
pub struct AppState<'state> {
    pub app: Box<dyn App>,
    pub conf: &'state dyn AppArg,
    pub input: HashMap<String, SharedLink>,
    pub output: HashMap<String, SharedLink>
}

// Callbacks that can be implented by apps
//
//   pull: inhale packets into the app network (put them onto output links)
//   push: exhale packets out the the app network (move them from input links
//         to output links, or peripheral device queues)
//   stop: stop the app (deinitialize)
pub trait App {
    fn pull(&self, _app: &AppState) {}
    fn push(&self, _app: &AppState) {} // Exhale packets from apps.input
    fn stop(&self) {}
}
// Recommended number of packets to inhale in pull()
pub const PULL_NPACKETS: usize = link::LINK_MAX_PACKETS / 10;

// Constructor trait/callback for app instance specifications
//
//   new: initialize and return app (resulting app must implement App trait)
//
// Objects that implement the AppConfig trait can be used to configure apps
// via config::app().
pub trait AppConfig: std::fmt::Debug {
    fn new(&self) -> Box<dyn App>;
}

// Trait used internally by engine/config to provide an equality predicate for
// implementors of AppConfig. Sort of a hack based on the Debug trait.
//
// Auto-implemented for all implementors of AppConfig.
pub trait AppArg: AppConfig {
    fn identity(&self) -> String { format!("{}::{:?}", module_path!(), self) }
    fn equal(&self, y: &dyn AppArg) -> bool { self.identity() == y.identity() }
}
impl<T: AppConfig> AppArg for T { }


// Configure the running app network to match (new) config.
//
// Successive calls to configure() will migrate from the old to the
// new app network by making the changes needed.
pub fn configure<'state>(state: &mut EngineState<'state>,
                         config: &config::Config<'state>) {
    // First determine the links that are going away and remove them.
    for link in state.link_table.clone().keys() {
        if config.links.get(link).is_none() {
            unlink_apps(state, link)
        }
    }
    // Do the same for apps.
    let apps: Vec<_> = state.app_table.keys().map(Clone::clone).collect();
    for name in apps {
        let old = state.app_table.get(&name).unwrap().conf;
        match config.apps.get(&name) {
            Some(new) => if !old.equal(*new) { stop_app(state, &name) },
            None => stop_app(state, &name)
        }
    }
    // Start new apps.
    for (name, &arg) in config.apps.iter() {
        if state.app_table.get(name).is_none() {
            start_app(state, name, arg)
        }
    }
    // Rebuild links.
    for link in config.links.iter() {
        link_apps(state, link);
    }
}

// Insert new app instance into network.
fn start_app<'state>(state: &mut EngineState<'state>,
                     name: &str, conf: &'state dyn AppArg) {
    state.app_table.insert(name.to_string(),
                           AppState { app: conf.new(),
                                      conf: conf,
                                      input: HashMap::new(),
                                      output: HashMap::new() });
}

// Remove app instance from network.
fn stop_app (state: &mut EngineState, name: &str) {
    state.app_table.remove(name).unwrap().app.stop();
}

// Allocate a fresh shared link.
fn new_shared_link() -> SharedLink { Rc::new(RefCell::new(link::new())) }

// Link two apps in the network.
fn link_apps(state: &mut EngineState, spec: &str) {
    let link = state.link_table.entry(spec.to_string())
        .or_insert_with(new_shared_link);
    let spec = config::parse_link(spec);
    state.app_table.get_mut(&spec.from).unwrap()
        .output.insert(spec.output, link.clone());
    state.app_table.get_mut(&spec.to).unwrap()
        .input.insert(spec.input, link.clone());
}

// Remove link between two apps.
fn unlink_apps(state: &mut EngineState, spec: &str) {
    state.link_table.remove(spec);
    let spec = config::parse_link(spec);
    state.app_table.get_mut(&spec.from).unwrap()
        .output.remove(&spec.output);
    state.app_table.get_mut(&spec.to).unwrap()
        .input.remove(&spec.input);
}

// Perform a single breath (inhale / exhale)
pub fn breathe(state: &EngineState) {
    for app in state.app_table.values() {
        app.app.pull(&app);
    }
    for app in state.app_table.values() {
        app.app.push(&app);
    }
}
