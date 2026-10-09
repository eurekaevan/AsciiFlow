# AsciiFlow 2.0.0

**FINALIZED — Final qualified v2 engineering milestone.** Stage 5.4 remains
SEALED; AsciiFlow v2 is COMPLETE. No new feature stage, publication, tag or
commit was created by this finalization.

## Identity and change boundary

The final artifact inherits the [sealed rc.2 qualification](lgpl-release-qualification.md):
qualified source semantics + version/finalization-only changes + final artifact
smoke. This is not a new full media qualification.

Base commit: `5dbe199825cb7aad8093e3cd8f940ba115a497b8`. The tree is **dirty**,
including preserved prior engineering work; no clean-commit identity is claimed.
The exact final build snapshot, binary patch, status, file hashes and submodule
state are retained under `target/releases/asciiflow-2.0.0/` and in the
[machine-readable final receipt](../tests/release/asciiflow-2.0.0-final.json).

| Source receipt | SHA-256 |
|---|---|
| Frozen source fingerprint | `96c2a27a204d72b54046dd97bcf7afddf99c47a20ac00ebb577eb527751e17ea` |
| Exact source archive | `138216c187f8868c4fae6411d6d378616cf6fa0ff7f6e67099f8b9e552ea93d8` |
| Dirty tracked patch | `c71fc5cfa293e0c43184443bda11beaf70f0dd5c5583e7668bb053de2dcdd8c2` |
| Cargo.lock | `458ae62e9f13ac6f2b33ac44967b2bd6794218552edf2941cf0a118fb22cad19` |

Compared with the qualified source snapshot, the only changed existing files
are `Cargo.toml`, `Cargo.lock` and `README.md`. The workspace version and seven
local lockfile package versions changed from `2.0.0-rc.2` to `2.0.0`; external
dependencies did not change. README gains the final milestone pointer.
**No production semantic code, support contract, FFmpeg profile or shader changed.**

The three new durable files are this milestone document, the final JSON receipt
and its evidence archive: respectively the human summary, machine identities
and actual build/test/validation logs. There are no new production types or
abstractions. These generated final receipts are explicitly excluded from the
build source fingerprint to avoid circular hashes.

Version occurrence classification is archived with the evidence: Cargo version
sources were current identities; rc.2 release notes, qualification report,
manifest and old package receipts are historical identities and remain unchanged.
rc.1 remains InternalQualificationOnly / Superseded / NotForRedistribution.
Neither historical artifact, hash, source identity nor qualification result was
overwritten, deleted or renamed.

## Final artifact

Archive: `target/releases/asciiflow-2.0.0/asciiflow-2.0.0-linux-x86_64.tar.gz`.
It contains the final executable, four replaceable FFmpeg shared libraries,
embedded generated shaders, MIT/LGPL license texts, notices, corresponding
FFmpeg source and recipe, and final metadata. It does not ship soak fixtures,
replay dumps or development build caches.

| Artifact | Bytes | SHA-256 |
|---|---:|---|
| `asciiflow-2.0.0-linux-x86_64.tar.gz` | 18,485,184 | `7094c80f94fce304ae6e870f60e8d60a52de26bfd9cadf865730cd7755b5286a` |
| `asciiflow` | 4,136,720 | `5eff7a3425881eb06ca3d97de52ffd867bba71d450b7542008a2f4d37fc5dd05` |

One independent fresh locked Release build passed, with no warnings, in
3m 06s. It used the same FFmpeg sysroot, `$ORIGIN/lib` linkage and production
features as rc.2 (`asciiflow-cli/lgpl-prebuilt`); no measurement/fault hooks
were enabled. Exact command and compiler identity are in the final receipt.
No bit-reproducible ELF build claim is made.

The completed archive was independently extracted into a fresh `/tmp` directory.
Its executable SHA, all 152 unchanged library/license/source inventory entries,
`--version`, identical `--help` and relocated bundled-library resolution passed.
The designated archive therefore carries the same executable that passed the
four final media smokes, with no dependency on the development working directory.

## Final checks

| Gate | Result |
|---|---|
| `--version` | `asciiflow 2.0.0`; no candidate suffix |
| `--help` | PASS; byte-for-byte unchanged from rc.2 |
| A: SDR → VAAPI H.264, single AAC copy | PASS; BT.709 limited, 8-bit 4:2:0 |
| B: PQ preserve → VAAPI HEVC Main 10 | PASS; BT.2020 / PQ / BT.2020 NCL limited, 10-bit |
| C: PQ → SDR → VAAPI HEVC Main 10, dual AAC copy | PASS; BT.709 limited, 10-bit SDR; no HDR metadata leakage |
| Software decode → CPU → VAAPI H.264 | PASS; software decode remains available |
| Explicit software H.264 | PASS; early Planning/InvalidConfig rejection, nonzero exit, no output/staging file or generic fallback |
| SPIR-V | All **25** actual generated modules PASS under `spirv-val --target-env vulkan1.3`; hashes unchanged from rc.2 |
| Workspace tests | **258 passed / 0 failed / 102 ignored**, unchanged ignore policy |
| Strict all-target Clippy | PASS, no warnings |
| Format / diff / generated support contract checks | PASS |

Every successful smoke used an existing qualified short fixture, fully decoded
all video/audio, and verified **300 frames, 1920×1080, 50 fps, six seconds**,
exact video PTS/duration, monotonic DTS and audio topology. AAC payload hashes,
PTS/DTS/duration, language, titles and default disposition were checked against
input. All four complete output files also exactly match their rc.2 hashes.
The final executable—not `cargo run` or an older binary—was tested.

Workspace tests used `--test-threads=1`, preserving the established protection
against concurrent tests' process-wide FD census interference. Hardware tests
that are already ignored by the default workspace suite were not relabeled PASS;
the four explicit real-GPU/software-decode smokes provide this round's media
evidence. The sealed retained matrix, 100k soaks, memory attribution, portability,
compatibility and long cancellation/mux campaigns were **inherited, not rerun**.

## Frozen dependency and license receipt

FFmpeg **8.1.3**, shared **LGPL-2.1-or-later**, is unchanged. Fresh final-binary
`ldd` resolves all four FFmpeg libraries to the package's `lib/`, not the host's
GPL FFmpeg. Realpaths and SHA-256 values are recorded; library payloads exactly
match rc.2. Actual `avcodec_license()`, `avformat_license()` and
`avutil_license()` each report `LGPL version 2.1 or later`.
The recursive ELF dependency closure contains no **x264, x265, FDK-AAC or Xvid**.
The previously qualified dynamically loaded Intel driver receipt is inherited.

MIT `LICENSE`, `COPYING.LGPLv2.1`, `THIRD_PARTY_NOTICES.md`, accompanying
dependency notices, exact FFmpeg source, configuration and build recipe retain
their rc.2 contents. Corresponding FFmpeg source archive SHA-256:
`7138d28c96d9d3e3af4ee3d8cad72741f8ffb40da90c1112235dea3ecd3178a3`.
No patches or new dependency configuration were introduced. This confirms the
existing distribution receipt; it is not a new legal or patent assurance.

## Scope and limitations

Qualification remains limited to the recorded Linux / Intel Arc Meteor Lake
software stacks and **one CLI process → one media job → process exit**.
The official LGPL prebuilt profile retains software H.264/HEVC/AV1 decode,
qualified VAAPI H.264/HEVC/AV1 encode, Vulkan processing and AAC packet copy/none
under existing MP4/CFR conditions. It excludes software H.264 encode; the
libx264 backend and historical software qualification remain in source.
The same machine-readable base contract plus release constraints remain the
support truth; version finalization does not promote unqualified combinations.

PQ preserve retains canonical limited BT.2020 NCL/PQ semantics. Explicit PQ→SDR
is qualified only within the **≤1000 cd/m² decoded source-pixel domain**.
HLG and full-range paths remain unsupported. Persistent-worker memory is
unqualified; ordinary MP4 sample indexes and bounded audio allocator retention
do not imply constant memory. Hardware portability is limited to qualified
stacks. No universal Linux, real-time, 24/7, legal or patent guarantee is made.

**Final status: FINALIZED. Stage 5.4 SEALED. v2 engineering milestone COMPLETE.**
