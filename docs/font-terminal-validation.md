# System font and terminal validation

Status: **Completed**. The original qualification used source version
`2.1.0-dev`; this identifies historical evidence, not a current release.
This is the historical font-discovery/terminal result before font-aware automatic
row selection. Current scalable-font automatic geometry is described in
[fonts](fonts.md); explicit grids retain the existing raster policy.
This is a development qualification, not a new redistributable package.
The historical release artifacts and reports are unchanged.

## Implementation boundary

`--font` now resolves builtin, explicit file, or Linux Fontconfig pattern.
Existing/path-like requests take precedence and cannot fall back to a family
name. Canonical file and collection index feed the existing FreeType builder;
the ramp/geometry and rasterization rules remain unchanged. An explicit
`--font-face-index`, including zero, is accepted only for a file request.
Fontconfig's returned face is authoritative for a name request. Matching uses
the configured aliases/cache, with no directory scan, glyph fallback, shaping,
font-size option, density calibration or name-keyed atlas cache.

Fontconfig is runtime-loaded: builtin and explicit files work when discovery is
unavailable. Parsed and matched native patterns are destroyed once; the shared
Fontconfig configuration/cache can remain process-resident. No discovery occurs
per frame. Requested name, selected name, source, canonical path and face index
are retained in diagnostics; the JSON font record also includes actual ramp and
tile geometry.

Terminal mode is selected from stderr TTY, `TERM`, `--verbose` and
`--no-progress`. Normal startup uses five application lines; native warnings are
not suppressed and may add lines. Normal completion is two English stdout lines.
Verbose output groups existing metrics and preserves CPU-wall/GPU-timestamp
caveats. Inspection modes have neither startup nor progress.

One CLI-owned monochrome line updates at most 10 Hz after successful encoder
acceptance/drain, through a passive optional callback shared by all encoder
paths. It cannot terminate the pipeline. A positive stream frame count supplies
the total, capped by `--max-frames`; unknown totals have no fake percentage/ETA.
Hidden modes attach no update callback. There is no UI sleep or extra ticking
thread. RAII clears the line on success, error and cancellation. FFmpeg native
log colors are disabled before native initialization to keep plain/verbose logs
free of ANSI colors.

## Dependencies and source identity

New direct dependencies are `yeslogic-fontconfig-sys` **6.0.1** (Linux, `dlopen`)
and `indicatif` **0.18.6** (default features disabled), both MIT. The native
binding is the runtime-loading backend used by
[fontconfig 0.11](https://docs.rs/fontconfig/0.11.0/fontconfig/); direct calls
preserve complete pattern syntax and explicit pattern ownership.
[Indicatif](https://docs.rs/indicatif/0.18.6/indicatif/struct.ProgressBar.html)
provides the single-line draw target, not a terminal UI framework. Installed
fonts and Fontconfig are not bundled. Existing FreeType and FFmpeg selections
remain unchanged; no redistribution qualification is asserted here.

Qualification uses base commit `9104b950e6f83ca54c5c666b3d909e0cd6a6791d`
plus the recorded font/terminal working-tree source hashes. The qualification
tree was intentionally uncommitted. Cargo.lock SHA-256:
`f94c72231ab531f246514d7c8cb8fb814931dd84345f2401ff893c46bce0e7e4`.
Tested development binary SHA-256:
`2c2da775de10be79f1340583ff5fc458a1483d8d8a4bd6f3047f4b867a436f0a`.
Release build command:

```sh
cargo build --release --locked --workspace --features asciiflow-cli/lgpl-prebuilt
```

## Verification

The [machine-readable receipt](../tests/ux/font-terminal-validation.json)
retains exact commands, output hashes, probes, terminal transcripts, control
recipes, source hashes and intentionally failed diagnostic controls.

| Gate | Observed result |
| --- | --- |
| Workspace tests | 265 passed, 0 failed, 103 ignored; ignored tests are not counted as passes |
| Strict all-target Clippy, formatting, whitespace, support-contract checker | PASS |
| Builtin A/B/C full-GPU short paths | 300 frames each, byte-identical to retained rc.2 outputs |
| Decode-back | Full decode, 1920×1080, 50 fps, exact timestamps `i/50`; SDR/PQ metadata and audio topology/copy preserved; C has no HDR metadata leakage |
| Explicit font before/after | Byte-identical output using Liberation Mono face 0 |
| System family versus explicit file | Identical atlas test and complete output; canonical face unchanged |
| System alias | Actual host match expected-rejected; isolated configured alias positive test PASS |
| CPU/Vulkan FreeType pixel parity | One explicitly executed ignored test PASS on Intel; host P010 compute test, not decoder interop evidence |
| Fontconfig absent | Name request rejects clearly; builtin and explicit file still pass |
| Named font plus explicit face 0, missing path | Early diagnostic, no target/staging output |
| Real terminal | Interactive progress observed and cleared; stdout completion stays plain |
| Redirected / no-progress / verbose | No live bar or ANSI; normal two-line or verbose grouped completion |
| Runtime failure after progress | File-size limit armed after progress; exit 1, line cleared before error, target/staging absent |
| SIGINT after progress | Exit 130, line cleared before cancellation, target/staging absent |

Runtime host: Intel Arc Meteor Lake (`0x8086/0x7d55`), kernel
`7.2.9-200.fc44.x86_64`, render node `/dev/dri/renderD128`, Intel Vulkan ICD,
iHD VAAPI, and retained LGPL FFmpeg 8.1.3 shared libraries. Exact child runtime
environment and dependency paths are recorded; probes/decode-back use the host
FFmpeg tools separately. Fontconfig runtime is **2.17.0**.

The native default `monospace` match was Noto Sans Mono's variable font and
failed the existing FreeType fixed-width gate. Liberation Mono passed. The
positive alias test uses an isolated Fontconfig configuration, not a host-wide
change. The failed native-font and initial diagnostic setup controls are kept
as observations, never rewritten as passes. A pre-initialization file limit
prevented Vulkan startup and therefore did not satisfy the progress-error gate;
the later limit-after-progress experiment did.

## Change inventory and limitations

Source changes: workspace version/lockfile; CLI dependency, arguments, font
initialization, startup/summary, progress ownership and CLI regressions; font
dependency/exports/resolution; the encoder's optional passive observer.
Documentation changes: README, fonts and this report. Earlier README/testing
cleanup changes are preserved.

New files are `resolve.rs` (font discovery), `progress.rs` (single-line lifecycle),
this requested report, and its JSON evidence receipt. No new media abstraction,
fixture font or bundled native component was added.
New types are `FontSource` (resolution origin), `ResolvedFont` (owned resolved
identity without native handles), and `TerminalProgress` (CLI line cleanup on
every exit). The existing encoder gains one optional observer setter, not a new
pipeline interface.

Production media semantics changed: **No**. Planner, codecs, HDR/color algorithms,
interop, mux/audio ownership and output safety are unchanged. The only media
addition is the optional progress observation. A/B/C and explicit-file output
identity give direct regression evidence; this is not a rerun of the full
soak/portability campaign. System font availability and acceptance depend on
local configuration and FreeType. Raster identity is stack/font scoped.
The frozen historical package does not contain system font discovery or the
terminal progress UI.

No production blocker remains for the recorded font/terminal checks. Future
changes use targeted regression testing, rather than a development-phase checklist.
