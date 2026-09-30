# C4B production output oracle harness

This harness records actual HDR→SDR production output evidence. Its presence or
offline tests do not establish production availability, throughput, numerical
tone-map parity, or hardware closure. Stage C-4B now establishes the checked-in
`hdr-to-sdr-production-v1.json` from actual thirty-run execution and full decode.
The separate hardware/resource/fault closure is documented in
`docs/stage5.3c4b-hdr-to-sdr-production.md` and `stage53c4b.json`.

Run from the repository root after building the intended production binary and
qualifying the required Intel render node. `ASCIIFLOW_BASELINE_BINARY` can select
a different binary. The input identity must match the existing
`tests/fixtures/codecs/c3-pq-legal-v1-identity.json`.

```sh
bash tests/baselines/media/generate-hdr-to-sdr-production-v1.sh \
  tests/fixtures/codecs /tmp/asciiflow-c4b-production
python3 tests/baselines/media/verify-hdr-to-sdr-production.py verify \
  /tmp/asciiflow-c4b-production /tmp/asciiflow-c4b-report.json \
  --input-directory tests/fixtures/codecs \
  --baseline /tmp/asciiflow-c4b-baseline.json
```

The matrix is two legal 300-frame, 1920×1080, 50 fps HEVC/AV1 PQ sources, each
converted to H.264 8-bit, HEVC/AV1 8-bit and HEVC/AV1 10-bit, three runs apiece.
Every command explicitly selects SDR, width 80, the standard charset and built-in
8×8 font, color, no audio, strict VAAPI decode, Vulkan GPU mapping, VAAPI encode,
and both interop directions `on`. Existing outputs, logs, measurements and
command and successful-exit records are never overwritten. A failed command preserves its evidence
and stops the matrix; use a new output directory for another complete attempt.
The verifier requires the post-success record binding the output and command
hashes; an incomplete or failed matrix cannot establish a baseline.

The verifier checks video-only output, all stream/frame color tags, 8/10-bit
4:2:0 geometry, H.264 High / HEVC Main or Main 10 / AV1 Main profiles, all 300 frame and packet timestamps, 50 fps, and coded elementary
stream/frame BT.709 limited tags. Mastering, content-light, dynamic HDR and Dolby
Vision side data fail the SDR gate. It requires FFmpeg 8.1.3, as does the retained
PQ production oracle.

Tier 1A packet identity, 1B stream/coded-signal identity, 1C decoded framehash and
Tier 2 frame metadata must match between all three runs. Tier 3 whole-file SHA-256
becomes a regression gate only when all three files match. These are the same
authoritative encoded-output tiers used by `verify-pq-production.py` and the
media regression policy; lossy decoded output is never compared against a raw
pre-encode oracle. Regression checks require every retained tier, including Tier
2; use `--check-baseline PATH` instead of `--baseline PATH` against an established
report. Reports and baselines are created exclusively and never overwritten.
An established Tier 3 gate requires the exact recorded build scope: binary hash,
production source hashes, kernel identity and selected RPM versions must match.
A scope mismatch fails explicitly; it cannot silently disable the hash gate.

Each report retains exact input manifest and file identities, binary identity,
command arguments and working directory, generator/oracle identities, source
revision and production diff hash, output file identities, packets, frame
metadata, decoded framehash, coded stream metadata, process timing and logs.
Verify from the same working directory used for generation, with the original
binary and input files still present. Source provenance in the report describes
verification time; the binary SHA-256 in each pre-run command record identifies
the executable actually invoked.
Pre-run command records also capture `uname` kernel identity and installed
FFmpeg/ffmpeg-libs, Mesa, intel-media-driver and SPIRV-Tools RPM versions when RPM
is available (otherwise explicitly `rpm-unavailable`). The source hash map uses
`rg --files` under `crates`, `apps` and `shaders`, includes untracked runtime
source such as C4A/C4B additions, package Cargo manifests, build scripts, shaders,
the workspace Cargo manifest/lockfile, and present toolchain/Cargo configuration.
Tests, examples and fixture data are excluded. All runs must bind the same
recorded build and machine identity, also checked against the verification-time
checkout; this is provenance evidence, not proof that a particular binary was
built from that checkout.

Offline validation:

```sh
bash -n tests/baselines/media/generate-hdr-to-sdr-production-v1.sh
python3 tests/baselines/media/test-hdr-to-sdr-production-oracle.py
```
