#!/usr/bin/env python3
"""Three-way shell benchmark: oracle (bash) vs two brush builds.

Runs each workload under all three shells with interleaved, pinned sampling
so machine drift affects every shell equally, verifies byte-for-byte output
parity before timing, and reports median +- MAD wall time plus pairwise
speedup ratios. A separate pass reads peak RSS (VmHWM) per workload.

Usage:
  three-way.py -a bash=/usr/bin/bash -b clap=/path/brush -c bpaf=/path/brush
"""

import argparse
import json
import shutil
import statistics
import subprocess
import sys
import time

BRUSH_FLAGS = [
    "--norc",
    "--noprofile",
    "--input-backend=basic",
    "--disable-bracketed-paste",
    "--disable-color",
]

MAD_SCALE = 1.4826
DEFAULT_SAMPLES = 15
STARTUP_BATCH = 20


def brush_like(name):
    return "brush" in name


def shell_argv(shell, script, args):
    argv = [shell["path"]]
    if brush_like(shell["name"]):
        argv += BRUSH_FLAGS
    else:
        argv += ["--norc", "--noprofile"]
    if script is None:
        argv += ["-c", "exit 0"]
    else:
        argv += [script] + args
    return argv


def run(argv, core=None):
    if core is not None:
        argv = ["taskset", "-c", str(core)] + argv
    return subprocess.run(argv, capture_output=True, text=True)


def timed_run(argv, core=None):
    start = time.perf_counter()
    result = run(argv, core)
    elapsed = time.perf_counter() - start
    if result.returncode != 0:
        raise RuntimeError(
            f"workload failed ({result.returncode}): {' '.join(argv)}\n"
            f"stderr: {result.stderr[:2000]}"
        )
    return elapsed, result.stdout


def peak_rss_kib(shell, script, args, core):
    """Peak RSS of the shell process running the workload, via VmHWM."""
    if not brush_like(shell["name"]):
        probe = "grep VmHWM /proc/$$/status"
    else:
        probe = "grep VmHWM /proc/$$/status"

    inner = f". '{script}' {' '.join(args)} >/dev/null 2>&1; {probe}"
    argv = [shell["path"]]
    argv += BRUSH_FLAGS if brush_like(shell["name"]) else ["--norc", "--noprofile"]
    argv += ["-c", inner]

    result = run(argv, core)
    for line in result.stdout.splitlines():
        if line.startswith("VmHWM:"):
            return int(line.split()[1])
    return None


def stats(values_ns):
    med = statistics.median(values_ns)
    mad = statistics.median(abs(v - med) for v in values_ns) * MAD_SCALE if len(values_ns) > 1 else 0.0
    return med, mad


def fmt_ms(seconds):
    return f"{seconds * 1e3:8.2f} ms"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("-a", "--shell-a", required=True, help="name=path (oracle, e.g. bash)")
    parser.add_argument("-b", "--shell-b", required=True, help="name=path")
    parser.add_argument("-c", "--shell-c", required=True, help="name=path")
    parser.add_argument("--samples", type=int, default=DEFAULT_SAMPLES)
    parser.add_argument("--core", type=int, default=None, help="pin all runs to this CPU")
    parser.add_argument("--skip-parity", action="store_true")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    shells = []
    for spec in (args.shell_a, args.shell_b, args.shell_c):
        name, _, path = spec.partition("=")
        path = shutil.which(path) or path
        shells.append({"name": name, "path": path})

    here = "/".join(__file__.split("/")[:-1])
    workloads = [
        {"name": "startup", "script": None, "args": [], "batch": STARTUP_BATCH},
        {"name": "interp-loop", "script": f"{here}/real-world/interp-loop.sh", "args": [], "batch": 1},
        {"name": "wordops", "script": f"{here}/real-world/wordops.sh", "args": [], "batch": 1},
        {"name": "config-lint-500", "script": f"{here}/real-world/config-lint.sh", "args": ["500"], "batch": 1},
        {"name": "deploy-sim", "script": f"{here}/real-world/deploy-sim.sh", "args": ["staging"], "batch": 1},
    ]

    # Parity check: identical stdout and exit status across all three shells.
    if not args.skip_parity:
        for wl in workloads:
            outputs = set()
            for shell in shells:
                batch = range(wl["batch"])
                outs = []
                for _ in batch:
                    _, out = timed_run(shell_argv(shell, wl["script"], wl["args"]), args.core)
                    outs.append(out)
                outputs.add(outs[-1])
            if len(outputs) != 1:
                print(f"PARITY FAILURE on {wl['name']}: outputs differ across shells", file=sys.stderr)
                sys.exit(2)

    results = {w["name"]: {s["name"]: [] for s in shells} for w in workloads}

    orders = [shells[i:] + shells[:i] for i in range(len(shells))]
    for sample in range(args.samples):
        order = orders[sample % len(orders)]
        for wl in workloads:
            for shell in order:
                total = 0.0
                for _ in range(wl["batch"]):
                    elapsed, _ = timed_run(shell_argv(shell, wl["script"], wl["args"]), args.core)
                    total += elapsed
                results[wl["name"]][shell["name"]].append(total / wl["batch"])

    if args.json:
        payload = {}
        for wl in workloads:
            entry = {}
            for shell in shells:
                med, mad = stats(results[wl["name"]][shell["name"]])
                entry[shell["name"]] = {"median_ms": med * 1e3, "mad_ms": mad * 1e3}
            payload[wl["name"]] = entry
        print(json.dumps(payload, indent=2))
        return

    print()
    print("=" * 76)
    print("Three-way shell benchmark (interleaved samples, median ± MAD·1.4826)")
    print(f"samples={args.samples}" + (f", pinned to cpu{args.core}" if args.core is not None else ""))
    print("=" * 76)

    for wl in workloads:
        print(f"\n🧪 {wl['name']}")
        meds = {}
        for shell in shells:
            med, mad = stats(results[wl["name"]][shell["name"]])
            meds[shell["name"]] = med
            print("   %10s: %s  (±%.2f ms)" % (shell["name"], fmt_ms(med), mad * 1e3))

        oracle = meds[shells[0]["name"]]
        for shell in shells[1:]:
            ratio = oracle / meds[shell["name"]]
            pct = (1.0 / ratio - 1.0) * 100
            print(f"   vs {shells[0]['name']}: {shell['name']} is {ratio:.2f}× ({pct:+.1f}%)")
        b, c = shells[1]["name"], shells[2]["name"]
        ratio = meds[c] / meds[b]
        faster = "faster" if meds[b] < meds[c] else "slower"
        print(f"   {b} vs {c}: {abs(1 - ratio) * 100:.1f}% ({faster})")

    print("\n📊 Peak RSS (VmHWM, KiB):")
    header = "   " + "".join(f"{s['name']:>16}" for s in shells)
    print(header)
    for wl in workloads:
        if wl["script"] is None:
            continue
        row = f"   {wl['name']:>14}"
        for shell in shells:
            kib = peak_rss_kib(shell, wl["script"], wl["args"], args.core)
            row += f"{kib if kib is not None else '?':>16}"
        print(row)


if __name__ == "__main__":
    main()
