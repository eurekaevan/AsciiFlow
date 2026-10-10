# LGPL prebuilt release qualification

## Decision

**Completed.** This records the qualified 2.0.0-rc.2 LGPL prebuilt candidate
and inherited reliability results. It is historical release provenance, not
a future development plan. This is a RedistributableCandidate,
not a universal legal/patent/compatibility guarantee. Nothing was published,
committed or tagged; no feature stage or GPL Full flavor was started.

This supersedes the public-release strategy, not the historical
rc.1 qualification report. Its original
NOT SEALED/source-provision limitation and all earlier FAIL evidence remain.

## Profile and contract (final report items 1–6, 18)

The opt-in Cargo feature `asciiflow-cli/lgpl-prebuilt` forwards one restriction
to media. Ordinary developer builds are not subject to this profile unless explicitly selected.
The existing machine-readable base contract now has an additive
`release_profiles.lgpl-prebuilt` constraint: `encode=software` is Excluded due
to libx264. The four base support states, cases and evidence are unchanged.
The same renderer generates profile availability next to base status; there
is no duplicated independent support table or fifth base status.

`portable-h264-sdr` remains historical Supported/PASS under a GPL-capable build
context, excluded only from the official LGPL prebuilt. libx264 backend code
and historical tests remain. The official LGPL prebuilt release does not
support software H.264 encoding; this is not a claim that AsciiFlow has no
software encoder in any build.

Probe still actually looks up libx264. The release constraint then marks the
route unavailable, showing `runtime libx264 present: false` plus the precise
profile reason. Explicit software encoding rejects at Planning/InvalidConfig
before output commit. Normal auto planner rules still choose eligible VAAPI;
if neither software nor hardware is available, no qualified route is selected.
No new fallback is introduced. Generic H.264 encoder lookup was removed from
both software capability probe and actual software encoder selection; named
libx264 selection remains for suitable developer builds.

Tests cover base PASS preservation, profile software rejection, hardware
eligibility, auto hardware selection and rejection when no eligible encoder
remains. An actual LGPL-library negative test, with h264_vaapi present and
libx264 absent, executes and passes: no generic substitution and no output.
Final release artifact's structured software rejection also passes. Help
accurately describes build/profile-dependent software availability; capability
output reports the exclusion rather than simply “encoder not found.”

## FFmpeg dependency (items 7–17, 34–35)

Exact unmodified FFmpeg 8.1.3 source: retained `ffmpeg-8.1.3.tar.xz`, SHA-256
`7138d28c96d9d3e3af4ee3d8cad72741f8ffb40da90c1112235dea3ecd3178a3`.
No distro patches are applied. The build is isolated under
`target/releases/stage54d2-rc2/ffmpeg-lgpl-v2/`; system libraries are not replaced.
The compiler identity and full expanded configure command are retained and
included in the source companion. Recipe:

```sh
bash third_party/ffmpeg/scripts/build-lgpl-prebuilt.sh \
  target/stage54d1b-mov-attribution/source/ffmpeg-8.1.3.tar.xz \
  target/stage54c1-toolchains/devel/usr \
  target/releases/stage54d2-rc2/ffmpeg-lgpl-v2
```

Complete configure option set (paths stand for the recorded exact expanded
prefix/development tree in `sources/configure-command.txt`):

```text
./configure --prefix=PREFIX --libdir=PREFIX/lib
--enable-shared --disable-static --disable-gpl --disable-nonfree
--disable-version3 --disable-autodetect --disable-doc --disable-debug
--disable-avdevice --enable-avfilter --disable-filters --disable-network
--enable-pthreads --enable-vaapi --enable-libdrm --enable-libdav1d
--disable-encoders --enable-encoder=h264_vaapi,hevc_vaapi,av1_vaapi
--disable-decoders --enable-decoder=h264,hevc,av1,libdav1d,aac
--disable-demuxers --enable-demuxer=mov
--disable-muxers --enable-muxer=mp4,null
--disable-protocols --enable-protocol=file
--extra-cflags=-IDEVEL/include -IDEVEL/include/libdrm
--extra-ldflags=-LDEVEL/lib64
```

Not `--disable-everything` plus guessed dependencies: the capability inventory
is derived from current contract and actual codec/mux/hwcontext calls. Internal
parsers/BSFs and dependency-selected VAAPI decoders remain available; enabling
them does not promote unsupported/unqualified product routes. Matroska input
is Unqualified and output Unsupported, hence not a required release demuxer.
MOV demuxer serves qualified MP4; this does not promote MOV input tuples.

`tests/release/required-ffmpeg-capabilities.json` and the actual receipt verify:

| Requirement | Actual availability |
| --- | --- |
| Software H.264 / HEVC decode | Built-in h264 / hevc |
| Software AV1 decode | BSD libdav1d; native av1 alone is hardware-only |
| VAAPI encode | h264_vaapi, hevc_vaapi, av1_vaapi |
| Audio | AAC packet copy/parsing; AAC decoder available for probing; no audio encoder |
| Container/protocol | mov demux, mp4 mux, local file protocol |
| Parsers / required BSF | h264/hevc/av1/aac; aac_adtstoasc |
| Frames / conversion | VAAPI/DRM, NV12/P010/YUV420 8/10-bit, swscale |
| Explicit external enables | libva, libdrm, BSD libdav1d only |

`ffmpeg -L`, `-buildconf`, actual avcodec_license(), avformat_license() and
avutil_license() all attest **LGPL version 2.1 or later**. Configuration macros
for GPL, nonfree, version3, libx264, libx265, libfdk_aac and libxvid are zero.
Codec lists and recursive dependency checks independently prove those external
codecs are neither enabled nor linked. The capability differential preserves
required routes; the numerous removed distro codec/container capabilities are
outside this release's applicable contract, including deliberately excluded
software encoding. Exact added/removed/preserved inventories are retained.

Only application-needed libavcodec/libavformat/libavutil/libswscale are shipped.
avfilter is enabled solely because upstream ffmpeg CLI requires it for `-L`
proof; CLI/tool-only libraries are not shipped. Actual binary ELF uses
`DT_RPATH=$ORIGIN/lib`, covering transitive bundled libav* without a source-tree
path. External /proc maps during real C jobs and clean `ldd` attest package
library paths/hashes, not system RPM Fusion libav*. Relocated archive uses its
own libraries too. No x264/x265/FDK/Xvid appears in ELF or driver-loaded closure.

Host runtime/driver receipts use Intel Meteor Lake `8086:7d55`, kernel
7.2.9-200.fc44, ANV/Mesa 26.2.3, libva 2.23 and iHD 26.1.5. Host components
are not bundled. FreeType uses its FTL alternative with attribution/text;
GCC/libstdc++ system-runtime exceptions are not GPL-only codec dependencies.
Whole-RPM aggregate licenses may also cover unrelated tools/docs and are not
misrepresented as each loaded library's code license. No broad host-package
audit or universal legal guarantee is asserted.

The exact FFmpeg source archive, no-patch declaration, build recipe, configure
command, compiler identity, upstream license and SOURCE.md are inside the
binary archive. Compatible dynamic libraries can be replaced; reverse
engineering for debugging modifications is not forbidden. Sources must
accompany any later publication with equal access. This bounded distribution
strategy follows [FFmpeg's LGPL checklist](https://ffmpeg.org/legal.html).
MIT project LICENSE is unchanged. Cargo's 60 normal dependencies/107 notice
texts, font8x8 MIT and Rust standard-library notices are included; no x264/FDK
source is required for this closure because neither enters it.

## Build/source/artifact identity (items 30–33, 36–39)

The base commit remains `5dbe199825cb7aad8093e3cd8f940ba115a497b8` plus the exact
archived dirty snapshot; no clean-tree claim or new commit is made. Compiled
source fingerprint:
`7b2ecb317526c3e145d65731e52bb630e40c9c2739e5cc1f47823ffc6c7d07f0`.
Source tar SHA:
`55574120f0cc75ad4ee83dd2c2098b2a1a756a97338b2cf1cffffc9d55d7d6a7`.
Exact tracked diff SHA:
`b41eb6d48cb23b54aaed7ee6e7e99a4790eb7168a2b4066ea3cacea69925ec47`.
Cargo.lock, file/mode/hash inventory and Git status are in the receipt.
Only generated final report/manifest/evidence files are excluded from that
build-input snapshot. Final package-only attribution/receipt additions do not
change the tested binary or libraries.

Two fresh independent AsciiFlow source/build trees used the same fixed LGPL
sysroot artifact:

```text
FFMPEG_DIR=isolated/sysroot
RUSTFLAGS='-C link-arg=-Wl,--disable-new-dtags,-rpath,$ORIGIN/lib'
cargo build -j 4 --release --locked --workspace
  --features asciiflow-cli/lgpl-prebuilt --message-format=json
```

Build 1 passed in 264 seconds, build 2 in 266, zero unexpected warnings.
Each passed version/help and actual short GPU smoke. Their ELF hashes happen
to match; this is not a bit-reproducible-build promise. D-1 fault/measurement
features are absent. All actual shaders pass spirv-val Vulkan 1.3: 25 per
build, 50 records, no historical fixed inventory threshold.

| Identity | SHA-256 |
| --- | --- |
| rc.2 binary | `bac42dceb1c1261a4af9e0a5e7853fd9f1c723657c9e91d82a34657a9ad03fb8` |
| Final archive | `c8412bd22db576291428d9ee02029d5837f2232a8fff02553283c28f87034a62` |
| FFmpeg bundle companion | `e81e5296ea37a61f0b452aa4475cff79e678c48a0f0260c62975c99d690425f9` |
| rc.1 archive, unchanged | `d2f62b91234abc2da09cba53bfb90cc759b3cb720e6f47fbc77bc9d12d22199b` |
| rc.1 binary, unchanged | `07ae232d997593d2fee5292c3659caf1cb02b7f25bce118a1f1e453a429e8d70` |

Final archive:
`target/releases/stage54d2-rc2/asciiflow-2.0.0-rc.2-linux-x86_64.tar.gz`,
18,448,401 bytes. It contains runtime binary/libraries, necessary embedded
assets, licenses/notices, receipts and the exact FFmpeg source companion; no
100k fixture, replay dump, test hooks or soak cache is shipped. Fresh extraction
outside the source tree passes version/help and actual media decode-back, with
the same designated binary SHA. rc.1 remains InternalQualificationOnly,
Superseded, NotForRedistribution; its archive/evidence are not overwritten.

## Production/oracle verification (items 19–29)

| Gate | Result |
| --- | --- |
| A SDR → H.264 VAAPI | PASS; BT.709 limited 8-bit, single AAC copy |
| B PQ preserve → HEVC10 VAAPI | PASS; BT.2020/PQ/BT.2020 NCL limited, no audio |
| C PQ→SDR → HEVC10 VAAPI | PASS; BT.709 limited 10-bit, dual AAC copy, no HDR metadata leak |
| Software decode → CPU → VAAPI H.264 | PASS, not software encoding |
| Explicit software encoding | Planning/InvalidConfig, profile-specific diagnostic, no output |
| Retained production | All 17 applicable hardware paths ×3 = 51 PASS |
| Excluded base case | portable-h264-sdr, historical PASS preserved, not one of the 17 |
| H.264 / retained oracle | Existing strict hash/packet/pixel/metadata gates unchanged and PASS |
| Short mux determinism | Three real dual-AAC C jobs, identical whole file and packet ordering |
| Short mux tiers | Tier 1A/1B/1C/2 PASS, Tier 3 MATCH; existing comparator |
| Failure / safe output | Bad input, unavailable GPU/driver/font, unsupported modes and unwritable output reject safely; existing targets preserved on failure, atomically replaced on success |
| SIGINT | Exit 130 after 524,332 staging bytes, no incomplete committed target or staging residue |
| Relocation / working directory | Fresh archive under /tmp; artifact directory, /tmp and home cwd PASS |

A/B/C and software-decode smoke fully decode 300 frames, 1920×1080, 50 fps,
6 seconds. PTS is exactly i/50, frame duration 1/50, DTS increasing. AAC copy
checks payload, timestamps/duration, language, default disposition and title;
single/dual tracks pass (283 packets per track). A/B/C complete output hashes
also match rc.1's technical smokes on identical inputs. Removing FDK does not
touch packet-copy ownership or turn audio into an encode path.

The preserved H.264 historical and Tier 1B-P cross-driver rules are unchanged;
no new cross-driver matrix is claimed. The new executable's build attestation
is not transferred from an old binary merely because outputs are identical.
D-1 long-soak/resource evidence remains valid under the requested scope;
same FFmpeg version/relevant implementations and strict 51-run regression
pass justify not rerunning 100k, memory attribution or full portability corpus.

Static checks: ordinary developer `cargo test --workspace --locked --
--test-threads=1` 258 PASS/0 FAIL/102 ignored; historical software tests remain
in that build context. Separate actual LGPL profile negative test: one PASS.
Strict Clippy passes both workspace default/all-target and LGPL CLI/media
all-target configurations. Fmt, diff whitespace and generated support checker
pass. Ignored tests are not claimed as executed.

Initial failures are kept, not rewritten: missing proof CLI from the first
avfilter-disabled FFmpeg build, /tmp quota failures from first clean-build
attempts, a zero-test exact-filter invocation and initial schema-v1 host probe.
Fresh corrected builds, executed negative test and explicit schema-v2 isolated
P0 receipt supersede those attempts. Only this task's failed temporary build
caches were removed; logs and all historical artifacts are preserved.

## Evidence and limitations (items 40–44)

Full machine report: `tests/release/lgpl-qualification.json`.
Internal proof archive: `tests/release/evidence/lgpl-rc2-evidence.tar.gz`.
It preserves exact patch/source identity, both clean build logs/features,
validated shaders, actual library licenses/closure/maps, hardware receipts,
all relevant smokes, retained oracles, mux tiers and failed attempt logs.
No placeholders or remaining release gates remain for this recorded candidate.

**Historical LGPL candidate qualification completed within the recorded scope.**
Still limited to recorded Linux Intel stacks, one process/one job, PQ limited
BT.2020 NCL and qualified <=1000 cd/m² explicit tone-map domain, qualified
MP4/CFR audio copy. HLG/full range and official software H.264 encoding are
unsupported. Persistent multi-job memory remains NOT QUALIFIED; ordinary MP4
indexes/bounded audio allocator retention are not a constant-memory claim.
No 24/7, real-time or universal legal/patent guarantee; no new feature stage.
