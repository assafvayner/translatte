#!/usr/bin/env python3
"""Summarizes a live-mode event log: the `liveev {...}` lines debug builds write (logcat, the iOS
app's stderr) or the iOS app's Documents/liveev.jsonl.

Usage: scripts/live/analyze.py <log> [--out DIR] [--loop-ms N] [--video recording.mp4 --frames-every 1.0]

Prints a short summary and writes DIR/summary.json (DIR defaults to the log's directory). With
--video and ffmpeg, also extracts a frame every --frames-every seconds to DIR/frames/.

Metrics: update rate and stage times (p50/p90); flicker (a text removed, then shown again within
3s) and each text's visible fraction over its lifetime (first add to last remove); time to clear
(each removal's delay since its box was last matched); churn (adds + removes per minute); the most
boxes on screen at once; and, for replays, frame age (update time minus the frame's video time;
pass --loop-ms, the clip's length, for a looped replay).
"""
import argparse
import json
import pathlib
import shutil
import subprocess
import sys

FLICKER_MS = 3000


def events(path):
    out = []
    for line in pathlib.Path(path).read_text(errors="replace").splitlines():
        i = line.find("liveev ")
        text = line[i + 7:] if i >= 0 else line.strip()
        if not text.startswith("{"):
            continue
        try:
            out.append(json.loads(text))
        except json.JSONDecodeError:
            pass
    return out


def pct(xs, p):
    if not xs:
        return None
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(len(xs) * p))]


def dist(xs):
    return {"n": len(xs), "p50": pct(xs, 0.5), "p90": pct(xs, 0.9), "max": max(xs) if xs else None}


def unwrap_pts(updates, loop_ms):
    """Video times of a looped replay's updates, counted from the first loop's start."""
    out, offset, last = [], 0, None
    for e in updates:
        if "pts_ms" not in e:
            continue
        if loop_ms and last is not None and e["pts_ms"] < last - 1000:
            offset += loop_ms
        last = e["pts_ms"]
        out.append((e["t_ms"], e["pts_ms"] + offset))
    return out


def analyze(evs, loop_ms=None):
    updates = [e for e in evs if e["ev"] == "update"]
    if not evs:
        return {}
    t0, t1 = evs[0]["t_ms"], evs[-1]["t_ms"]
    span_s = max((t1 - t0) / 1000, 1e-9)
    up_span = (updates[-1]["t_ms"] - updates[0]["t_ms"]) / 1000 if len(updates) > 1 else 0

    live = {}  # id -> src
    last_seen = {}  # id -> t of last match
    visible = {}  # src -> number of boxes showing it
    since = {}  # src -> t it became visible
    shown_ms = {}  # src -> total visible ms
    first_add, last_remove = {}, {}
    hidden_at = {}  # src -> t its last box went away
    flickers = []
    clear = []
    adds = removes = 0
    reasons = {}
    max_boxes = 0

    def show(src, t):
        if visible.get(src, 0) == 0:
            since[src] = t
            if src in hidden_at and t - hidden_at[src] <= FLICKER_MS:
                flickers.append({"src": src, "gap_ms": t - hidden_at[src], "t_ms": t})
        visible[src] = visible.get(src, 0) + 1

    def hide(src, t):
        visible[src] -= 1
        if visible[src] == 0:
            shown_ms[src] = shown_ms.get(src, 0) + t - since[src]
            hidden_at[src] = t

    for e in evs:
        t, ev = e["t_ms"], e["ev"]
        if ev == "update":
            for i in e.get("seen", []):
                last_seen[i] = t
        elif ev == "add":
            adds += 1
            live[e["id"]] = e["src"]
            last_seen[e["id"]] = t
            first_add.setdefault(e["src"], t)
            show(e["src"], t)
        elif ev in ("move", "retext"):
            last_seen[e["id"]] = t
        elif ev == "remove" and e["id"] in live:
            removes += 1
            reasons[e.get("reason")] = reasons.get(e.get("reason"), 0) + 1
            src = live.pop(e["id"])
            if e.get("reason") != "reset" and e["id"] in last_seen:
                clear.append(t - last_seen[e["id"]])
            last_remove[src] = t
            hide(src, t)
        max_boxes = max(max_boxes, len(live))
    for src, n in visible.items():
        if n > 0:
            shown_ms[src] = shown_ms.get(src, 0) + t1 - since[src]
            last_remove[src] = t1

    fractions = {}
    for src, start in first_add.items():
        life = last_remove.get(src, t1) - start
        fractions[src] = round(shown_ms.get(src, 0) / life, 3) if life > 0 else 1.0

    ages = [t - pts for t, pts in unwrap_pts(updates, loop_ms)]
    return {
        "span_s": round(span_s, 2),
        "updates": len(updates),
        "updates_per_s": round((len(updates) - 1) / up_span, 2) if up_span > 0 else None,
        "interval_ms": dist([e["interval_ms"] for e in updates if "interval_ms" in e]),
        "det_ms": dist([e["det_ms"] for e in updates]),
        "rec_ms": dist([e["rec_ms"] for e in updates]),
        "tr_ms": dist([e["tr_ms"] for e in updates]),
        "total_ms": dist([e["det_ms"] + e["rec_ms"] + e["tr_ms"] for e in updates]),
        "regions": dist([e["regions"] for e in updates]),
        "frame_age_ms": dist(ages),
        "frame_size": f'{updates[-1]["w"]}x{updates[-1]["h"]}' if updates else None,
        "adds": adds,
        "removes": removes,
        "remove_reasons": reasons,
        "churn_per_min": round((adds + removes) / span_s * 60, 1),
        "max_boxes": max_boxes,
        "flickers": len(flickers),
        "flicker_events": flickers,
        "time_to_clear_ms": dist(clear),
        "texts": len(first_add),
        "visible_fraction": fractions,
        "visible_fraction_p50": pct(list(fractions.values()), 0.5),
    }


def fmt(d):
    if not d or d.get("n", 0) == 0:
        return "-"
    return f'p50 {d["p50"]}  p90 {d["p90"]}  max {d["max"]}  (n={d["n"]})'


def report(s):
    if not s:
        return "no liveev events"
    lines = [
        f'{s["updates"]} updates over {s["span_s"]}s ({s["updates_per_s"]}/s), frames {s["frame_size"]}',
        f'  interval ms   {fmt(s["interval_ms"])}',
        f'  det ms        {fmt(s["det_ms"])}',
        f'  rec ms        {fmt(s["rec_ms"])}',
        f'  tr ms         {fmt(s["tr_ms"])}',
        f'  total ms      {fmt(s["total_ms"])}',
        f'  regions       {fmt(s["regions"])}',
        f'  frame age ms  {fmt(s["frame_age_ms"])}',
        f'boxes: {s["adds"]} adds, {s["removes"]} removes {s["remove_reasons"]}, '
        f'churn {s["churn_per_min"]}/min, max {s["max_boxes"]} at once',
        f'flicker: {s["flickers"]} re-shows within {FLICKER_MS // 1000}s across {s["texts"]} texts; '
        f'visible fraction p50 {s["visible_fraction_p50"]}',
        f'time to clear ms  {fmt(s["time_to_clear_ms"])}',
    ]
    worst = sorted(s["visible_fraction"].items(), key=lambda kv: kv[1])[:5]
    for src, f in worst:
        lines.append(f"  {f:5.2f}  {src[:60]!r}")
    return "\n".join(lines)


def extract_frames(video, out_dir, every):
    if not shutil.which("ffmpeg"):
        print("ffmpeg not found; skipping frames", file=sys.stderr)
        return
    frames = out_dir / "frames"
    frames.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        ["ffmpeg", "-v", "error", "-y", "-i", str(video), "-vf", f"fps=1/{every}", str(frames / "frame_%04d.png")],
        check=True,
    )
    print(f"frames: {frames} (one per {every}s)")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("log")
    ap.add_argument("--out")
    ap.add_argument("--loop-ms", type=int, help="clip length, to time frames of a looped replay")
    ap.add_argument("--video")
    ap.add_argument("--frames-every", type=float, default=1.0)
    args = ap.parse_args()
    out_dir = pathlib.Path(args.out or pathlib.Path(args.log).parent)
    out_dir.mkdir(parents=True, exist_ok=True)
    summary = analyze(events(args.log), args.loop_ms)
    print(report(summary))
    (out_dir / "summary.json").write_text(json.dumps(summary, indent=2, ensure_ascii=False) + "\n")
    print(f"summary: {out_dir / 'summary.json'}")
    if args.video:
        extract_frames(args.video, out_dir, args.frames_every)


if __name__ == "__main__":
    main()
