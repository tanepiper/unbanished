//! The original's Loiter picks, measured in a run of the original, for comparing draws past them.
//!
//! `Activity::Loiter` picks one of the objects near a citizen from a set that the original keys
//! by each object's `Object::Internal` address (docs/ITERATION-ORDER.md). The port keeps the set
//! in creation order, so the same draw can pick another object. A comparison run can give the
//! map rectangle of each object the original picked, in order; each pick then takes the set
//! member with that rectangle.
//!
//! Deviation: not in the original; a check run from the command line (`--loiter-picks`).

use super::activities::MapRect;
use std::cell::RefCell;
use std::collections::VecDeque;

thread_local! {
    static PICKS: RefCell<Option<VecDeque<MapRect>>> = const { RefCell::new(None) };
}

/// Take the original's picks on this thread, in the order the original made them.
pub fn set(picks: impl IntoIterator<Item = MapRect>) {
    PICKS.with(|p| *p.borrow_mut() = Some(picks.into_iter().collect()));
}

/// The place in the set to take: the member with the original's next rectangle, or `k` (the
/// drawn place) when no picks were given, they ran out, or no member has that rectangle.
pub(crate) fn place(k: usize, rects: impl Iterator<Item = Option<MapRect>>) -> usize {
    let Some(want) = PICKS.with(|p| p.borrow_mut().as_mut().and_then(VecDeque::pop_front)) else {
        return k;
    };
    let mut rects = rects;
    rects.position(|r| r == Some(want)).unwrap_or_else(|| {
        eprintln!("no object near the citizen has the original's pick {want:?}");
        k
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With picks given, each takes the member with its rectangle in turn, then the drawn place.
    #[test]
    fn picks_take_the_member_with_their_rectangle() {
        let rects = [Some((1, 1, 2, 2)), None, Some((5, 5, 1, 1))];
        assert_eq!(place(0, rects.iter().copied()), 0);
        set([(5, 5, 1, 1), (9, 9, 1, 1)]);
        assert_eq!(place(0, rects.iter().copied()), 2);
        assert_eq!(place(1, rects.iter().copied()), 1, "no member has it");
        assert_eq!(place(1, rects.iter().copied()), 1, "the picks ran out");
    }
}
