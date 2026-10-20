//! Parallel map for loading work that is a pure function of one item (decoding a texture, a
//! sound or a resource): the results come back in input order, so callers see exactly what a
//! sequential `map` gives. Native builds spread the items over the CPU cores with scoped
//! threads; wasm32 builds run them in order on the calling thread (wasm threads need
//! cross-origin isolation, see `docs/PERFORMANCE.md`).
//!
//! Never use this for work that draws from the game's `Random` or depends on the order in
//! which items are processed.

/// `items.iter().map(f).collect()`, with the calls to `f` run on several threads (native).
pub fn map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let workers = threads().min(items.len());
        if workers > 1 {
            return threaded(items, &f, workers);
        }
    }
    items.iter().map(f).collect()
}

/// `items.iter_mut().map(f).collect()` for disjoint parts of one buffer (for example bands
/// of rows of an image). Each thread takes a run of neighbouring items (native).
pub fn map_mut<T: Send, R: Send>(items: &mut [T], f: impl Fn(&mut T) -> R + Sync) -> Vec<R> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let workers = threads().min(items.len());
        if workers > 1 {
            let run = items.len().div_ceil(workers);
            let f = &f;
            return std::thread::scope(|s| {
                let mut parts = items.chunks_mut(run);
                let first = parts.next();
                let handles: Vec<_> = parts
                    .map(|part| s.spawn(move || part.iter_mut().map(f).collect::<Vec<R>>()))
                    .collect();
                let mut out: Vec<R> = first.into_iter().flatten().map(f).collect();
                for h in handles {
                    out.extend(h.join().unwrap_or_else(|e| std::panic::resume_unwind(e)));
                }
                out
            });
        }
    }
    items.iter_mut().map(f).collect()
}

/// Run `background` on another thread while `foreground` runs on this one (native); on
/// wasm32, `foreground` then `background`, in order. `background` must be a pure function of
/// what it reads, so that the order makes no difference.
pub fn join<A: Send, B>(
    background: impl FnOnce() -> A + Send,
    foreground: impl FnOnce() -> B,
) -> (A, B) {
    #[cfg(not(target_arch = "wasm32"))]
    if threads() > 1 {
        return std::thread::scope(|s| {
            let a = s.spawn(background);
            let b = foreground();
            (a.join().unwrap_or_else(|e| std::panic::resume_unwind(e)), b)
        });
    }
    let b = foreground();
    (background(), b)
}

/// Each worker takes the next unclaimed item until none are left; the results are put back
/// in input order.
#[cfg(not(target_arch = "wasm32"))]
fn threaded<T: Sync, R: Send>(
    items: &[T],
    f: &(impl Fn(&T) -> R + Sync),
    workers: usize,
) -> Vec<R> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let next = AtomicUsize::new(0);
    let work = || {
        let mut done = Vec::new();
        loop {
            let i = next.fetch_add(1, Ordering::Relaxed);
            let Some(item) = items.get(i) else {
                return done;
            };
            done.push((i, f(item)));
        }
    };
    let mut out: Vec<Option<R>> = (0..items.len()).map(|_| None).collect();
    std::thread::scope(|s| {
        let handles: Vec<_> = (1..workers).map(|_| s.spawn(work)).collect();
        let mut place = |done: Vec<(usize, R)>| {
            for (i, r) in done {
                out[i] = Some(r);
            }
        };
        place(work());
        for h in handles {
            // A panic in `f` on a worker is raised again here, as it would be in order.
            place(h.join().unwrap_or_else(|e| std::panic::resume_unwind(e)));
        }
    });
    out.into_iter()
        .map(|r| r.expect("every item mapped"))
        .collect()
}

/// Threads to use: `UNBANISHED_THREADS` if set (1 runs everything in order), else the number
/// of cores.
#[cfg(not(target_arch = "wasm32"))]
fn threads() -> usize {
    static THREADS: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *THREADS.get_or_init(|| {
        crate::env::var("THREADS")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|&t: &usize| t >= 1)
            .or_else(|| std::thread::available_parallelism().ok().map(|n| n.get()))
            .unwrap_or(1)
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn keeps_input_order() {
        let items: Vec<u64> = (0..1000).collect();
        let out = super::map(&items, |&i| i * i);
        assert_eq!(out, items.iter().map(|&i| i * i).collect::<Vec<_>>());
        assert!(super::map(&[] as &[u8], |&b| b).is_empty());
        assert_eq!(super::map(&[7u8], |&b| b + 1), vec![8]);
        assert_eq!(super::join(|| 1, || 2), (1, 2));
        let mut buf: Vec<u32> = (0..1000).collect();
        let mut parts: Vec<&mut [u32]> = buf.chunks_mut(7).collect();
        let sums = super::map_mut(&mut parts, |p| {
            p.iter_mut().for_each(|v| *v *= 2);
            p.iter().sum::<u32>()
        });
        let expect: Vec<u32> = (0..1000u32)
            .collect::<Vec<_>>()
            .chunks(7)
            .map(|c| c.iter().map(|v| v * 2).sum())
            .collect();
        assert_eq!(sums, expect);
        assert_eq!(buf, (0..1000).map(|v| v * 2).collect::<Vec<u32>>());
    }
}
