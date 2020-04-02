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
//   main(&EngineState, Options) - run the engine breathe loop
//   Options - engine breathe loop options
//   now() -> Instant - return current monotonic engine time
//   timeout(Duration) -> [()->bool] - make timer returning true after duration
//   report_load() - print load report
//   report_links() - print link statistics

use super::link;
use super::config;

use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use std::thread::sleep;
use std::cmp::min;

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

// Call this to “run snabb”.
pub fn main(state: &EngineState, options: Option<Options>) {
    let options = match options {
        Some(options) => options,
        None => Options{..Default::default()}
    };
    let mut done: Option<Box<dyn Fn(&EngineState, &EngineStats) -> bool>> =
        options.done;
    if let Some(duration) = options.duration {
        if done.is_some() { panic!("You can not have both 'duration' and 'done'"); }
        let deadline = timeout(duration);
        done = Some(Box::new(move |_, _| deadline()));
    }

    breathe(state);
    while match &done {
        Some(done) => !done(state, unsafe {&STATS}),
        None => true
    } {
        pace_breathing();
        breathe(state);
    }
    if !options.no_report {
        if options.report_load  { report_load(); }
        if options.report_links { report_links(state); }
    }

    unsafe { MONOTONIC_NOW = None; }
}

// Engine breathe loop Options
//
//  done: run the engine until predicate returns true
//  duration: run the engine for duration (mutually exclusive with 'done')
//  no_report: disable engine reporting before return
//  report_load: print a load report upon return
//  report_links: print summarized statistics for each link upon return
#[derive(Default)]
pub struct Options {
    pub done: Option<Box<dyn Fn(&EngineState, &EngineStats) -> bool>>,
    pub duration: Option<Duration>,
    pub no_report: bool,
    pub report_load: bool,
    pub report_links: bool
}

// Return current monotonic time.
// Can be used to drive timers in apps.
static mut MONOTONIC_NOW: Option<Instant> = None;
pub fn now() -> Instant {
    match unsafe { MONOTONIC_NOW } {
        Some(instant) => instant,
        None => Instant::now()
    }
}

// Make a closure which when called returns true after duration,
// and false otherwise.
pub fn timeout(duration: Duration) -> Box<dyn Fn() -> bool> {
    let deadline = now() + duration;
    Box::new(move || now() > deadline)
}

// Perform a single breath (inhale / exhale)
fn breathe(state: &EngineState) {
    unsafe { MONOTONIC_NOW = Some(Instant::now()); }
    for app in state.app_table.values() {
        app.app.pull(&app);
    }
    for app in state.app_table.values() {
        app.app.push(&app);
    }
    unsafe { STATS.breaths += 1; }
}

// Breathing regluation to reduce CPU usage when idle by calling sleep.
//
// Dynamic adjustment automatically scales the time to sleep between
// breaths from nothing up to MAXSLEEP (default: 100us). If packets
// are processed during a breath then the SLEEP period is halved, and
// if no packets are processed during a breath then the SLEEP interval
// is increased by one microsecond.
static mut LASTFREES: u64 = 0;
static mut SLEEP: u64 = 0;
const MAXSLEEP: u64 = 100;
fn pace_breathing() {
    unsafe {
        if LASTFREES == STATS.frees {
            SLEEP = min(SLEEP + 1, MAXSLEEP);
            sleep(Duration::from_micros(SLEEP));
        } else {
            SLEEP /= 2;
        }
        LASTFREES = STATS.frees;
    }
}

// Load reporting prints several metrics:
//   time  - period of time that the metrics were collected over
//   fps   - frees per second (how many calls to packet::free())
//   fpb   - frees per breath
//   bpp   - bytes per packet (average packet size)
//   sleep - usecs of sleep between breaths
static mut LASTLOADREPORT: Option<Instant> = None;
static mut REPORTEDFREES: u64 = 0;
static mut REPORTEDFREEBITS: u64 = 0;
static mut REPORTEDFREEBYTES: u64 = 0;
static mut REPORTEDBREATHS: u64 = 0;
pub fn report_load() {
    unsafe {
        let frees = STATS.frees;
        let freebits = STATS.freebits;
        let freebytes = STATS.freebytes;
        let breaths = STATS.breaths;
        if let Some(lastloadreport) = LASTLOADREPORT {
            let interval = now().duration_since(lastloadreport).as_secs_f64();
            let newfrees = frees - REPORTEDFREES;
            let newbits = freebits - REPORTEDFREEBITS;
            let newbytes = freebytes - REPORTEDFREEBYTES;
            let newbreaths = breaths - REPORTEDBREATHS;
            let fps = (newfrees as f64 / interval) as u64;
            let fbps = newbits as f64 / interval;
            let fpb = if newbreaths > 0 { newfrees / newbreaths } else { 0 };
            let bpp = if newfrees > 0 { newbytes / newfrees } else { 0 };
            println!("load: time: {:.2} fps: {} fpGbps: {:.3} fpb: {} bpp: {} sleep: {}",
                     interval,
                     fps,
                     fbps / 1e9,
                     fpb,
                     bpp,
                     SLEEP);
        }
        LASTLOADREPORT = Some(now());
        REPORTEDFREES = frees;
        REPORTEDFREEBITS = freebits;
        REPORTEDFREEBYTES = freebytes;
        REPORTEDBREATHS = breaths;
    }
}

// Print a link report (packets sent, percent dropped)
pub fn report_links(state: &EngineState) {
    let mut names: Vec<_> = state.link_table.keys().collect();
    names.sort();
    for name in names {
        let link = state.link_table.get(name).unwrap().borrow();
        let txpackets = link.txpackets;
        let txdrop = link.txdrop;
        println!("{} sent on {} (loss rate: {}%)",
                 txpackets, name, loss_rate(txdrop, txpackets));
    }
}

fn loss_rate(drop: u64, sent: u64) -> u64 {
    if sent == 0 { return 0; }
    drop * 100 / (drop + sent)
}
