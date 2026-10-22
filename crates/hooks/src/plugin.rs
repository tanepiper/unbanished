//! One rule-hook plugin: a WebAssembly module, checked when it is loaded and run with a fixed
//! budget per call.

use unbanished_game::random::Random;
use unbanished_sim::hooks::HookContext;
use wasmi::{Config, Engine, Instance, Linker, Module, Store, StoreLimitsBuilder, TypedFunc};

use crate::host::{self, Host};

/// The API version this game gives plugins.
pub const API_VERSION: i32 = 1;

/// The fuel (WebAssembly instructions, roughly) one call may use. A plugin that runs out stops
/// at the same instruction on every computer, and is switched off.
pub const FUEL: u64 = 2_000_000;

/// The most memory a plugin may have.
const MEMORY: usize = 16 << 20;

/// A loaded plugin.
pub struct Plugin {
    /// Its name (the file name without `.wasm`).
    pub name: String,
    store: Store<Host>,
    after_iteration: Option<TypedFunc<(), ()>>,
    /// Switched off after it failed (a trap, no fuel left).
    pub stopped: bool,
}

impl Plugin {
    /// Loads a plugin from its bytes. It must use no floating point (it would not give the same
    /// results on every computer), import only API version 1's functions, and say it was made
    /// for API version 1.
    pub fn load(name: &str, wasm: &[u8]) -> Result<Plugin, String> {
        let mut config = Config::default();
        config.floats(false).consume_fuel(true);
        let engine = Engine::new(&config);
        let module = Module::new(&engine, wasm).map_err(|e| {
            let text = e.to_string();
            if text.contains("float") {
                format!("{name}: uses floating point, which rule hooks may not ({text})")
            } else {
                format!("{name}: not a valid WebAssembly module ({text})")
            }
        })?;
        let host = Host {
            year: 0,
            month: 0,
            time_in_year: 0,
            citizens: 0,
            stock: Vec::new(),
            rng: Random::new(0),
            events: Vec::new(),
            limits: StoreLimitsBuilder::new().memory_size(MEMORY).build(),
        };
        let mut store = Store::new(&engine, host);
        store.limiter(|h| &mut h.limits);
        let mut linker = Linker::new(&engine);
        host::link(&mut linker).map_err(|e| e.to_string())?;
        store.set_fuel(FUEL).map_err(|e| e.to_string())?;
        let instance: Instance =
            linker
                .instantiate_and_start(&mut store, &module)
                .map_err(|e| {
                    format!("{name}: {e} (a rule hook may import only `unbanished` functions)")
                })?;
        let version = instance
            .get_typed_func::<(), i32>(&store, "api_version")
            .map_err(|_| format!("{name}: exports no `api_version`"))?
            .call(&mut store, ())
            .map_err(|e| format!("{name}: api_version: {e}"))?;
        if version != API_VERSION {
            return Err(format!(
                "{name}: made for rule-hook API {version}; this game gives API {API_VERSION}"
            ));
        }
        let after_iteration = instance
            .get_typed_func::<(), ()>(&store, "after_iteration")
            .ok();
        Ok(Plugin {
            name: name.to_string(),
            store,
            after_iteration,
            stopped: false,
        })
    }

    /// Runs the plugin's `after_iteration`, if it has one.
    pub fn after_iteration(&mut self, cx: &mut HookContext<'_>) {
        let Some(f) = self.after_iteration else {
            return;
        };
        if self.stopped {
            return;
        }
        let h = self.store.data_mut();
        h.year = cx.year();
        h.month = cx.month();
        h.time_in_year = cx.time_in_year();
        h.citizens = cx.citizens();
        h.stock = (0..cx.stock_slots()).map(|s| cx.stock(s)).collect();
        cx.swap_random(&mut h.rng);
        let result = self
            .store
            .set_fuel(FUEL)
            .and_then(|_| f.call(&mut self.store, ()));
        let h = self.store.data_mut();
        cx.swap_random(&mut h.rng);
        for (severity, text) in std::mem::take(&mut h.events) {
            cx.post_event(severity, &text);
        }
        if let Err(e) = result {
            log::error!("rule hook {}: {e}; it is switched off", self.name);
            self.stopped = true;
        }
    }
}
