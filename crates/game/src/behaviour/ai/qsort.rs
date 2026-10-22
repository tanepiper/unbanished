//! The C runtime's `qsort`, whose unstable order the game's sorting depends on.

use super::*;

/// The MSVC CRT `qsort` inlined for candidates, ordering by priority, highest first (comparator
/// `b.priority - a.priority`). Median of three, short sort (selection of the maximum) for 8 or
/// fewer elements, smaller partition first.
///
/// Original: `FUN_140030770`.
pub fn crt_qsort(a: &mut [Candidate]) {
    crt_qsort_by(a, |a: &Candidate, b: &Candidate| b.1 - a.1);
}

/// The Microsoft C runtime's `qsort` with a `less` comparator, as the game's templated copies use
/// it: the same algorithm as [`crt_qsort_by`], with `cmp(a, b) > 0` being `less(b, a)` and
/// equality neither way.
///
/// Original: `FUN_14002c9d0`, `FUN_140030c00`, `FUN_1400220f0`, `FUN_14001f120`, `FUN_140015c10`,
/// `FUN_140015f10`, `FUN_140016ce0`, `FUN_140016fe0`.
pub fn crt_qsort_less<T>(a: &mut [T], less: impl Fn(&T, &T) -> bool) {
    crt_qsort_by(a, |x, y| {
        if less(y, x) {
            1
        } else if less(x, y) {
            -1
        } else {
            0
        }
    });
}

/// The same with a three-way comparator.
pub fn crt_qsort_by<T>(a: &mut [T], cmp: impl Fn(&T, &T) -> i32) {
    const CUTOFF: usize = 8;
    if a.len() < 2 {
        return;
    }
    let mut stack: Vec<(usize, usize)> = Vec::new();
    let (mut lo, mut hi) = (0usize, a.len() - 1);
    loop {
        let size = hi - lo + 1;
        if size <= CUTOFF {
            // shortsort
            let mut h = hi;
            while h > lo {
                let mut max = lo;
                for p in lo + 1..=h {
                    if cmp(&a[p], &a[max]) > 0 {
                        max = p;
                    }
                }
                a.swap(max, h);
                h -= 1;
            }
        } else {
            let mut mid = lo + size / 2;
            if cmp(&a[lo], &a[mid]) > 0 {
                a.swap(lo, mid);
            }
            if cmp(&a[lo], &a[hi]) > 0 {
                a.swap(lo, hi);
            }
            if cmp(&a[mid], &a[hi]) > 0 {
                a.swap(mid, hi);
            }
            let (mut loguy, mut higuy) = (lo as isize, hi as isize);
            let m = |mid: usize| mid as isize;
            loop {
                if m(mid) > loguy {
                    loop {
                        loguy += 1;
                        if !(loguy < m(mid) && cmp(&a[loguy as usize], &a[mid]) <= 0) {
                            break;
                        }
                    }
                }
                if m(mid) <= loguy {
                    loop {
                        loguy += 1;
                        if !(loguy <= hi as isize && cmp(&a[loguy as usize], &a[mid]) <= 0) {
                            break;
                        }
                    }
                }
                loop {
                    higuy -= 1;
                    if !(higuy > m(mid) && cmp(&a[higuy as usize], &a[mid]) > 0) {
                        break;
                    }
                }
                if higuy < loguy {
                    break;
                }
                a.swap(loguy as usize, higuy as usize);
                if m(mid) == higuy {
                    mid = loguy as usize;
                }
            }
            higuy += 1;
            if m(mid) < higuy {
                loop {
                    higuy -= 1;
                    if !(higuy > m(mid) && cmp(&a[higuy as usize], &a[mid]) == 0) {
                        break;
                    }
                }
            }
            if m(mid) >= higuy {
                loop {
                    higuy -= 1;
                    if !(higuy > lo as isize && cmp(&a[higuy as usize], &a[mid]) == 0) {
                        break;
                    }
                }
            }
            if higuy - lo as isize >= hi as isize - loguy {
                if (lo as isize) < higuy {
                    stack.push((lo, higuy as usize));
                }
                if loguy < hi as isize {
                    lo = loguy as usize;
                    continue;
                }
            } else {
                if loguy < hi as isize {
                    stack.push((loguy as usize, hi));
                }
                if (lo as isize) < higuy {
                    hi = higuy as usize;
                    continue;
                }
            }
        }
        match stack.pop() {
            Some((l, h)) => {
                lo = l;
                hi = h;
            }
            None => return,
        }
    }
}
