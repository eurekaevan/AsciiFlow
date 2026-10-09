"""Synthetic evidence-contract tests; these are not native qualification evidence."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("memory-plateau.py")
SPEC = importlib.util.spec_from_file_location("memory_plateau", SCRIPT)
PLATEAU = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PLATEAU)


class MemoryPlateau(unittest.TestCase):
    cycles = 200
    jobs_per_cycle = 4

    def fixture(self, root):
        result = {
            "schema": "asciiflow-default-memory-plateau-v1",
            "cycles": self.cycles,
            "jobs": self.cycles * self.jobs_per_cycle,
            "extended": False,
            "last_hwm_cycle": 0,
            "observed_plateau": True,
            "reference_configurations": 4,
            "retained_reference_bytes": 928880,
        }
        (root / "plateau-result.json").write_text(json.dumps(result))

        baseline = {
            "boundary": "initial", "cycle": 0, "fd_count": 12,
            "thread_count": 4, "private_anonymous_kib": 1000,
            "last_hwm_cycle": 0, "complete_cycles_without_new_hwm": 0,
            "allocator": {"in_use_including_mmap_bytes": 4096},
        }
        samples = [baseline]
        for cycle in range(1, self.cycles + 1):
            for job in range(self.jobs_per_cycle):
                samples.append({
                    **baseline, "boundary": "job-end", "cycle": cycle,
                    "job_index": (cycle - 1) * self.jobs_per_cycle + job,
                    "complete_cycles_without_new_hwm": cycle - 1,
                })
            samples.append({
                **baseline, "boundary": "cycle-end", "cycle": cycle,
                "job_index": cycle * self.jobs_per_cycle - 1,
                "complete_cycles_without_new_hwm": cycle,
            })
        self.write_jsonl(root / "memory.jsonl", samples)
        self.write_jsonl(root / "hwm.jsonl", [baseline])

        resources = {
            name: {"active_count": 0, "active_bytes": 0, "peak_count": 1}
            for name in ("vulkan_buffer_bindings", "vaapi_frames", "dma_buf_planes")
        }
        jobs = []
        for index in range(self.cycles * self.jobs_per_cycle):
            configuration = f"configuration-{index % self.jobs_per_cycle}"
            jobs.append({
                "index": index,
                "case": configuration,
                "outcome": "Success",
                "post_cleanup_resources_zero": True,
                "job_entry_returned_after_worker_join_boundary": True,
                "fd_count_before_session": 12,
                "fd_count_after_session": 12,
                "thread_count_before": 4,
                "thread_count_after": 4,
                "resource_samples": [{"resources": resources}],
                "byte_exact_with_first_output_for_configuration": True,
                "input_sha256": f"input-{index % self.jobs_per_cycle}",
                "output_sha256": f"output-{index % self.jobs_per_cycle}",
                "oracle": {"decoded_frames": 3},
                "plan": {"backend": "Vulkan"},
            })
        self.write_jsonl(root / "jobs.jsonl", jobs)

    @staticmethod
    def write_jsonl(path, rows):
        path.write_text("".join(json.dumps(row) + "\n" for row in rows))

    def with_fixture(self, mutate=None):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        self.fixture(root)
        if mutate:
            mutate(root)
        return PLATEAU.summarize(root)

    def test_valid_synthetic_200_cycle_evidence_is_summarized(self):
        result = self.with_fixture()
        self.assertEqual(result["cycles"], 200)
        self.assertEqual(result["cycle_records"], 200)
        self.assertEqual(result["job_outcomes"]["Success"], 800)
        self.assertTrue(result["observed_plateau"])

    def test_nonempty_or_nonzero_resource_accounting_is_rejected(self):
        for change in ("nonempty", "nonzero"):
            with self.subTest(change=change):
                def mutate(root):
                    path = root / "jobs.jsonl"
                    rows = list(PLATEAU.records(path))
                    rows[0]["resource_samples"][-1]["resources"] = (
                        {"vulkan_buffer_bindings": {"active_count": 1, "active_bytes": 0,
                                                     "peak_count": 1}}
                        if change == "nonempty" else
                        {"vulkan_buffer_bindings": {"active_count": 0, "active_bytes": 1,
                                                     "peak_count": 1}}
                    )
                    self.write_jsonl(path, rows)
                with self.assertRaises(AssertionError):
                    self.with_fixture(mutate)

    def test_extended_run_cannot_claim_completion_before_cycle_500(self):
        def mutate(root):
            path = root / "plateau-result.json"
            result = json.loads(path.read_text())
            result["extended"] = True
            path.write_text(json.dumps(result))
        with self.assertRaises(AssertionError):
            self.with_fixture(mutate)

    def test_wrong_intermediate_completed_cycle_counter_is_rejected(self):
        def mutate(root):
            path = root / "memory.jsonl"
            rows = list(PLATEAU.records(path))
            row = next(row for row in rows if row["boundary"] == "cycle-end" and row["cycle"] == 100)
            row["complete_cycles_without_new_hwm"] = 98
            self.write_jsonl(path, rows)
        with self.assertRaises(AssertionError):
            self.with_fixture(mutate)

    def test_late_hwm_reset_and_incorrect_plateau_claim_are_rejected(self):
        changes = (
            {"last_hwm_cycle": 150},
            {"observed_plateau": False},
        )
        for change in changes:
            with self.subTest(change=change):
                def mutate(root):
                    result_path = root / "plateau-result.json"
                    result = json.loads(result_path.read_text())
                    result.update(change)
                    result_path.write_text(json.dumps(result))
                    if "last_hwm_cycle" in change:
                        path = root / "memory.jsonl"
                        rows = list(PLATEAU.records(path))
                        for row in rows:
                            row["last_hwm_cycle"] = 150
                            row["complete_cycles_without_new_hwm"] = max(
                                0, row["cycle"] - (1 if row["boundary"] != "cycle-end" else 0) - 150
                            )
                        self.write_jsonl(path, rows)
                with self.assertRaises(AssertionError):
                    self.with_fixture(mutate)


if __name__ == "__main__":
    unittest.main()
