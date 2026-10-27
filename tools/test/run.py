#!/usr/bin/env python3
"""Run Un-Banished's test suites, in tiers, with known outcomes.

    tools/test/run.py                    # the quick tier (the default): a few minutes
    tools/test/run.py standard           # quick, then determinism and the browser suites
    tools/test/run.py long               # everything, including version and network checks
    tools/test/run.py probe browser      # only these suites
    tools/test/run.py --list             # the suites, their tiers and what they check
    tools/test/run.py probe --update-baselines   # record new probe baselines after a change

Every suite runs the same way every time: fixed seeds, fixed window and page sizes, throwaway
settings, saves and browser profiles, and waits on the game's own state rather than on time.
A suite that needs something this computer lacks (game data, Chrome, Firefox) says so and is
skipped, not failed. The summary is printed at the end and written to
target/test-results/summary.json. Exit code 1 when any suite failed.

docs/develop/TESTING.md explains each suite and how to add to them.
"""

from __future__ import annotations

import argparse
import difflib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable

REPO = Path(__file__).resolve().parents[2]
# No test is ever heard: every game and tool started from here keeps its sound at volume 0.
os.environ["UNBANISHED_MUTE"] = "1"
RESULTS = REPO / "target" / "test-results"
WINDATA = Path(os.environ.get("UB_WINDATA", REPO / "game-data" / "original" / "WinData"))
WEB_DIST = REPO / "target" / "test-web" / "game"
BASELINES = REPO / "tools" / "test" / "baselines"
GAME = REPO / "target" / "release" / "unbanished"


class Skip(Exception):
    """A suite cannot run on this computer; the message says why."""


@dataclass
class Suite:
    name: str
    tier: str
    about: str
    run: Callable[["Context"], None]


@dataclass
class Context:
    update_baselines: bool = False
    verbose: bool = False
    notes: list[str] = field(default_factory=list)


def sh(cmd: list[str] | str, *, env: dict | None = None, cwd: Path = REPO, verbose: bool = False,
       check: bool = True, capture: bool = True) -> subprocess.CompletedProcess:
    """Run a command; on failure raise with the end of its output."""
    shell = isinstance(cmd, str)
    proc = subprocess.run(cmd, shell=shell, cwd=cwd, env={**os.environ, **(env or {})},
                          text=True, capture_output=capture and not verbose)
    if check and proc.returncode != 0:
        tail = ((proc.stdout or "") + (proc.stderr or "")).strip().splitlines()[-25:]
        raise RuntimeError(f"`{cmd if shell else ' '.join(cmd)}` exited with {proc.returncode}\n"
                           + "\n".join("      " + l for l in tail))
    return proc


def have_game_data() -> bool:
    return all((WINDATA / f).exists() for f in ("data0.pkg", "data1.pkg", "patch0.pkg", "patch1.pkg"))


def need_game_data() -> None:
    if not have_game_data():
        raise Skip(f"no game data in {WINDATA} (run tools/extract-assets.sh)")


def build_game(ctx: Context) -> None:
    sh(["cargo", "build", "-q", "--offline", "--release", "-p", "unbanished-app", "--bin", "unbanished"],
       verbose=ctx.verbose)


# --------------------------------------------------------------------------------------------
# Quick tier


def suite_checks(ctx: Context) -> None:
    """Formatting of changed Rust files, file sizes, the deviation ledger and the doc comments."""
    # No Rust file may pass 500 lines; the larger files listed may shrink, never grow.
    size = subprocess.run([sys.executable, "tools/test/file_size.py"], cwd=REPO, capture_output=True, text=True)
    if size.returncode != 0:
        raise RuntimeError("files too large (tools/test/file_size.py):\n" + size.stdout)
    changed = sh("git diff --name-only HEAD -- '*.rs'; git ls-files --others --exclude-standard -- '*.rs'"
                 ).stdout.split()
    changed = [f for f in changed if (REPO / f).exists()]
    if changed:
        sh(["rustfmt", "--edition", "2021", "--check", *changed])
    before = (REPO / "docs/DEVIATIONS.md").read_text()
    sh([sys.executable, "tools/triage/deviations.py"])
    if (REPO / "docs/DEVIATIONS.md").read_text() != before:
        raise RuntimeError("docs/DEVIATIONS.md was out of date; it is updated now: stage it")
    sh([sys.executable, "tools/triage/doc_comments.py", "check"])
    # The labels made from the port's citations stay current (tools/triage/cite_labels.py).
    # The class registrations' labels in names.tsv follow docs/GAME-TYPE-REGISTRATIONS.tsv; the
    # check needs the local Ghidra export.
    if (REPO / "analysis/export/Application-steam-x64.exe/functions.tsv").exists():
        sh([sys.executable, "tools/triage/type_labels.py", "--check"])
    port_labels = REPO / "labels/names-from-port.tsv"
    before = port_labels.read_text() if port_labels.exists() else ""
    # --notes: module notes name their module. --any --prefix FUN_: a `FUN_` address in the comment
    # directly above a function or a type names it too, below any "Original:" citation.
    sh([sys.executable, "tools/triage/cite_labels.py", ".", "--existing", "labels/names.tsv", "--no-lines",
        "--notes", "--any", "--prefix", "FUN_", "--out", str(port_labels)])
    if port_labels.read_text() != before:
        raise RuntimeError("labels/names-from-port.tsv was out of date; it is updated now: stage it")
    # The core depends on no drawing, window, interface, sound or network crate.
    tree = sh(["cargo", "tree", "--offline", "-p", "unbanished-sim", "-e", "normal", "--prefix", "none",
               "--target", "all"]).stdout
    names = {line.split()[0] for line in tree.splitlines() if line.strip()}
    banned = {"wgpu", "winit", "unbanished-ui", "unbanished-gfx", "unbanished-audio", "unbanished-net"}
    if names & banned:
        raise RuntimeError(f"unbanished-sim depends on {sorted(names & banned)}")


def suite_harness(ctx: Context) -> None:
    """The agent harness: every Python and shell tool parses, the skills point at real paths,
    and, when this computer has them, the emulator and the decompile tool work."""
    tools = [p for p in (REPO / "tools").rglob("*.py") if "node_modules" not in p.parts]
    for p in tools:
        try:
            compile(p.read_text(encoding="utf-8"), str(p), "exec")
        except SyntaxError as e:
            raise RuntimeError(f"{p.relative_to(REPO)}:{e.lineno}: {e.msg}") from e
    scripts = [p for p in (REPO / "tools").rglob("*.sh") if "node_modules" not in p.parts]
    for p in scripts:
        sh(["bash", "-n", str(p)])
    sh([sys.executable, "tools/triage/skills_check.py"])
    ctx.notes.append(f"{len(tools)} Python tools, {len(scripts)} shell scripts, skills")
    venv = REPO / ".venv" / "bin" / "python"
    exe = REPO / "game-data" / "original" / "Application-steam-x64.exe"
    if venv.exists() and exe.exists():
        # The emulator loads the game and its engine DLL and links the game's imports.
        out = sh([str(venv), "-c", (
            "import sys; sys.path.insert(0, 'tools/emu'); from pe_emu import Emu; "
            "emu = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll'); "
            "print('emulator ok', len(emu.exports))")]).stdout
        if "emulator ok" not in out:
            raise RuntimeError(f"the emulator did not load the binaries: {out}")
        ctx.notes.append("emulator loads the binaries")
    else:
        ctx.notes.append("emulator not checked (no .venv or game data)")
    if (REPO / "analysis" / "export" / "Application-steam-x64.exe" / "decomp.c").exists():
        out = sh([sys.executable, "tools/triage/decomp.py", "0x1400aa000"]).stdout
        if "Names::Generate" not in out:
            raise RuntimeError("tools/triage/decomp.py did not label FUN_1400aa000 as Names::Generate")
        ctx.notes.append("decomp.py reads the export")
    else:
        ctx.notes.append("decomp.py not checked (no Ghidra export)")


def suite_unit(ctx: Context) -> None:
    """Every Rust test in the workspace (unit, corpus, parity and save tests)."""
    proc = sh(["cargo", "test", "--offline", "-q", "--workspace", "--release", "--no-fail-fast"],
              check=False)
    out = proc.stdout + proc.stderr
    passed = sum(int(l.split()[3]) for l in out.splitlines() if l.startswith("test result:"))
    failed = sum(int(l.split()[5]) for l in out.splitlines() if l.startswith("test result:"))
    ctx.notes.append(f"{passed} passed, {failed} failed")
    if proc.returncode != 0 or failed:
        tail = [l for l in out.splitlines() if "FAILED" in l or "panicked" in l][:15]
        raise RuntimeError("cargo test failed\n" + "\n".join("      " + l for l in tail))


def suite_wasm(ctx: Context) -> None:
    """The browser build compiles with no warnings."""
    proc = sh(["cargo", "build", "--offline", "--release", "-p", "unbanished-app",
               "--target", "wasm32-unknown-unknown"])
    warnings = [l for l in proc.stderr.splitlines() if l.startswith("warning")]
    if warnings:
        raise RuntimeError(f"{len(warnings)} warnings:\n" + "\n".join("      " + w for w in warnings[:10]))


def suite_lint(ctx: Context) -> None:
    """Clippy finds nothing in the workspace, or in the browser build (warnings are errors)."""
    for args in (["--workspace", "--all-targets"], ["-p", "unbanished-app", "--target", "wasm32-unknown-unknown"]):
        proc = subprocess.run(["cargo", "clippy", "--offline", "--release", *args, "--message-format=short",
                               "--", "-D", "warnings"], cwd=REPO, capture_output=True, text=True)
        if proc.returncode != 0:
            found = [l for l in proc.stderr.splitlines() if ": error" in l or ": warning" in l]
            raise RuntimeError(f"{len(found)} findings:\n" + "\n".join("      " + l for l in found[:15]))


# A build with no extensions (`--no-default-features`), in its own target folder so that it
# never replaces the full build the other suites run.
VANILLA_DIR = REPO / "target" / "vanilla"


def suite_features(ctx: Context) -> None:
    """The game builds with no extensions (native and browser) and with the unstable ones, with no warnings."""
    for target in ([], ["--target", "wasm32-unknown-unknown"]):
        proc = sh(["cargo", "build", "--offline", "--release", "-p", "unbanished-app", "--no-default-features",
                   "--target-dir", str(VANILLA_DIR), *target])
        warnings = [l for l in proc.stderr.splitlines() if l.startswith("warning")]
        if warnings:
            where = "browser" if target else "native"
            raise RuntimeError(f"{len(warnings)} warnings ({where}):\n" + "\n".join("      " + w for w in warnings[:10]))
    # Every unstable extension too: it must build, though no release contains it.
    proc = sh(["cargo", "check", "--offline", "--release", "-p", "unbanished-app", "--features", "unstable"])
    warnings = [l for l in proc.stderr.splitlines() if l.startswith("warning")]
    if warnings:
        raise RuntimeError(f"{len(warnings)} warnings (unstable):\n" + "\n".join("      " + w for w in warnings[:10]))


def suite_vanilla(ctx: Context) -> None:
    """The build with no extensions plays every scenario exactly as the full build."""
    need_game_data()
    build_game(ctx)
    sh(["cargo", "build", "-q", "--offline", "--release", "-p", "unbanished-app", "--bin", "unbanished",
        "--no-default-features", "--target-dir", str(VANILLA_DIR)], verbose=ctx.verbose)
    out = sh(["tools/compat/compare.sh", str(GAME), str(VANILLA_DIR / "release" / "unbanished")]
             ).stdout.strip().splitlines()
    ctx.notes.append(out[-1].split(": ")[0] + ": identical" if out else "")


def run_scenario(name: str, spec: dict) -> list[dict]:
    """Run one probe scenario headless and return its answers."""
    with tempfile.TemporaryDirectory(prefix="ub-probe-") as tmp:
        tmp = Path(tmp)
        (tmp / "user").mkdir()
        out = tmp / "probe.jsonl"
        env = {"UNBANISHED_USER_DIR": str(tmp / "user"), "UNBANISHED_SETTINGS": str(tmp / "settings.txt"),
               "UNBANISHED_SEED": "1234"}
        cmd = [str(GAME), "--boot-capture", str(tmp / "last.png"), str(spec["seconds"]),
               "--capture-size", "1280x720", *spec["args"], "--probe-out", str(out)]
        proc = sh(cmd, env=env, check=False)
        if proc.returncode != 0 or not out.exists():
            raise RuntimeError(f"{name}: the game exited with {proc.returncode}\n      "
                               + "\n      ".join((proc.stderr or "").splitlines()[-10:]))
        return [json.loads(l) for l in out.read_text().splitlines() if l.strip()]


def suite_probe(ctx: Context) -> None:
    """Probe scenarios (tools/test/scenarios.json) give exactly their baselines."""
    need_game_data()
    build_game(ctx)
    scenarios = {k: v for k, v in json.loads((REPO / "tools/test/scenarios.json").read_text()).items()
                 if not k.startswith("_")}
    only = os.environ.get("UB_SCENARIO")
    failures = []
    for name, spec in scenarios.items():
        if only and only != name:
            continue
        answers = run_scenario(name, spec)
        errors = [a for a in answers if "error" in a]
        baseline = BASELINES / f"{name}.json"
        text = json.dumps(answers, indent=1, sort_keys=True) + "\n"
        if ctx.update_baselines:
            BASELINES.mkdir(parents=True, exist_ok=True)
            baseline.write_text(text)
            ctx.notes.append(f"{name}: baseline written ({len(answers)} answers, {len(errors)} errors)")
            continue
        if not baseline.exists():
            failures.append(f"{name}: no baseline; run with --update-baselines and review it")
            continue
        expected = baseline.read_text()
        if text != expected:
            diff = list(difflib.unified_diff(expected.splitlines(), text.splitlines(),
                                             f"baselines/{name}.json", "this run", n=1, lineterm=""))
            failures.append(f"{name}: differs from its baseline\n" + "\n".join("      " + l for l in diff[:40]))
        else:
            ctx.notes.append(f"{name}: {len(answers)} answers match")
    if failures:
        raise RuntimeError("\n    ".join(failures))


# --------------------------------------------------------------------------------------------
# Standard tier


def suite_determinism(ctx: Context) -> None:
    """Two new games with the same seed draw identical frames (tools/triage/determinism.sh)."""
    need_game_data()
    out = sh(["tools/triage/determinism.sh", "22", "1234"]).stdout.strip().splitlines()
    ctx.notes.append(out[-1] if out else "")


def build_web(ctx: Context) -> None:
    """Build the browser version into target/test-web/game, unless it is newer than the code."""
    if shutil.which("trunk") is None:
        raise Skip("no trunk (cargo install trunk)")
    sh(["trunk", "build", "--release", "--dist", str(WEB_DIST), "--public-url", "/"],
       cwd=REPO / "apps" / "game", env={"CARGO_TARGET_DIR": str(REPO / "target" / "web")})


def node_suite(ctx: Context, file: str) -> None:
    if shutil.which("node") is None:
        raise Skip("no node (Node 22 or later)")
    RESULTS.mkdir(parents=True, exist_ok=True)
    out = RESULTS / f"{Path(file).stem}.json"
    proc = sh(["node", file], env={"UB_TEST_JSON": str(out), "UB_WEB_DIST": str(WEB_DIST),
                                   "UB_WINDATA": str(WINDATA)}, check=False)
    results = json.loads(out.read_text()) if out.exists() else []
    passed = sum(r["status"] == "pass" for r in results)
    skipped = [r for r in results if r["status"] == "skip"]
    failed = [r for r in results if r["status"] == "fail"]
    ctx.notes.append(f"{passed} passed, {len(skipped)} skipped, {len(failed)} failed")
    for r in skipped:
        ctx.notes.append(f"skipped: {r['name']} ({r.get('reason')})")
    if failed or proc.returncode != 0:
        lines = [f"{r['name']}: {r.get('reason', '')}" for r in failed] or [proc.stderr[-800:]]
        raise RuntimeError("\n      ".join(lines))


def suite_browser(ctx: Context) -> None:
    """The browser version in headless Chrome and Firefox: start page, manager, Exit and Quit."""
    build_web(ctx)
    errors = []
    for f in ("tools/test/browser/start-page.test.mjs", "tools/test/browser/exit.test.mjs",
              "tools/test/browser/game.test.mjs"):
        try:
            node_suite(ctx, f)
        except RuntimeError as e:
            errors.append(str(e))
    if errors:
        raise RuntimeError("\n      ".join(errors))


# --------------------------------------------------------------------------------------------
# Long tier


def suite_site(ctx: Context) -> None:
    """The website builds with no dead links."""
    if shutil.which("npm") is None:
        raise Skip("no npm")
    sh(["npm", "--prefix", "site", "run", "build"])


def compat(script: str, about: str, *extra: str) -> Callable[[Context], None]:
    def run(ctx: Context) -> None:
        need_game_data()
        build_game(ctx)
        out = sh([f"tools/compat/{script}", str(GAME), *extra]).stdout.strip().splitlines()
        ctx.notes.append(out[-1] if out else "")
    run.__doc__ = about
    return run


SUITES = [
    Suite("checks", "quick", "Formatting of changed Rust files, the deviation ledger, doc comments, the core's dependencies", suite_checks),
    Suite("harness", "quick", "The agent harness: tools parse, skills point at real paths, emulator and decompile tool work", suite_harness),
    Suite("unit", "quick", "Every Rust test in the workspace (unit, corpus, parity, saves)", suite_unit),
    Suite("wasm", "quick", "The browser build compiles with no warnings", suite_wasm),
    Suite("lint", "quick", "Clippy finds nothing, natively or in the browser build", suite_lint),
    Suite("probe", "quick", "Probe scenarios give exactly their baselines (UI and town)", suite_probe),
    Suite("determinism", "standard", "Two runs with one seed draw identical frames", suite_determinism),
    Suite("browser", "standard", "Headless Chrome and Firefox: start page, manager, Exit, Quit", suite_browser),
    Suite("features", "standard", "The game builds with no extensions (native and browser), and with the unstable ones", suite_features),
    Suite("site", "long", "The website builds", suite_site),
    Suite("vanilla", "long", "The build with no extensions plays exactly as the full build", suite_vanilla),
    Suite("replay", "long", "Recorded games replay to the same state", compat("replay.sh", "Replays")),
    Suite("lockstep", "long", "Players over a simulated network stay identical", compat("lockstep.sh", "Lockstep")),
    Suite("network", "long", "Game processes through a relay stay identical", compat("network.sh", "Network")),
    Suite("multiplayer", "long", "Host, join, play and leave through the Multiplayer window",
          compat("multiplayer.sh", "Multiplayer")),
    Suite("server", "long", "The server: a resync, a departure, a new player, a restart that resumes the town",
          compat("server.sh", "Server")),
    Suite("server-simulate", "long", "The server runs each room's game itself and the players agree with it",
          compat("server.sh", "Server running the game", "--simulate")),
    Suite("hooks", "long", "A rule hook changes the game the same way every time, and players with it stay in step",
          compat("hooks.sh", "Rule hooks")),
]
TIERS = {"quick": ["quick"], "standard": ["quick", "standard"], "long": ["quick", "standard", "long"]}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("what", nargs="*", default=["quick"], help="a tier (quick, standard, long) or suite names")
    parser.add_argument("--list", action="store_true", help="list the suites and stop")
    parser.add_argument("--update-baselines", action="store_true", help="write new probe baselines")
    parser.add_argument("--fail-fast", action="store_true", help="stop at the first failed suite")
    parser.add_argument("--verbose", action="store_true", help="show build output")
    args = parser.parse_args()

    if args.list:
        for tier in ("quick", "standard", "long"):
            print(f"{tier}:")
            for s in SUITES:
                if s.tier == tier:
                    print(f"  {s.name:12} {s.about}")
        return 0

    names: list[str] = []
    for w in args.what:
        if w in TIERS:
            names += [s.name for s in SUITES if s.tier in TIERS[w]]
        elif any(s.name == w for s in SUITES):
            names.append(w)
        else:
            parser.error(f"unknown tier or suite {w!r} (see --list)")
    names = list(dict.fromkeys(names))

    summary = []
    for s in [s for s in SUITES if s.name in names]:
        ctx = Context(update_baselines=args.update_baselines, verbose=args.verbose)
        print(f"…     {s.name}: {s.about}", flush=True)
        start = time.monotonic()
        try:
            s.run(ctx)
            status, reason = "pass", ""
        except Skip as e:
            status, reason = "skip", str(e)
        except Exception as e:  # noqa: BLE001 - every failure is reported, not raised
            status, reason = "fail", str(e)
        secs = time.monotonic() - start
        mark = {"pass": "ok   ", "skip": "skip ", "fail": "FAIL "}[status]
        print(f"{mark} {s.name} ({secs:.0f} s)" + "".join(f"\n      {n}" for n in ctx.notes)
              + (f"\n      {reason}" if reason else ""), flush=True)
        summary.append({"suite": s.name, "tier": s.tier, "status": status, "seconds": round(secs, 1),
                        "notes": ctx.notes, **({"reason": reason} if reason else {})})
        if status == "fail" and args.fail_fast:
            break

    RESULTS.mkdir(parents=True, exist_ok=True)
    (RESULTS / "summary.json").write_text(json.dumps(summary, indent=1) + "\n")
    counts = {k: sum(r["status"] == k for r in summary) for k in ("pass", "skip", "fail")}
    total = sum(r["seconds"] for r in summary)
    print(f"\n{counts['pass']} passed, {counts['skip']} skipped, {counts['fail']} failed in {total:.0f} s")
    return 1 if counts["fail"] else 0


if __name__ == "__main__":
    sys.exit(main())
