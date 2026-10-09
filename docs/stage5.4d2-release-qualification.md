# Stage 5.4D-2 Release Qualification

## Decision

**NOT SEALED.** The recorded candidate builds, installs, relocates and passes
the technical release checks. The sole release blocker is incomplete exact
corresponding-source provision for GPL-linked non-system native components.
This is a local candidate, not an approved public release. No publication,
commit, tag, D-2 successor or new media capability was started.

D-1A, D-1B and D-1 remain **SEALED**. D-2, D overall and Stage 5.4 overall
remain **NOT SEALED**. Historical FAIL/NOT SEALED evidence remains unchanged.

## RC source and artifact identity (report items 1–11, 39–40)

| Identity | Recorded value |
| --- | --- |
| Version | `asciiflow 2.0.0-rc.1` |
| Base Git commit | `5dbe199825cb7aad8093e3cd8f940ba115a497b8` |
| Exact source fingerprint | `e0a6719cde23947a4174de7658103629c943a0432b06cadaf16bea46dcfaf25d` |
| Source archive SHA-256 | `a6219bbf0d914baf53c4fe6ce2b696eb2b577f43713e87797e6c6c5d95b38634` |
| Exact tracked dirty diff SHA-256 | `f8c002157b19322e73cd0e176cb299ad368baf84d8a826c744f094efdf62f456` |
| Cargo.lock SHA-256 | `c745692c9a218474d7fa89027ca284a2eca638c764711993e46746c6b4df534b` |
| Binary SHA-256 | `07ae232d997593d2fee5292c3659caf1cb02b7f25bce118a1f1e453a429e8d70` |
| Binary bytes | 4,136,312 |
| Candidate archive | `asciiflow-2.0.0-rc.1-linux-x86_64-candidate.tar.gz` |
| Archive SHA-256 | `d2f62b91234abc2da09cba53bfb90cc759b3cb720e6f47fbc77bc9d12d22199b` |
| Archive bytes | 1,640,465 |

The tree is intentionally dirty, including existing D-1 work; the commit alone
does **not** identify the RC. The archived source manifest covers tracked and
non-ignored untracked inputs, records file bytes/modes/hashes, full Git status
and empty submodule state. The exact patch and source archive are retained in
`target/releases/stage54d2-rc1/`. The generated D-2 report, qualification JSON
and evidence tar are explicitly excluded from the build-input fingerprint.
No previous target tree, shader cache or generated build output was copied.

Two independent copies of the frozen source each ran:

```text
cargo build --release --locked --workspace --message-format=json
```

Build 1 passed in 216 seconds; build 2 passed in 220 seconds, each with zero
unexpected compiler warnings. Both binaries happened to have the SHA above;
bit-reproducible ELF builds are not a release contract. Downloaded Cargo source
cache reuse is allowed; build output reuse was not. Offline builds are not a
claimed qualification gate. Actual Rust/Cargo was 1.97.1; declared Rust minimum
1.88 reflects existing let-chain syntax but was not separately tested.

Both binaries passed version/help and an actual 300-frame full-GPU SDR smoke.
The designated archive's binary is the exact binary used for canonical A/B/C,
software, failure, safe-output, working-directory and signal tests. The final
archive was freshly extracted and passed version/help after a notice-only
Rust standard-library license addition; its binary hash is unchanged from
the independently installed 300-frame smoke. No stale binary was packaged.

## CLI, configuration and features (items 5–6, 9, 34–35)

Help, planner and generated support contract agree: H.264 is 8-bit only;
HEVC/AV1 8/10-bit encoding requires qualified VAAPI; unsupported SDR depth
conversion is not promised. `preserve` retains SDR or canonical limited
BT.2020/PQ; explicit `sdr` converts qualified decoded PQ <=1000 cd/m² to
BT.709 SDR and is a no-op for SDR. PQ requires the qualified full-GPU interop
path. HLG and full range remain unsupported. Audio modes describe compressed
copy, not transcoding, with the existing CFR and container compatibility rules.
There are no subcommands or newly introduced public options.

Default logging has no qualification counters or per-frame resource history.
Detailed aggregate/device diagnostics use the existing `--verbose` option;
native warnings/errors remain visible. Bug reports can include version,
kernel, FFmpeg, Vulkan and VAAPI receipts; no telemetry was added.

Actual build features: CLI/core/media/CPU/font have no optional features;
interop has `hdr-to-sdr-production`; Vulkan has that feature plus the existing
transitive `hdr-pq-qualification` name required by production PQ shaders.
D-1 `native-reliability`, `reliability-measurement`, `mux-qualification`,
`encode-characterization` and experimental/fault HDR features are absent.
Normal factories install no injected fault. Remaining library fault enums/APIs
are unreachable through the normal release CLI: this is **disabled injection**,
not a claim that every fault-related symbol is physically absent.

## Actual runtime receipt (items 12–16)

| Component | Final candidate's recorded environment |
| --- | --- |
| Platform | Fedora 44, Linux x86-64 |
| GPU | Intel Meteor Lake-P / Intel Arc Graphics, `8086:7d55`, revision 08 |
| Kernel | `7.2.9-200.fc44.x86_64` |
| Render node | `/dev/dri/renderD128` |
| ICD | `/usr/share/vulkan/icd.d/intel_icd.x86_64.json` |
| ANV/Mesa | `26.2.3-1.fc44` |
| Vulkan loader / device API | 1.4.341.0 / 1.4.354 |
| libva / iHD | `2.23.0-3.fc44` / `26.1.5-1.fc44` |
| Driver selection | `LIBVA_DRIVER_NAME=iHD`, `LIBVA_DRIVERS_PATH=/usr/lib64/dri-nonfree` |
| FFmpeg | `8.1.3-1.fc44` |
| libavcodec / libavformat | 62.28.103 / 62.12.103 |
| libavutil / libswscale / libswresample | 60.26.103 / 9.5.103 / 6.3.103 |
| FreeType | `2.14.3-1.fc44` |

P0 receipt, actual library paths/hashes, FFmpeg configuration, loaded driver
probes, `ldd` and ELF dynamic receipts are retained. No dependency is missing;
there is no developer-home RPATH. libva, FreeType, libc/libstdc++ and FFmpeg
transitive libraries are host dependencies. Vulkan loader/ICD are also probed
at runtime, not inferred solely from `ldd`. Canonical tests explicitly unset
inherited `LD_LIBRARY_PATH`, proving no stale C# developer library path is
required. This is a tested-stack receipt, not all-Linux/all-Intel support.

## Installation, shaders, assets and licenses (items 17–22)

The minimal archive contains one executable, README/release notes, generated
support tables, project LICENSE, THIRD_PARTY_NOTICES, GPLv3 text, RC status and
receipt, Cargo dependency notices and Rust standard-library notices. Its 130
files contain no native `.so`, external shader directory, font file, media
fixture, replay dump, soak evidence, fault artifact or temporary build cache.
Native dependencies are installed separately. No AppImage/RPM/Flatpak system
was introduced. Artifact-directory, `/tmp` and home-directory media invocations
all pass; source-tree/working-directory independence is verified.

Each clean build generated 25 actual SPIR-V modules. All 50 path/hash receipts
pass `spirv-val --target-env vulkan1.3`. Project GLSL is compiled by locked
shaderc 0.10.1 and embedded via build output; unknown binary blobs are not used.
Built-in BASIC_FONTS is font8x8 0.3.1, MIT, embedded; explicit FreeType fonts
are user supplied. Test-only fonts are not distributed. The font source SHA is
`14a2b437b441d2f5ce5c6a8a3663530d3eb92f573d5ffb3864398e9994e0ca62`;
its MIT text SHA is
`47d9e9e9a4c54af113e351891bff6cf732793880b83506af6d339976c681a1b5`.

The notice collector covers 60 actual normal Cargo dependencies and 107
upstream license/notice texts, including non-MIT/Apache declarations. Rust
standard-library COPYRIGHT-library.html and its 12 referenced license texts
are included. Upstream ffmpeg-sys-next declares WTFPL without packaging a
license text; that fact is recorded, not replaced with an invented edition.
Exact application source and 86 Linux-resolved Cargo source packages are
retained separately, not inserted into the runtime archive.

**License/source gate remains incomplete.** Actual libavcodec/libavformat/
libavutil report GPL version 3 or later (`--enable-gpl --enable-version3`
and libx264). Project source remains MIT; this candidate conservatively treats
the combined executable distribution as GPL-3.0-or-later rather than claiming
MIT-only or LGPL-only. [FFmpeg's licensing guidance](https://ffmpeg.org/legal.html)
explains why its GPL-enabled configuration differs from the LGPL checklist.
Not bundling the native `.so` files is not assumed to remove source obligations.

The exact FFmpeg source RPM is retained (SHA
`285fe17863d31e2bec872e4a53456560867c716645fd44c4660bb1fa2ec968e9`).
That alone is not a complete corresponding-source companion/provision plan
for linked non-system native components. Missing examples are exact
`x264-0.165-5.20250608gitb35605ac.fc44.src.rpm` and
`fdk-aac-free-2.0.3-2.fc44.src.rpm`. The latter is not described as non-free
merely because FFmpeg enables libfdk-aac. Complete bounded source provision
must be resolved before release; this report is not legal/patent clearance.

## Support and hardware matrix (items 23–24)

[Production support](production-support.md) is generated from
`tests/support/production-support-v1.json`, version 1.1.0; its 25 contract
cases include input codec/depth/dynamic range, output intent/codec/depth,
audio, container and Supported/ConditionallySupported/Unsupported/Unqualified
states. It is not a Cartesian-product qualification promise. The existing
support checker passes; planner/runtime/CLI truth was not replaced by a
handwritten second matrix.

The preserved five-stack registry is `tests/portability/qualified-stacks.json`:

| FFmpeg | ANV/Mesa | iHD | Qualification scope |
| --- | --- | --- | --- |
| 8.1.3 | 26.2.3 | 26.1.5 | Canonical |
| 8.1.2 | 26.2.3 | 26.1.5 | Recorded alternate |
| 8.1.1 | 26.2.3 | 26.1.5 | Recorded alternate |
| 8.1.3 | 26.0.3 | 26.1.5 | Recorded alternate |
| 8.1.3 | 26.2.3 | 25.4.6 | Recorded Tier 1B-P policy |

Those historical C receipts use the same `8086:7d55` GPU, kernel
7.2.8-200.fc44 and libva 2.23. D-1 and this D-2 canonical receipt use kernel
7.2.9-200.fc44; this does not retroactively qualify every alternate combination
on every kernel. Historical FailedQualification entries, including initial
iHD 25.4.6 H.264 strict-oracle failures, remain preserved. Later Tier 1B-P
qualification does not rewrite them. Absence from the matrix is not proof of
incompatibility. Existing H.264/Tier 1B-P and stack-scoped oracle rules remain.

## Actual final-artifact smokes (items 25–33)

| Smoke | Production path / output | Audio | Result |
| --- | --- | --- | --- |
| A | H.264 VAAPI → Vulkan/NV12 → H.264 High, BT.709 limited 8-bit | One AAC copy | PASS |
| B | HEVC Main10 PQ VAAPI → Vulkan/P010 preserve → HEVC Main 10, BT.2020/PQ/BT.2020 NCL limited | None | PASS |
| C | Legal-domain AV1 PQ VAAPI → Vulkan/BT.2446 → P010 SDR → HEVC Main 10, BT.709 limited 10-bit | Two AAC copies | PASS |
| Software | Software decode → CPU → H.264 Constrained Baseline, BT.709 limited 8-bit | None | PASS |

Each output is 300 frames, 1920×1080, 50 fps, video duration 6 seconds.
Full video/audio decode with error-fatal settings succeeds. Video PTS equals
`i/50`, durations equal `1/50`, DTS is strictly increasing. C has no leaked
HDR mastering/CLL metadata at stream or decoded-frame level. Source identities,
exact generator and output commands, probes and output hashes are in the
machine-readable receipt. Each AAC track has 283 packets; payloads, PTS/DTS,
duration, language, default disposition and MP4 track-title/name are preserved.
A is eng/default; C is eng/default plus jpn/non-default.

All A/B/C complete files are byte-identical to release-layer pre-change
controls on identical input. No production media semantics changed. Whole
output SHA-256 values:

```text
A 052fb2881a8c38b5ef841bf246885826a7ab4c6f69f683383b97d75a8f574bac
B 6aca8ad25ea139f4788f61a49992d6994479ebf5fb9d1bed7fe6ee94a5d172bd
C 2702df0f9745642ed7823ba33cff4a96464dc4a3250eec2016bd61d3af85707c
```

Bad input, unwritable output, absent VAAPI node, absent Vulkan ICD, absent iHD
and absent font all give clear non-panic diagnostics and no committed invalid
output. HLG/full-range/BT.2020 SDR, invalid H.264 10-bit, unsupported depth
conversion and PQ→H.264 without explicit SDR intent reject with expected
structured stage/category. Existing targets survive failed jobs unchanged;
successful jobs replace atomically with verified complete output.

Real Path C dual-AAC cancellation after 524,332 staging bytes gives exit 130,
no committed incomplete target and no staging residue. This is a short release
smoke, not a replacement for preserved D-1 long-progress cleanup evidence.

## Verification, documentation and limits (items 36–38)

Fmt, strict workspace/all-target Clippy, diff whitespace and support-contract
checker pass. Final serial workspace run: 257 passed, zero failed, 102 ignored;
ignored native opt-ins are not claimed as run. Separate actual release GPU
tests establish the smokes above. Initial parallel FD-census test interference
and SIGINT test timeout under concurrent cold builds are retained as failed
test observations; relevant idle/serial retests and the final full serial run
pass. Assertions were not weakened. Default parallel CI is not claimed green
on the basis of the serial run.

Changes are release version/help/logging/dependency declaration, generated
support documentation and packaging/notices; Rust lint edits are equivalent
unsigned parity checks/short-circuit syntax, not media redesign. No algorithm,
planner policy, mux timing or ownership fix was introduced. Previously sealed
100k soaks, 17×3 retained paths, compatibility and portability evidence are
reused, not unnecessarily rerun.

README, generated support documentation, architecture boundary and
[RC notes](release-notes-v2-rc1.md) cover build/install/invocation, GPU needs,
PQ preserve versus explicit PQ→SDR, compressed copy audio and common failures.
New files are required release notes/notices, a notice collector, one actual
artifact smoke runner, this report, qualification JSON and internal evidence
archive. No new public option/type/architecture abstraction was introduced.

Known user-facing limits remain: recorded Linux/Intel stacks only; single-job
CLI, not a daemon/GUI/persistent worker; persistent multi-job memory unqualified;
PQ/BT.2020 NCL/limited only; decoded <=1000 cd/m² qualified tone-map domain;
HLG/full range unsupported; no general audio transcode, VFR audio-copy promise
or arbitrary container combination. Ordinary MP4 sample indexes and bounded
audio allocator retention can grow working set. No constant-memory, real-time,
24/7 or complete driver-internal-resource claim is made.

## Evidence and closure (items 41–44)

`tests/release/stage54d2-release-qualification.json` carries the full source,
artifact, hardware, features, shader and actual smoke receipts. Internal
`tests/release/evidence/stage54d2-release-evidence.tar.gz` preserves build/test
logs, initial failure logs, exact patch, all validated shader files, stack and
license receipts, generator identities and install checks (SHA-256
`9d7e4cbc54a6be068de648b16510c94da413b1258d4c75048099c6a9031f483b`).
It is not shipped in the runtime archive. The designated local candidate and
source companions are in `target/releases/stage54d2-rc1/`.

Final statuses: **D-2 NOT SEALED; D NOT SEALED; 5.4 NOT SEALED; v2 RC not
public-release-ready.** Only native corresponding-source provision remains
blocking. No next feature stage was started.
