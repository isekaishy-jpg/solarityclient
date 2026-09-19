"""Causal analysis must not subtract asynchronous work or double count nested spans."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("analyze_trace", Path(__file__).parents[1] / "analyze-trace.py")
analyzer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(analyzer)


def span(identity, parent, start, duration, thread="main", kind="span"):
    return dict(span_id=identity, parent_id=parent, related_id=0, origin_frame=1,
                completion_frame=1, start_ns=start, duration_ns=duration, kind=kind,
                label="fixture", owner=0, reason=0, value=0, name="", thread=thread)


class TraceAnalysis(unittest.TestCase):
    def test_nested_overlap_and_workers_are_not_added_to_parent_cost(self):
        rows = [span(1, 0, 0, 10_000_000), span(2, 1, 1_000_000, 4_000_000),
                span(3, 1, 3_000_000, 4_000_000), span(4, 1, 0, 9_000_000, "worker"),
                span(5, 1, 20_000_000, 3_000_000)]
        report = analyzer.summarize(rows, 1, 10)
        parent = next(row for row in report["operations"] if row["id"] == 1)
        self.assertEqual(parent["inclusive_ms"], 10)
        self.assertEqual(parent["exclusive_wall_ms"], 4)
        self.assertEqual(report["missing_references"], [])

    def test_equal_source_ordinals_in_independent_scenes_stay_separate(self):
        self.check_scene_sources("M2 frame preparation")

    def test_staged_admission_keeps_model_sources_and_deferred_workers_separate(self):
        self.check_scene_sources("m2.frame_admission")

    def test_resumed_admission_and_earlier_pose_share_one_logical_scene(self):
        scene = span(1, 0, 0, 0, kind="admit")
        scene["label"] = "m2.frame"
        setup = span(2, 1, 0, 10)
        resumed = span(3, 1, 100, 10)
        for row in (setup, resumed):
            row["label"] = "m2.frame_admission"
        placement = span(4, 3, 100, 5)
        placement.update(label="m2.placement", owner=1, reason=1)
        source = span(5, 4, 100, 0, kind="asset")
        source.update(owner=1, name="UNIT.M2")
        pose = span(6, 2, 10, 30, "worker")
        pose.update(label="runtime.application.terrain_frame.m2.preparation.poses.input.sample", owner=1)
        report = analyzer.summarize([scene, setup, resumed, placement, source, pose], 1, 10)
        self.assertEqual(len(report["models"]), 1)
        model = report["models"][0]
        self.assertEqual(model["asset"], "UNIT.M2")
        self.assertEqual(model["scene_span"], 1)
        self.assertEqual(model["pose_worker_ms"], 0.00003)
        self.assertFalse(any(row["id"] == 1 for row in report["operations"]))
        self.check_scene_sources("m2.frame")

    def check_scene_sources(self, label):
        first = span(1, 0, 0, 100)
        first["label"] = label
        second = span(2, 0, 200, 100)
        second["label"] = label
        a, b = span(3, 1, 0, 10), span(4, 2, 200, 20)
        for row in (a, b):
            row.update(label="m2.placement", owner=1, reason=1)
        catalog_a, catalog_b = span(5, 3, 1, 0, kind="asset"), span(6, 4, 201, 0, kind="asset")
        catalog_a.update(owner=1, name="FIRST.M2")
        catalog_b.update(owner=1, name="SECOND.M2")
        requirements = span(7, 3, 2, 0, kind="value")
        requirements.update(label="m2.requirements", reason=(1 << 1) | (1 << 5), value=1)
        worker = span(8, 3, 400, 200, "worker")
        worker.update(label="m2.geometry", owner=1, reason=1)
        report = analyzer.summarize([first, second, a, b, catalog_a, catalog_b, requirements, worker], 1, 10)
        self.assertEqual({row["asset"] for row in report["models"]}, {"FIRST.M2", "SECOND.M2"})
        self.assertEqual(len(report["models"]), 2)
        model = next(row for row in report["models"] if row["asset"] == "FIRST.M2")
        self.assertEqual(model["visible"], 1)
        self.assertEqual(model["callback_owner"], 1)
        self.assertEqual(model["primary_shadow"], 0)
        self.assertEqual(model["geometry_worker_ms"], 0.0002)
        self.assertIn(requirements, report["observations"])

    def test_missing_dependency_is_explicit_and_gpu_is_separate(self):
        gpu = span(2, 99, 4, 8_000_000, kind="gpu")
        timing = span(3, 1, 20, 9_000_000, kind="timing")
        report = analyzer.summarize([span(1, 0, 0, 10), gpu, timing], 1, 10)
        self.assertEqual(report["missing_references"], [99])
        self.assertEqual(len(report["gpu"]), 1)
        self.assertEqual(len(report["operations"]), 1)
        self.assertEqual(report["operations"][0]["exclusive_wall_ms"], 0.00001)
        self.assertIn(timing, report["observations"])


if __name__ == "__main__":
    unittest.main()
