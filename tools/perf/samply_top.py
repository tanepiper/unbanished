"""Summarise a samply profile: the functions with the most self and inclusive time.

Usage:
  samply record --save-only --unstable-presymbolicate -o OUT.json.gz -- ./target/profiling/release/unbanished ...
  python3 tools/perf/samply_top.py OUT.json.gz [--thread unbanished] [--after SECONDS] [--before SECONDS]
                                    [--top 40] [--grep TEXT] [--callers FUNCTION]

`--after`/`--before` keep the samples in a time window (seconds from the first sample), for
example to skip loading. `--grep` keeps only stacks that pass through a function containing
TEXT. `--callers` lists who calls a function, by time. The script reads the `.syms.json` file
that `--unstable-presymbolicate` writes next to the profile. Build with line tables for useful
names: `CARGO_TARGET_DIR=target/profiling CARGO_PROFILE_RELEASE_DEBUG=line-tables-only
cargo build --release -p unbanished-app`.
"""
import argparse
import bisect
import collections
import gzip
import json
import re


def load(path):
    opener = gzip.open if path.endswith(".gz") else open
    with opener(path, "rt") as f:
        profile = json.load(f)
    syms_path = re.sub(r"\.gz$", "", path) + ".syms.json"
    with open(syms_path) as f:
        syms = json.load(f)
    return profile, syms


def symbolizer(profile, syms):
    strings = syms["string_table"]
    tables = {}
    for lib in syms["data"]:
        table = sorted(lib["symbol_table"], key=lambda e: e["rva"])
        tables[lib["debug_id"].replace("-", "").upper()] = (
            [e["rva"] for e in table],
            table,
        )
    libs = []
    for lib in profile["libs"]:
        key = lib["breakpadId"][:-1].upper()
        libs.append((lib["name"], tables.get(key)))

    def name(lib_index, rva):
        if lib_index is None or lib_index >= len(libs):
            return f"0x{rva:x}"
        lib_name, table = libs[lib_index]
        if not table:
            return f"{lib_name}+0x{rva:x}"
        starts, entries = table
        i = bisect.bisect_right(starts, rva) - 1
        if i < 0:
            return f"{lib_name}+0x{rva:x}"
        e = entries[i]
        if rva >= e["rva"] + max(e["size"], 1) and e["size"]:
            return f"{lib_name}+0x{rva:x}"
        return strings[e["symbol"]]

    return name


def clean(name):
    """Shorten a demangled Rust name: drop hashes and generic noise."""
    name = re.sub(r"::h[0-9a-f]{16}$", "", name)
    name = re.sub(r"\[[0-9a-f]{16}\]", "", name)
    return name


def stacks(profile, thread, name):
    t = thread
    frames, funcs, stack_table = t["frameTable"], t["funcTable"], t["stackTable"]
    resources = t["resourceTable"]
    cache = {}

    def frame_name(f):
        if f in cache:
            return cache[f]
        func = frames["func"][f]
        res = funcs["resource"][func]
        lib = resources["lib"][res] if res is not None and res >= 0 else None
        addr = frames["address"][f]
        n = clean(name(lib, addr)) if addr is not None and addr >= 0 else t["stringArray"][funcs["name"][func]]
        cache[f] = n
        return n

    stack_cache = {}

    def frames_of(s):
        if s in stack_cache:
            return stack_cache[s]
        out = []
        while s is not None:
            out.append(frame_name(stack_table["frame"][s]))
            s = stack_table["prefix"][s]
        stack_cache[s] = out
        return out  # leaf first

    samples = t["samples"]
    times = samples.get("time")
    if times is None:
        times, acc = [], 0.0
        for d in samples["timeDeltas"]:
            acc += d
            times.append(acc)
    weights = samples.get("weight") or [1] * samples["length"]
    for s, ms, w in zip(samples["stack"], times, weights):
        if s is None:
            continue
        yield ms, frames_of(s), w


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("profile")
    ap.add_argument("--thread", default="unbanished")
    ap.add_argument("--after", type=float, default=0.0)
    ap.add_argument("--before", type=float, default=1e18)
    ap.add_argument("--top", type=int, default=40)
    ap.add_argument("--grep")
    ap.add_argument("--callers")
    args = ap.parse_args()
    profile, syms = load(args.profile)
    name = symbolizer(profile, syms)
    thread = next(t for t in profile["threads"] if t["name"] == args.thread)
    self_t = collections.Counter()
    incl = collections.Counter()
    callers = collections.Counter()
    total = 0
    first = None
    for ms, frames, w in stacks(profile, thread, name):
        first = ms if first is None else first
        sec = (ms - first) / 1000.0
        if not (args.after <= sec < args.before):
            continue
        if args.grep and not any(args.grep in f for f in frames):
            continue
        total += w
        self_t[frames[0]] += w
        for f in set(frames):
            incl[f] += w
        if args.callers:
            for i, f in enumerate(frames):
                if args.callers in f and i + 1 < len(frames):
                    callers[frames[i + 1]] += w
                    break
    if not total:
        print("no samples in the window")
        return
    interval = profile["meta"].get("interval", 1.0)
    print(f"{total} samples (~{total * interval / 1000:.1f} s at {interval} ms)")
    print("\nself time:")
    for f, n in self_t.most_common(args.top):
        print(f"  {100 * n / total:5.1f}%  {f[:150]}")
    print("\ninclusive time:")
    for f, n in incl.most_common(args.top):
        print(f"  {100 * n / total:5.1f}%  {f[:150]}")
    if args.callers:
        print(f"\ncallers of {args.callers}:")
        for f, n in callers.most_common(args.top):
            print(f"  {100 * n / total:5.1f}%  {f[:150]}")


if __name__ == "__main__":
    main()
