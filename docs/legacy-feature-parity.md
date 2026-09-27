# Former C# implementation: feature follow-up for Rust

AsciiFlow now has one implementation: the Rust workspace. The former .NET
source, tests, solution and bundled native libraries were removed from the
active tree; Git history preserves their exact code. This page is a **feature
inventory**, not a second product manual or a promise that old behavior is
already supported. When a feature is considered for Rust, write a new contract
and tests against the current media, color and failure-safety architecture.

| Former user-visible behavior | Current Rust status | Follow-up decision / acceptance boundary |
| --- | --- | --- |
| Color and monochrome ASCII, BT.709 luminance mapping, standard/detailed/literal ramps | Implemented with NV12/P010 CPU/Vulkan paths; the default ramp polarity has been corrected for black-background rendering | Rust uses a limited-range smoothstep glyph LUT. The former C# mapper used a different piecewise S-curve on full-range gray values, so pixel-for-pixel tone-curve parity is **not** claimed. Evaluate any tone-curve change against Rust color/polarity tests, not old Skia/RGB hashes. |
| Bounded decode/process/encode, cancellation, staged output replacement, actual completed-frame summary | Implemented with different ownership and failure contracts | Preserve [failure semantics](failure-semantics.md) and hardware resource bounds when extending the pipeline. |
| Compatible compressed-audio passthrough | Implemented for MP4 with `auto`, strict `copy`, and `none` | Current audio copy requires the existing CFR video timeline. Variable-rate or discontinuous input needs an explicit future timeline policy; never silently retime audio. |
| FFmpeg content-based input/container probing | Present, but the qualified Rust input matrix is narrower than the old general FFmpeg claim | Qualify additional container/codec/color combinations individually; do not infer full support merely because FFmpeg opens a file. |
| `.mp4`, `.m4v`, `.mov`, `.mkv`, `.avi`, `.ts`, `.m2ts` H.264 output and `.webm` VP9 output chosen by extension | **Not implemented**. Rust production output is MP4 with H.264, HEVC or AV1 | If needed, add an explicit container/codec capability matrix, muxer-aware audio policy, transactional-output tests and decode-back fixtures. WebM/VP9 requires its own encoder and timeline qualification. |
| User-selected output frame rate (`--framerate`) | **Not implemented**; Rust uses the source's rational frame rate and requires CFR-compatible timestamps for audio copy | Specify drop/duplicate/interpolation and audio-offset policy before adding a flag; test noninteger rates, VFR/discontinuities and cancellation. |
| Named system-font family, pixel size and cross-platform font fallback | **Not implemented**. Rust has the built-in 8×8 font or an explicit monospaced font file/face; invalid explicit fonts fail closed | Consider font discovery and fallback only with deterministic selection, portable fixture tests and bounded atlas geometry. Do not introduce silent substitution for an explicit file. |
| `speed`, `balanced`, `quality` encoder modes and VP9 tuning | **Not implemented as user presets**. Rust has qualified codec-specific settings and measurement gates | Add per-codec profiles only after measuring quality, throughput, size, timestamps and repeatability on the target encoder; keep default behavior stable. |
| Estimated progress when `nb_frames` is unavailable | Rust preserves unknown frame count and reports actual completion; equivalent estimate behavior is not established here | If requested, label estimates clearly, never use them as completion truth, and test unknown/incorrect metadata. |
| Legacy `-i/-o` options and implicit default output path | Rust uses positional input/output; output is optional only for capability/plan inspection | Treat CLI compatibility as an explicit product decision, not an automatic migration requirement. |

For future work, decide requirements in this order: (1) which additional
container/codec combinations users actually need, (2) the video/audio
timeline policy for rate changes or discontinuities, then (3) optional
font-discovery and encoder-quality controls. Each decision should get a
source-backed contract and regression fixtures before implementation. The
old implementation's benchmark numbers are not Rust performance targets.

The former C# README and architecture description are not authoritative for
Rust. Current commands and support boundaries are in the root [README](../README.md),
[architecture](architecture.md), [codec contract](codecs.md),
[font contract](fonts.md), [audio contract](audio.md), and
[testing policy](testing.md). Historical stage and media-baseline revision
identifiers remain in their reports because they identify distinct evidence,
not parallel application versions.
