#!/usr/bin/env python3
"""Inspect sampled causal chains without summing overlapping frame/worker/GPU costs."""
import argparse
import collections
import csv
import json
from pathlib import Path


def records(path):
    """Read one pass without retaining the complete capture in memory."""
    with path.open(newline="", encoding="utf-8-sig") as source:
        for row in csv.DictReader(source):
            for key in ("span_id", "parent_id", "related_id", "origin_frame", "completion_frame",
                        "start_ns", "duration_ns", "owner", "reason", "value"):
                row[key] = int(row[key])
            yield row


def union_length(intervals):
    """Remove overlap between same-thread children before reporting exclusive wall time."""
    total = 0
    end = 0
    for start, stop in sorted(intervals):
        total += max(0, stop - max(start, end))
        end = max(end, stop)
    return total


def summarize(rows, frame, top):
    """Keep dependency edges distinct from nested synchronous intervals."""
    by_id = {row["span_id"]: row for row in rows}
    children = collections.defaultdict(list)
    for row in rows:
        if row["kind"] == "span":
            children[row["parent_id"]].append(row)
    operations = []
    for row in rows:
        if row["kind"] not in ("span", "slow"):
            continue
        start, stop = row["start_ns"], row["start_ns"] + row["duration_ns"]
        intervals = []
        for child in children[row["span_id"]]:
            # Deferred descendants and other threads are causal, not nested CPU cost.
            if child["thread"] == row["thread"]:
                a, b = max(start, child["start_ns"]), min(stop, child["start_ns"] + child["duration_ns"])
                if a < b:
                    intervals.append((a, b))
        operations.append({"id": row["span_id"], "parent": row["parent_id"],
                           "scope": row["label"], "thread": row["thread"], "owner": row["owner"],
                           "inclusive_ms": row["duration_ns"] / 1e6,
                           "exclusive_wall_ms": (row["duration_ns"] - union_length(intervals)) / 1e6})
    def scene(row):
        # Several independent model scenes can be prepared in one UI/world frame.
        seen = set()
        current = row
        while current and current["span_id"] not in seen:
            seen.add(current["span_id"])
            if current["label"] == "M2 frame preparation":
                return current["span_id"]
            current = by_id.get(current["parent_id"])
        return 0
    assets = {(row["origin_frame"], scene(row), row["owner"]): row["name"]
              for row in rows if row["kind"] == "asset"}
    models = collections.defaultdict(lambda: collections.defaultdict(float))
    placements = {(row["origin_frame"], scene(row), row["owner"]): row
                  for row in rows if row["label"] == "m2.placement" and row["kind"] == "span"}
    for row in rows:
        if row["label"] in ("m2.placement", "m2.geometry") and row["kind"] == "span":
            key = (row["origin_frame"], scene(row), row["reason"])
            models[key]["ordered_ms" if row["label"] == "m2.placement" else "geometry_worker_ms"] += row["duration_ns"] / 1e6
            if row["label"] == "m2.placement":
                models[key]["placements"] += 1
        elif row["label"] == "runtime.application.terrain_frame.m2.preparation.poses.input.sample" and row["kind"] == "span":
            placement = placements.get((row["origin_frame"], scene(row), row["owner"]))
            if placement:
                models[(row["origin_frame"], scene(row), placement["reason"])]["pose_worker_ms"] += row["duration_ns"] / 1e6
        elif row["label"].startswith("m2.output.") or row["label"] in ("m2.requirements", "m2.cpu_bone_demand"):
            placement = by_id.get(row["parent_id"])
            if placement and placement["label"] == "m2.placement":
                model = models[(placement["origin_frame"], scene(placement), placement["reason"])]
                if row["label"] == "m2.requirements":
                    for bit, name in enumerate(("admitted", "visible", "primary_shadow", "environment_shadow",
                                                "light_owner", "callback_owner", "particle_owner", "palette_demand",
                                                "batch_hit", "shadow_output", "cpu_output", "geometry_pending")):
                        model[name] += bool(row["reason"] & (1 << bit))
                elif row["label"] == "m2.cpu_bone_demand":
                    model["cpu_bones"] += row["value"]
                else:
                    model[row["label"].removeprefix("m2.output.")] += row["value"]
    model_rows = [{"frame": key[0], "scene_span": key[1], "source": key[2], "asset": assets.get(key, "unmapped source"), **value}
                  for key, value in models.items()]
    missing = sorted({row[key] for row in rows for key in ("parent_id", "related_id")
                      if row[key] and row[key] not in by_id})
    return {"origin_frame": frame, "records": len(rows), "missing_references": missing,
            "operations": sorted(operations, key=lambda row: -row["inclusive_ms"])[:top],
            "models": sorted(model_rows, key=lambda row: -row.get("ordered_ms", 0))[:top],
            "observations": [row for row in rows if row["kind"] in ("admit", "phase", "value", "asset", "timing")],
            "links": [row for row in rows if row["kind"] == "link"],
            "gpu": [row for row in rows if row["kind"] == "gpu"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trace", type=Path)
    parser.add_argument("--frame", type=int, help="origin frame; default is slowest sampled live frame")
    parser.add_argument("--after-frame", type=int, default=0)
    parser.add_argument("--top", type=int, default=12)
    parser.add_argument("--json", type=Path)
    args = parser.parse_args()
    frame = args.frame
    if frame is None:
        frames = (row for row in records(args.trace) if row["label"] == "frame.live"
                  and row["kind"] == "span" and row["origin_frame"] >= args.after_frame)
        largest = max(frames, key=lambda row: row["duration_ns"], default=None)
        if largest is None:
            parser.error("no sampled live frame exists in this trace")
        frame = largest["origin_frame"]
    selected = [row for row in records(args.trace) if row["origin_frame"] == frame]
    # Pull only referenced ancestors/operations from other originating frames.
    # A stopped capture or bounded-buffer loss may legitimately leave unresolved IDs.
    known = {row["span_id"] for row in selected}
    for _ in range(16):
        missing = {row[key] for row in selected for key in ("parent_id", "related_id") if row[key] and row[key] not in known}
        if not missing:
            break
        found = [row for row in records(args.trace) if row["span_id"] in missing]
        if not found:
            break
        selected.extend(found)
        known.update(row["span_id"] for row in found)
    report = summarize(selected, frame, args.top)
    print(f"Origin frame {frame}: {len(selected)} records; {len(report['missing_references'])} unresolved references")
    print("Inclusive spans overlap. Exclusive wall time is not charged CPU time. Worker and GPU durations are not additive to frame time.")
    for row in report["operations"]:
        print(f"{row['inclusive_ms']:8.3f} ms inclusive / {row['exclusive_wall_ms']:8.3f} ms exclusive wall  {row['scope']} owner={row['owner']} id={row['id']}")
    print("Model-source work (ordered and worker time shown separately):")
    for row in report["models"]:
        print(f"{row.get('ordered_ms', 0):8.3f} ms ordered / {row.get('pose_worker_ms', 0):8.3f} ms pose / {row.get('geometry_worker_ms', 0):8.3f} ms geometry  {row['asset']} placements={int(row.get('placements', 0))} particles={int(row.get('particle_vertices', 0))}")
    print(f"Dependency edges: {len(report['links'])}; GPU phase results: {len(report['gpu'])}")
    if args.json:
        args.json.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
