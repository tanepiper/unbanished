//! Where the original's float jumps send a NaN (issue #25), for the functions whose NaN
//! branch the port has followed. Each case is read from the machine code (*observed*): an
//! unordered `comiss` sets carry, zero and parity, so `jb`, `jbe` and `je` are taken for a
//! NaN. `tools/triage/nan_jumps.py --hazards ADDR` lists a function's marked tests.

use unbanished_game::crt::truncate;
use unbanished_game::nature::growth::adjust_map_byte;

/// `FUN_140125940`: a NaN adjustment takes the multiplying branch (`jbe` at 0x1401259c5), and
/// `cvttss2si` turns the NaN, or a value out of range, into a negative number, so the byte
/// becomes 0.
#[test]
fn a_nan_or_huge_map_adjustment_clears_the_byte() {
    assert_eq!(adjust_map_byte(128, f32::NAN), 0);
    assert_eq!(adjust_map_byte(255, 1e10), 0);
    // In range, the two branches are unchanged.
    assert_eq!(adjust_map_byte(255, 0.5), 127);
    assert_eq!(adjust_map_byte(0, -0.5), 127);
}

#[test]
fn truncate_is_cvttss2si() {
    assert_eq!(truncate(2.9), 2);
    assert_eq!(truncate(-2.9), -2);
    assert_eq!(truncate(f32::NAN), i32::MIN);
    assert_eq!(truncate(f32::INFINITY), i32::MIN);
    assert_eq!(truncate(-3e9), i32::MIN);
    assert_eq!(truncate(2_147_483_520.0), 2_147_483_520);
}

/// `FUN_14007eda0`: a NaN age counts as a child's (`jb` at 0x14007ee61), in school as a
/// student's.
#[test]
fn a_nan_age_counts_as_a_child() {
    use unbanished_game::people::status::{aggregate, CitizenValue};
    let citizen = |age: f32, school: bool| CitizenValue {
        age,
        adult_age: 10.0,
        school,
        ..Default::default()
    };
    let s = aggregate([
        citizen(f32::NAN, false),
        citizen(f32::NAN, true),
        citizen(20.0, false),
    ]);
    assert_eq!((s.adults, s.students, s.children), (1, 1, 1));
}
