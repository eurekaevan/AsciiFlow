"""Stack-scoped corpus context; never promotes or rewrites media baselines."""
import ctypes
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
CLASSES = {"ExpectedExact", "ApprovedVolatileDifference", "SemanticEquivalent",
           "CapabilityDrift", "PerformanceDrift", "Regression", "Unresolved"}


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def query(argv):
    result = subprocess.run(argv, cwd=ROOT, capture_output=True, text=True,
                            timeout=60, check=False)
    return {"argv": [str(x) for x in argv], "exit_code": result.returncode,
            "stdout": result.stdout, "stderr": result.stderr}


def identity(path):
    path = Path(path).resolve(strict=True)
    return {"path": str(path), "bytes": path.stat().st_size, "sha256": digest(path)}


def loaded_libraries(binary):
    result = query(["ldd", str(binary)])
    if result["exit_code"]:
        raise ValueError(f"cannot resolve runtime libraries: {result}")
    libraries = {}
    for line in result["stdout"].splitlines():
        match = re.match(r"\s*(lib(?:avcodec|avformat|avutil|swscale|swresample)\.so\.\d+) => (\S+)", line)
        if match:
            libraries[match[1]] = identity(match[2])
    if not all(any(k.startswith(name + ".so.") for k in libraries)
               for name in ("libavcodec", "libavformat", "libavutil", "libswscale")):
        raise ValueError("incomplete actual libav runtime identity")
    for name, record in libraries.items():
        library = ctypes.CDLL(record["path"])
        stem = name.split(".so.")[0].removeprefix("lib")
        function = getattr(library, stem + "_version")
        function.restype = ctypes.c_uint
        version = function()
        record["version"] = f"{version >> 16}.{(version >> 8) & 255}.{version & 255}"
    return libraries


def dependency_identity(binary):
    result = query(["ldd", str(binary)])
    if result["exit_code"]:
        raise ValueError("could not attest linked runtime dependencies")
    dependencies = {}
    for line in result["stdout"].splitlines():
        match = re.match(r"\s*(\S+) => (/\S+)", line)
        if match:
            dependencies[match[1]] = identity(match[2])
    return dependencies


def driver_identity():
    return {name: identity(path) for name, path in {
        "anv": "/usr/lib64/libvulkan_intel.so", "ihd": "/usr/lib64/dri-nonfree/iHD_drv_video.so",
        "libva": "/usr/lib64/libva.so.2", "libdrm": "/usr/lib64/libdrm.so.2"
    }.items() if Path(path).is_file()}


def source_identity():
    diff = subprocess.run(["git", "diff", "--binary", "HEAD", "--", ".",
                           ":(exclude)tests/portability/stacks/**",
                           ":(exclude)tests/portability/stage54c1.json",
                           ":(exclude)tests/portability/stage54c1a.json",
                           ":(exclude)docs/stage5.4c1a-deterministic-mux.md",
                           ":(exclude)docs/stage5.4c1-portability-baseline.md"], cwd=ROOT,
                          capture_output=True, check=True).stdout
    paths = subprocess.run(["git", "ls-files", "--cached", "--others", "--exclude-standard"],
                           cwd=ROOT, capture_output=True, text=True, check=True).stdout.splitlines()
    # Recorded stacks/results are evidence, not source. Excluding them avoids a
    # self-referential identity when the second manifest records the first one.
    files = {p: digest(ROOT / p) for p in sorted(set(paths))
             if (ROOT / p).is_file() and not p.startswith("tests/portability/stacks/")
             and p not in {"tests/portability/stage54c1.json", "tests/portability/stage54c1a.json", "docs/stage5.4c1a-deterministic-mux.md", "docs/stage5.4c1-portability-baseline.md"}}
    return {"head": query(["git", "rev-parse", "HEAD"])["stdout"].strip(),
            "dirty_diff_sha256": hashlib.sha256(diff).hexdigest(),
            "cargo_lock_sha256": digest(ROOT / "Cargo.lock"), "files_sha256": files}


def capture(stack_id, binary, prefix=None):
    if not re.fullmatch(r"[a-z0-9][a-z0-9.-]+", stack_id) or stack_id in {"old", "new", "test"}:
        raise ValueError("stack_id must be a stable descriptive identifier")
    with tempfile.TemporaryDirectory(prefix="asciiflow-vulkan-profile-") as directory:
        result = subprocess.run(["vulkaninfo", "--json=0"], cwd=directory,
                                capture_output=True, text=True, timeout=60)
        profiles = list(Path(directory).glob("VP_VULKANINFO_*.json"))
        vulkan_profile = {"exit_code": result.returncode, "stderr": result.stderr,
                          "profiles": [json.loads(p.read_text()) for p in profiles]}
    vaapi = query(["vainfo", "--display", "drm", "--device", "/dev/dri/renderD128"])
    profiles = {}
    for profile, entrypoint in re.findall(r"(VAProfile\w+)\s*:\s*(VAEntrypoint\w+)", vaapi["stdout"]):
        profiles.setdefault(profile, []).append(entrypoint)
    profiles = {key: sorted(set(value)) for key, value in sorted(profiles.items())}
    return {"schema_version": 1, "stack_id": stack_id,
            "runtime": {"prefix": str(Path(prefix).resolve()) if prefix else None},
            "source": source_identity(), "binary": identity(binary),
            "libraries": loaded_libraries(binary),
            "dependencies": dependency_identity(binary), "driver_files": driver_identity(),
            "ffmpeg": {"identity": identity(shutil.which("ffmpeg")), "version": query(["ffmpeg", "-version"])},
            "ffprobe": {"identity": identity(shutil.which("ffprobe")), "version": query(["ffprobe", "-version"])},
            "kernel": query(["uname", "-a"]), "rustc": query(["rustc", "-Vv"]),
            "cargo": query(["cargo", "-V"]),
            "packages": query(["rpm", "-q", "mesa-vulkan-drivers", "libva", "intel-media-driver", "libdrm"]),
            "gpu": query(["lspci", "-nnk", "-s", "00:02.0"]),
            "render_node": "/dev/dri/renderD128",
            "vaapi": vaapi, "vaapi_profiles": profiles,
            "vulkan": query(["vulkaninfo", "--summary"]), "vulkan_profile": vulkan_profile}


def activate(manifest, binary):
    """Activate only a recorded isolated prefix, then attest actual executables/libs."""
    stack = json.loads(Path(manifest).read_text())
    if stack.get("schema_version") != 1:
        raise ValueError("unsupported stack manifest schema")
    prefix = stack["runtime"]["prefix"]
    if prefix:
        prefix = Path(prefix).resolve(strict=True)
        os.environ["PATH"] = str(prefix / "bin") + os.pathsep + os.environ["PATH"]
        os.environ["LD_LIBRARY_PATH"] = str(prefix / "lib")
    if digest(binary) != stack["binary"]["sha256"]:
        raise ValueError("stack binary identity changed")
    for tool in ("ffmpeg", "ffprobe"):
        if digest(shutil.which(tool)) != stack[tool]["identity"]["sha256"]:
            raise ValueError(f"stack {tool} identity changed")
    if loaded_libraries(binary) != stack["libraries"]:
        raise ValueError("stack loaded libav identity changed")
    if dependency_identity(binary) != stack["dependencies"] or driver_identity() != stack["driver_files"]:
        raise ValueError("stack linked dependency or driver bytes changed")
    if source_identity() != stack["source"]:
        raise ValueError("stack source identity changed; recapture both stacks after fixes")
    return stack


def capability_diff(reference, candidate, path=""):
    """Structural facts, not driver version guesses; array order is not a fact."""
    if isinstance(reference, dict) and isinstance(candidate, dict):
        changes = []
        for key in sorted(reference.keys() | candidate.keys()):
            name = f"{path}.{key}" if path else key
            if key not in reference:
                changes.append({"path": name, "kind": "added", "candidate": candidate[key]})
            elif key not in candidate:
                changes.append({"path": name, "kind": "removed", "reference": reference[key]})
            else:
                changes.extend(capability_diff(reference[key], candidate[key], name))
        return changes
    if isinstance(reference, list) and isinstance(candidate, list):
        reference = sorted(reference, key=lambda x: json.dumps(x, sort_keys=True))
        candidate = sorted(candidate, key=lambda x: json.dumps(x, sort_keys=True))
        if reference != candidate:
            from collections import Counter
            before = Counter(json.dumps(x, sort_keys=True) for x in reference)
            after = Counter(json.dumps(x, sort_keys=True) for x in candidate)
            return ([{"path": path, "kind": "removed", "reference": json.loads(value), "count": count}
                     for value, count in sorted((before - after).items())] +
                    [{"path": path, "kind": "added", "candidate": json.loads(value), "count": count}
                     for value, count in sorted((after - before).items())])
    return [] if reference == candidate else [{"path": path, "kind": "changed",
                                              "reference": reference, "candidate": candidate}]


def artifact_classification(same_stack, exact_equal, semantic_equal, explained):
    if semantic_equal is False:
        return "Regression"
    if semantic_equal is None or (not exact_equal and not explained):
        return "Unresolved"
    if exact_equal:
        return "ExpectedExact"
    return "Regression" if same_stack else "SemanticEquivalent"


def exact_stack_identity(stack):
    """Labels, paths and diagnostic prose cannot weaken an identical-stack gate."""
    return {"source": stack["source"], "binary_sha256": stack["binary"]["sha256"],
            "libraries": {name: {k: value[k] for k in ("sha256", "version")}
                          for name, value in stack["libraries"].items()},
            "tools": {tool: stack[tool]["identity"]["sha256"] for tool in ("ffmpeg", "ffprobe")},
            "dependencies": {name: value["sha256"] for name, value in stack["dependencies"].items()},
            "driver_files": {name: value["sha256"] for name, value in stack["driver_files"].items()},
            "host": {key: stack[key]["stdout"] for key in ("kernel", "packages", "gpu", "rustc", "cargo")},
            "render_node": stack["render_node"], "vulkan_profile": stack["vulkan_profile"]["profiles"]}


def oracle_tiers(log):
    """Read the existing report without interpreting byte drift as a failure."""
    tiers = {}
    for tier in ("1A", "1B", "1C", "2", "3"):
        states = "MATCH|DIFFERENT" if tier == "3" else "PASS|FAIL"
        matches = re.findall(r"^Tier " + tier + r"[^\n]*: (" + states + r")$", log, re.MULTILINE)
        # A panic may repeat the report. Conflicting or missing reports are not
        # usable evidence; never fill them from the command's exit status.
        if matches and len(set(matches)) == 1:
            tiers[tier] = matches[0]
    return tiers


def compare_corpus(run, reference_directory, manifest):
    """Compare actual execution evidence; use the existing Rust media oracle."""
    from run import load, save
    reference_directory = reference_directory.resolve()
    reference_stack = load(reference_directory / "stack.json")
    candidate_stack = run.args.stack_context
    if reference_stack["source"] != candidate_stack["source"]:
        raise ValueError("cross-source comparison is historical evidence, not portability")
    same_stack = exact_stack_identity(reference_stack) == exact_stack_identity(candidate_stack)
    reference = {r["id"]: r for r in load(reference_directory / "results.json")["results"]
                 if r["category"] == "classification"}
    candidate = {r["id"]: r for r in run.results if r["category"] == "classification"}
    ids = {f["id"] for f in manifest["fixtures"]}
    if set(reference) != ids or set(candidate) != ids:
        raise ValueError("fixture identities differ between stack runs")
    records = []
    for fixture in manifest["fixtures"]:
        name = fixture["id"]
        before, after = reference[name], candidate[name]
        record = {"fixture": name, "reference_stack": reference_stack["stack_id"],
                  "candidate_stack": candidate_stack["stack_id"]}
        if before["result"] != "PASS" or after["result"] != "PASS":
            records.append({**record, "classification": "Unresolved", "reason": "execution did not pass on both stacks"})
            continue
        left, right = before["evidence"], after["evidence"]
        if left["input_sha256"] != right["input_sha256"]:
            raise ValueError(f"{name}: input changed")
        plans = [{k: v for k, v in (e.get("actual_plan") or {}).items() if k != "reasons"}
                 for e in (left, right)]
        record["planner_diff"] = capability_diff(*plans)
        record["planner_reason_diff"] = capability_diff(
            (left.get("actual_plan") or {}).get("reasons"), (right.get("actual_plan") or {}).get("reasons"))
        diagnostics = [load(directory / evidence["diagnostic"])
                       for directory, evidence in [(reference_directory, left), (run.out, right)]]
        record["capability_diff"] = capability_diff(diagnostics[0].get("capabilities"), diagnostics[1].get("capabilities"))
        if fixture["expected"]["runtime"] != "Pass":
            diagnostics = [load(directory / evidence["runtime_diagnostic"])
                           for directory, evidence in [(reference_directory, left), (run.out, right)]]
            failures = [{k: (d.get("failure") or {}).get(k) for k in ("stage", "category", "code")}
                        for d in diagnostics]
            record["failure_semantics"] = failures
            record["classification"] = "ExpectedExact" if failures[0] == failures[1] else "Unresolved"
            record["expected_rejection"] = True
            records.append(record)
            continue
        paths = [directory / f"{name}-output.mp4" for directory in [reference_directory, run.out]]
        exact = digest(paths[0]) == digest(paths[1])
        env = dict(os.environ, ASCIIFLOW_REGRESSION_REFERENCE=str(paths[0]),
                   ASCIIFLOW_REGRESSION_CANDIDATE=str(paths[1]),
                   ASCIIFLOW_PORTABILITY_PQ="1" if fixture["expected"]["classification"] == "HdrPq" and fixture["request"]["dynamic_range"] == "preserve" else "0")
        env.pop("ASCIIFLOW_REGRESSION_EXACT_BUILD", None)
        if same_stack:
            env["ASCIIFLOW_REGRESSION_EXACT_BUILD"] = "attested"
        passed = run.command(f"portability-oracle-{name}", ["cargo", "test", "--release", "-p", "asciiflow-media", "--test", "media_regression",
                             "compare_portability_pair_from_env", "--", "--ignored", "--exact", "--nocapture"], env)
        command = run.commands[-1]
        log = (run.out / command["log"]).read_text()
        tiers = oracle_tiers(log)
        # Compilation failures or missing oracle results cannot become a
        # semantic-equivalence claim, even if an output SHA happens to match.
        semantic = passed if len(tiers) == 5 else None
        record.update(tiers=tiers, whole_file_equal=exact, oracle_command=command["id"],
                      oracle_log_sha256=digest(run.out / command["log"]),
                      classification=artifact_classification(same_stack, exact, semantic, passed))
        if record["planner_diff"]:
            record["classification"] = "Unresolved"
        records.append(record)
    document = {"schema_version": 1, "mode": "same-stack regression" if same_stack else "cross-stack portability",
                "source_identity_equal": True, "differences": records}
    save(run.out / "portability-comparison.json", document)
    if any(r["classification"] in {"Regression", "Unresolved"} for r in records):
        raise ValueError("portability semantic/eligibility differences remain unresolved; see comparison receipt")
    return document
