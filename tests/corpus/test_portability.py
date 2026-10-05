import unittest
from copy import deepcopy
import hashlib
from pathlib import Path
import tempfile
from portability import artifact_classification, capability_diff, exact_byte_proof, exact_stack_identity, oracle_tiers


class PortabilityControls(unittest.TestCase):
    def test_byte_proof_requires_executed_semantics_and_exact_runtime(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        log = Path(directory.name) / "oracle.log"
        text = "".join(f"Tier {tier} identity: PASS\n" for tier in ("1A", "1B", "1C", "2"))
        text += "Tier 3 identity: DIFFERENT\n"
        log.write_text(text)
        prior = {"candidate_stack_identity": "runtime", "differences": [{
            "fixture": "fixture", "candidate_output_sha256": "bytes",
            "classification": "SemanticEquivalent", "tiers": {t: "PASS" for t in ("1B", "1C", "2")},
            "semantic_oracle": {"log_path": str(log), "log_sha256": hashlib.sha256(text.encode()).hexdigest(),
                                "exit_code": 0, "argv": ["cargo", "test", "compare_portability_pair_from_env"]}}]}
        self.assertEqual(exact_byte_proof(prior, "fixture", "runtime", "bytes", "bytes")["tiers"]["3"], "MATCH")
        for name, runtime, reference, candidate in [("other", "runtime", "bytes", "bytes"),
                ("fixture", "changed-runtime", "bytes", "bytes"), ("fixture", "runtime", "bytes", "changed"),
                ("fixture", "runtime", "other-bytes", "other-bytes")]:
            self.assertIsNone(exact_byte_proof(prior, name, runtime, reference, candidate))
        for classification in ("Regression", "Unresolved"):
            altered = deepcopy(prior)
            altered["differences"][0]["classification"] = classification
            self.assertIsNone(exact_byte_proof(altered, "fixture", "runtime", "bytes", "bytes"))
        for alteration in ({}, {"exit_code": 1}, {"log_sha256": "changed"}, {"log_path": str(log.parent / "missing.log")}):
            altered = deepcopy(prior)
            altered["differences"][0]["semantic_oracle"].update(alteration)
            if not alteration:
                altered["differences"][0].pop("semantic_oracle")
            self.assertIsNone(exact_byte_proof(altered, "fixture", "runtime", "bytes", "bytes"))
        for state in ("FAIL", None):
            altered = deepcopy(prior)
            altered["differences"][0]["tiers"]["1C"] = state
            self.assertIsNone(exact_byte_proof(altered, "fixture", "runtime", "bytes", "bytes"))

    def test_tier3_difference_is_diagnostic_not_missing_semantic_evidence(self):
        log = "".join(f"Tier {tier} identity: PASS\n" for tier in ("1A", "1B", "1C", "2"))
        log += "Tier 3 whole-file SHA-256 bytes (build identity not attested here): DIFFERENT\n"
        self.assertEqual(oracle_tiers(log), {"1A": "PASS", "1B": "PASS", "1C": "PASS",
                                            "2": "PASS", "3": "DIFFERENT"})
        self.assertEqual(oracle_tiers(log + log), oracle_tiers(log))
        self.assertEqual(oracle_tiers(log.replace("DIFFERENT", "MATCH"))["3"], "MATCH")

    def test_missing_or_conflicting_oracle_report_is_not_filled_in(self):
        self.assertEqual(oracle_tiers("test result: ok\n"), {})
        self.assertEqual(oracle_tiers("Tier 2 structure: PASS\nTier 2 structure: FAIL\n"), {})

    def test_capability_added_removed_changed_and_array_order(self):
        a = {"features": ["b", "a"], "profile": "Supported", "removed": 1}
        b = {"features": ["a", "b"], "profile": "Unsupported", "added": 2}
        changes = capability_diff(a, b)
        self.assertEqual([(x["path"], x["kind"]) for x in changes],
                         [("added", "added"), ("profile", "changed"), ("removed", "removed")])

    def test_exact_identity_cannot_override_semantics(self):
        self.assertEqual(artifact_classification(False, True, False, True), "Regression")
        self.assertEqual(artifact_classification(False, True, None, True), "Unresolved")

    def test_profile_entrypoint_and_modifier_additions_are_explicit(self):
        changes = capability_diff({"modifiers": [9], "profiles": {"main": ["decode"]}},
                                  {"modifiers": [0, 9], "profiles": {"main": ["decode", "encode"]}})
        self.assertEqual([(r["path"], r["kind"]) for r in changes],
                         [("modifiers", "added"), ("profiles.main", "added")])

    def test_same_stack_retains_strict_gate(self):
        self.assertEqual(artifact_classification(True, False, True, True), "Regression")
        self.assertEqual(artifact_classification(True, True, True, True), "ExpectedExact")

    def test_cross_stack_requires_explained_difference(self):
        self.assertEqual(artifact_classification(False, False, True, False), "Unresolved")
        self.assertEqual(artifact_classification(False, False, True, True), "SemanticEquivalent")

    def test_stack_label_and_paths_do_not_relax_same_stack_gate(self):
        stack = {"stack_id": "fedora-reference", "source": {"head": "source"},
                 "binary": {"sha256": "binary", "path": "/one/bin"},
                 "libraries": {"libavcodec": {"sha256": "lib", "version": "62.28.103", "path": "/one/lib"}},
                 "dependencies": {"libc": {"sha256": "libc"}}, "driver_files": {"anv": {"sha256": "anv"}},
                 "ffmpeg": {"identity": {"sha256": "ffmpeg"}}, "ffprobe": {"identity": {"sha256": "ffprobe"}},
                 "render_node": "/dev/dri/renderD128", "vulkan_profile": {"profiles": ["Intel"]}}
        stack.update({key: {"stdout": key} for key in ("kernel", "packages", "gpu", "rustc", "cargo")})
        renamed = deepcopy(stack)
        renamed["stack_id"] = "another-descriptive-label"
        renamed["binary"]["path"] = "/copy/bin"
        renamed["libraries"]["libavcodec"]["path"] = "/copy/lib"
        self.assertEqual(exact_stack_identity(stack), exact_stack_identity(renamed))
        renamed["driver_files"]["anv"]["sha256"] = "different-driver"
        self.assertNotEqual(exact_stack_identity(stack), exact_stack_identity(renamed))


if __name__ == "__main__":
    unittest.main()
