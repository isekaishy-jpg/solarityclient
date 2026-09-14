"""Summarize a permanent F10 capture using only the Python standard library."""

import argparse
import csv
import math
from collections import defaultdict
from pathlib import Path
import statistics
import struct


def read_rows(path):
    """Read writer-owned CSVs without interpreting game payloads or log text."""
    with path.open(encoding="utf-8", newline="") as source:
        return list(csv.DictReader(source))


def percentile(values, fraction):
    """Return an exact observed nearest-rank percentile from retained frames."""
    ordered = sorted(values)
    return ordered[max(0, math.ceil(len(ordered) * fraction) - 1)]


def report(path, warmup):
    """Separate full frames, overlapping phases, sampled details and memory trends."""
    rows = read_rows(path)
    events = read_rows(path.with_suffix(".events.csv"))
    resources = read_rows(path.with_suffix(".resources.csv"))
    print(path.with_suffix(".txt").read_text(encoding="utf-8"))
    print(f"\nAnalysis excludes the first {warmup:g} seconds of writer intervals.")
    frames = [row for row in events if row["scope"] == "frame.live"
              and not row["phase"] and float(row["elapsed_seconds"]) > warmup]
    for lane in ("ordinary", "detail"):
        selected = [int(row["value"]) / 1e6 for row in frames if row["frame_lane"] == lane]
        if selected:
            mean = statistics.mean(selected)
            fps = f"{1000 / mean:.1f}" if mean > 0 else "below clock resolution"
            print(f"{lane}: {len(selected)} frames; mean {mean:.3f} ms "
                  f"({fps} FPS); p95 {percentile(selected, .95):.3f} ms; "
                  f"p99 {percentile(selected, .99):.3f} ms; max {max(selected):.3f} ms")
    ordinary = [row for row in frames if row["frame_lane"] == "ordinary"]
    if len(ordinary) >= 20:
        part = max(1, len(ordinary) // 5)
        for name, group in (("first fifth", ordinary[:part]), ("last fifth", ordinary[-part:])):
            values = [int(row["value"]) / 1e6 for row in group]
            print(f"{name}: mean {statistics.mean(values):.3f} ms; "
                  f"p10 {percentile(values, .1):.3f} ms; p95 {percentile(values, .95):.3f} ms")
    grouped = defaultdict(lambda: [0, 0, 0])
    for row in rows:
        if row["unit"] != "ns" or float(row["elapsed_seconds"]) <= warmup:
            continue
        key = (row["frame_lane"], row["thread"], row["scope"], row["phase"])
        group = grouped[key]
        group[0] += int(row["count"])
        group[1] += int(row["total"])
        group[2] = max(group[2], int(row["max"]))
    print("\nLargest scope totals by lane (inclusive/overlapping; do not sum):")
    for lane in ("ordinary", "detail"):
        selected = sorted(((key, value) for key, value in grouped.items() if key[0] == lane),
                          key=lambda item: item[1][1], reverse=True)[:25]
        for (_, thread, scope, phase), (count, total, maximum) in selected:
            print(f"{lane:8} {total / 1e6:10.2f} ms total; {total / count / 1e6:8.3f} ms/call; "
                  f"max {maximum / 1e6:8.3f}; {count:8} calls; {thread} {scope} {phase}")
    memory = [row for row in resources if row["kind"] == "memory" and row["available"] != "0"]
    if memory:
        for name in ("working_set_bytes", "private_bytes"):
            start, end = (int(memory[index][name]) / 2**20 for index in (0, -1))
            print(f"{name}: {start:.1f} -> {end:.1f} MiB ({end-start:+.1f})")
    positions = defaultdict(dict)
    for row in events:
        if row["scope"].startswith("scene.player_") and row["scope"].endswith("_f32_bits"):
            value = struct.unpack("<f", struct.pack("<I", int(row["value"])))[0]
            positions[int(row["completion_frame"])][row["scope"]] = value
    if positions:
        for name, frame in (("first", min(positions)), ("last", max(positions))):
            print(f"{name} sampled position (frame {frame}): {positions[frame]}")


def main():
    """Accept the primary interval CSV; sibling files share its capture stem."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("capture", type=Path)
    parser.add_argument("--warmup-seconds", type=float, default=2.0)
    args = parser.parse_args()
    report(args.capture, args.warmup_seconds)


if __name__ == "__main__":
    main()
