# Stack-scoped portability qualification

Stage 5.4C-2 uses `tests/portability/c2-matrix.json` as a canonical-centered
star matrix. `run-expanded-matrix.py` orchestrates the existing corpus and media
oracles; it does not invent a relaxed comparison engine. Schema-v2 stack capture
attests initialized ANV/iHD modules and their native dependency closure. Child
selectors are cleared and explicitly reconstructed for each stack; requested
driver paths alone are not evidence. Opt-in `/proc` samples additionally retain
actual CLI driver mappings when the process lives long enough to observe them.
Missing mappings remain unobserved, not inferred success.

Repeated same-stack comparisons may reuse a previously executed, passing Rust
Tier 1B/1C/2 result only when the complete runtime identity and the inspected
candidate file SHA match, and both new files are byte-identical. Receipts label
this exact-byte proof and hash the prior comparison; they do not claim a new
Rust invocation. Missing/failed semantic evidence or any byte/runtime drift
still invokes the unchanged oracle. Cross-stack comparisons never take this
shortcut.

The [C-2 report](stage5.4c2-expanded-portability-matrix.md) records the current
qualification outcome. `tests/portability/qualified-stacks.json` distinguishes
qualified, failed and incomplete stacks; exploration never promotes canonical.

Qualified support is scoped to the recorded Intel/Mesa/iHD/FFmpeg stacks, not
a claim about every release or a minimum supported version. A second passing
stack does not broaden the support contract or overwrite canonical artifacts.

The [Stage 5.4C-1A closure](stage5.4c1a-deterministic-mux.md) preserves strict
same-stack global packet order. Its report/receipt, like the C-1 report/receipt
and stack registry, are excluded only to avoid self-referential source hashes;
its generator, immutable captures, tests, production source, recipes and other
documentation remain in the source identity. `run-mux-closure.py` executes
retained/static/lifecycle/timing/portability gates using the existing oracles.

| Layer | Evidence | Decision |
|---|---|---|
| P0 Environment/capability identity | Source HEAD/diff/files, lockfile, binary, actual loaded libav paths/SHA/version, FFmpeg tools/config, Rust, kernel, GPU/driver, Mesa/ANV, libva/iHD, structured Vulkan profile | Identify changed variables; never infer hardware qualification from lavapipe |
| P1 Pipeline eligibility | Per-input CapabilitySnapshot, actual initialized PipelinePlan, codec/profile/format and NV12/P010 interop | Added/removed/changed capability is recorded separately; unexpected fallback or probe/init disagreement is a blocker |
| P2 Media correctness | Existing Tier 1B/1C/2, full decode, color, HDR side-data policy, frame/packet timestamps, audio payload/routing/title | Primary cross-stack gate; unexplained coded changes, pixel changes, or timing/color/audio regressions block qualification |
| P3 Exact artifact identity | Tier 1A packets, extradata, container metadata, Tier 3 whole-file SHA | Strict for an independently attested identical stack and deterministic baseline; diagnostic across stacks |

P layers do not rename the retained media tiers. In particular a changed SHA
alone is not a regression across stacks, and identical decoded pixels alone
do not prove coded metadata, stream routing, timing or container correctness.

## Recorded stack context

`tests/portability/stack.py capture` prints a versioned manifest; save reviewed
manifests under `tests/portability/stacks/` with descriptive stable stack IDs.
It records actual resolved libav libraries, not just SONAMEs or the RPM database.
Capture in the declared prefix environment. Corpus `--stack` activates that
prefix, attests tools/library/binary/source identities, and saves the manifest
alongside the run. No global loader configuration or system package replacement.

Source identity excludes the stack manifests and final report/receipt themselves
to avoid self-referential evidence. All code, test recipes, fixtures, lockfile
and other documentation remain part of the compared source inventory. Freeze
these before capturing either stack. A fix requires recapture and reruns of both
stacks; old-source outputs are historical evidence only.

Use the existing corpus runner, not a separate portability execution engine:

```sh
python3 -B tests/corpus/run.py quick --stack tests/portability/stacks/STACK.json \
  --manifest tests/portability/core-set.json --binary target/release/asciiflow \
  --generated-inputs target/stage54c1-evidence/inputs --output target/STACK-run
# On the second stack, add --reference-run target/REFERENCE-run.
python3 -B tests/portability/stack.py diff REFERENCE-capabilities.json CANDIDATE-capabilities.json
```

The core set covers the five SDR codec/depth paths, HEVC/AV1 PQ preserve,
HEVC PQ→H2648 and AV1 PQ→HEVC10, AAC single/dual, B-frames, 24000/1001,
nonzero PTS, the Stage 5.4B fixes and HLG/full-range/conflict rejection.
Source bytes and generator version stay fixed; never regenerate inputs using
the alternate FFmpeg and silently call them the same fixture.

`--reference-run` uses the unchanged Rust media comparator. The test-only
portability entry point admits PQ inspection explicitly, but does not alter
the historical strict PQ/H.264 tests. H.264 retains its pinned UUID, fixed
VAAPI suffix and Lavc62.28 patch-token SEI rule: SPS/PPS/VCL/unknown SEI
differences are not automatically approved. HEVC/AV1 do not borrow that rule.
Only the established narrow Lavf/Lavc patch metadata policy applies; new
metadata differences require explicit evidence, never generic tag removal.

Every difference is classified as ExpectedExact, ApprovedVolatileDifference,
SemanticEquivalent, CapabilityDrift, PerformanceDrift, Regression or Unresolved.
Supported-path Unresolved differences block sealing. A nonzero oracle command
without all tier results is Unresolved, never a semantic-equivalence claim.

## Surface and lifecycle checks

The ignored `portability_descriptors` native test captures actual NV12/P010
decoded input and encoder-owned output layouts: dimensions, object/layer/plane
counts, object size/modifier and per-plane object index/offset/pitch. FD numbers
are process-local, not layout identities. Both directions actually import into
ANV, with validation observed through teardown and exact FD return.

The offline runtime-layout control varies pitch, offsets and modifier values;
it proves structural validation does not hardcode the canonical layout. It
does not prove a synthetic modifier is importable: the actual Vulkan runtime
format/modifier query remains authoritative. A genuine driver-layout change
requires real import/processing/output validation, not forced canonical pitches.

For an alternate hardware stack also run an existing 1000-frame minimum (3000
preferred) full-GPU lifecycle test. Record FD baseline/peak/after, requiring
after == baseline, not identical peaks across drivers. Validation must report
zero errors for SDR, PQ preserve and PQ→SDR. Performance is characterization;
no arbitrary percentage failure threshold is used.

## Upgrade workflow

1. Capture the new stack without changing the reference source/fixtures.
2. Diff capabilities, real descriptors and initialized planner selections.
3. Run the core set and unchanged semantic oracles; retain stack-scoped hashes.
4. Classify every difference; isolate failures before altering source or policy.
5. Expand relevant corpus coverage if indicated, then record reviewed qualification.

Never automatically bless a stack, overwrite a baseline on failure, declare a
minimum Mesa/iHD version from two observations, or add a global driver denylist
without a root cause. If production is fixed, rerun canonical retained 17×3
and relevant real-media cases as well as the alternate stack.

## Isolated FFmpeg 8.1.2 recipe

Source: [official release archive](https://ffmpeg.org/releases/ffmpeg-8.1.2.tar.xz),
SHA-256 `464beb5e7bf0c311e68b45ae2f04e9cc2af88851abb4082231742a74d97b524c`.
Verify its `.asc` signature using the official FFmpeg signing key fingerprint
`FCF986EA15E6E293A5644F10B4322F04D67658D8` before extracting it.

`tests/portability/build-ffmpeg-8.1.2.sh` builds only inside
`target/stage54c1-toolchains`. Headers are extracted, not installed, from:

| RPM | SHA-256 |
|---|---|
| libva-devel-2.23.0-3.fc44.x86_64 | 61b12657dc9cad71c60fa37db98e195ad4363d073ac9aca2bf603c9f43ce9628 |
| libdrm-devel-2.4.134-1.fc44.x86_64 | 001b477e68cf8747c7574e9183907d0754c279de43dbc1d672399b548ccb4371 |
| libdav1d-devel-1.5.4-1.fc44.x86_64 | ff376d533610231d05aa8ff6712fe6955b1e62e60edf0541bd209ecc11016507 |

Runtime libva/libdrm/libdav1d and drivers remain the host versions. This source
build differs from the distribution FFmpeg configuration as well as its patch
version; do not attribute a difference solely to the patch number. The first
minimal build lacking libdav1d could not software-decode AV1 and is not a
qualified alternate; the completed recipe explicitly enables the unchanged
host libdav1d. No package downgrade or ldconfig override is involved.

Preparation commands (from the repository root; downloads/extraction only):

```sh
mkdir -p target/stage54c1-toolchains/downloads target/stage54c1-toolchains/devel
curl --fail --location --output target/stage54c1-toolchains/downloads/ffmpeg-8.1.2.tar.xz https://ffmpeg.org/releases/ffmpeg-8.1.2.tar.xz
curl --fail --location --output target/stage54c1-toolchains/downloads/ffmpeg-8.1.2.tar.xz.asc https://ffmpeg.org/releases/ffmpeg-8.1.2.tar.xz.asc
curl --fail --location --output target/stage54c1-toolchains/downloads/ffmpeg-devel.asc https://ffmpeg.org/ffmpeg-devel.asc
mkdir -m 700 -p target/stage54c1-toolchains/keyring
gpg --homedir target/stage54c1-toolchains/keyring --import target/stage54c1-toolchains/downloads/ffmpeg-devel.asc
gpg --homedir target/stage54c1-toolchains/keyring --verify target/stage54c1-toolchains/downloads/ffmpeg-8.1.2.tar.xz.asc target/stage54c1-toolchains/downloads/ffmpeg-8.1.2.tar.xz
dnf --repo=fedora --repo=updates download --destdir=target/stage54c1-toolchains/downloads --arch=x86_64 libva-devel-2.23.0-3.fc44 libdrm-devel-2.4.134-1.fc44 libdav1d-devel-1.5.4-1.fc44
tar -xJf target/stage54c1-toolchains/downloads/ffmpeg-8.1.2.tar.xz -C target/stage54c1-toolchains
```

Verify the three RPM hashes above, then extract each with `rpm2cpio | cpio -idm`
while working inside `target/stage54c1-toolchains/devel`; never run an install
transaction. Run `bash tests/portability/build-ffmpeg-8.1.2.sh`. Keep the full
configure/build logs with the resulting stack manifest. The capture/activation
prefix must prepend to the existing PATH, retaining the same Rust toolchain.
