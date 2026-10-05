import unittest
from copy import deepcopy
from portability import artifact_classification, capability_diff, exact_stack_identity, oracle_tiers


class PortabilityControls(unittest.TestCase):
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
