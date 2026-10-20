//! A hook for timing the parts of a step (not in the original). The game app sets it to its
//! frame profiler; with nothing set, a scope costs one check. It only reads the clock, so it
//! never changes the simulation.

use std::any::Any;
use std::cell::Cell;

/// Starts timing a named part; the part ends when the returned value is dropped.
pub type Timer = fn(&'static str) -> Box<dyn Any>;

thread_local! {
    static TIMER: Cell<Option<Timer>> = const { Cell::new(None) };
}

/// Times every later scope on this thread with `timer`.
pub fn set(timer: Timer) {
    TIMER.with(|t| t.set(Some(timer)));
}

/// Times the part named `name` until the result is dropped (nothing when no timer is set).
pub fn scope(name: &'static str) -> Option<Box<dyn Any>> {
    TIMER.with(|t| t.get()).map(|timer| timer(name))
}
