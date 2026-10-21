#!/usr/bin/env bash
# Summarizes live camera translation on a connected Android device. Debug builds log each live
# update to logcat ("live <rate>/s · det <ms> · rec <ms> · tr <ms> ms regions <n>").
# Usage: scripts/android/live-stats.sh [seconds] (default 30): clears logcat, waits while you point
# the camera at text, then prints update rate and stage-time stats.
set -euo pipefail

SECS="${1:-30}"
adb get-state >/dev/null
adb logcat -c
echo "point the camera at text for ${SECS}s..."
sleep "$SECS"
adb logcat -d -s translatte:I | python3 -c '
import re, statistics, sys
pat = re.compile(r"live (\S+)/s · det (\d+) · rec (\d+) · tr (\d+) ms regions (\d+)")
rows = [m.groups() for m in map(pat.search, sys.stdin) if m]
if not rows:
    sys.exit("no live updates logged (debug build, camera open, live translation on?)")
rates = [float(r[0]) for r in rows if r[0] != "–"]
def stats(name, xs):
    xs = sorted(xs)
    print(f"{name:>8}: mean {statistics.mean(xs):7.1f}  p50 {xs[len(xs) // 2]:7.1f}  p90 {xs[int(len(xs) * 0.9)]:7.1f}")
print(f"{len(rows)} updates")
if rates:
    stats("upd/s", rates)
for i, name in [(1, "det ms"), (2, "rec ms"), (3, "tr ms"), (4, "regions")]:
    stats(name, [int(r[i]) for r in rows])
stats("total ms", [int(r[1]) + int(r[2]) + int(r[3]) for r in rows])
'
