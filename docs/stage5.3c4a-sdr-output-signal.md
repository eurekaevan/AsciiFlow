# Stage 5.3C-4A — SDR output signal qualification

Status: **SEALED**, qualified on Intel Arc Meteor Lake on 2026-09-30.
This is output-signal/surface qualification, not production availability.
C-4B is justified but not started. There are no remaining C-4A blockers.

## Architecture and boundaries

```text
PQ P010 -> sealed C-3 Vulkan HDR→SDR nonlinear RGB
        -> C-4A BT.709 NCL / limited quantization / 2×2 chroma reduction
           ├─ NV12 (8-bit)
           └─ P010LE (10-bit active samples, stored <<6)
        -> FFmpeg-allocated encoder-style VAAPI writable surface
```

The input is C-3's unchanged finite, bounded [0,1] nonlinear BT.709 RGB,
qualified under P2/N3. C-4A applies neither another limiting pass nor an OETF.
NaN, infinity or excursions are upstream contract violations, not clamped inputs.
The sealed display-power signal is not retroactively described as a camera OETF.

The production planner/CLI, C-1/C-2B/C-3 shader math and existing SDR/PQ-preserve
writers remain unchanged. No encoder is opened, no frame is sent to an encoder,
no mux output is generated, and native AVFrame color metadata is not assigned.
Encoder-*style* pool allocation proves pixels in FFmpeg-owned hardware surfaces;
it does not prove final encode acceptance, metadata signaling or C-4B legality.

## Signal and packing contract

The independent CPU f64 oracle is `asciiflow_cpu::sdr_output`. BT.709 NCL:

```text
Y  = .2126 R + .7152 G + .0722 B
Cb = (B-Y)/1.8556
Cr = (R-Y)/1.5748
```

Evaluation uses the algebraically equivalent neutral-preserving difference form:
`dr=R-G; db=B-G; Y=G+.2126dr+.0722db; Cb=.5db-(.2126/1.8556)dr;
Cr=.5dr-(.0722/1.5748)db`. Canceling the common factors before evaluation
also preserves analytical axis half-code ties; subtracting a rounded Y can
misclassify those ties even in f64. This keeps monochrome chroma exactly neutral.
The BT.2020 source matrix is not reused; red-luma negative tests detect it.

| Format | Y code | Cb/Cr code | Legal active ranges | Storage |
| --- | --- | --- | --- | --- |
| NV12 | 16+219Y | 128+224C | Y16..235, C16..240 | u8 |
| P010LE | 64+876Y | 512+896C | Y64..940, C64..960 | little-endian u16, code<<6 |

Rounding is `floor(code+.5)`, positive half codes upward, with no dithering.
Every pixel is converted separately. Each row-major 2×2 block averages its four
*unquantized nonlinear* Cb/Cr values, then quantizes UV once. Y is quantized per
pixel. Output is tight Y followed by interleaved CbCr; zero/odd geometry rejects.

Audit: the sealed SDR writers consume cell YUV and average integer atlas coverage,
not RGB-derived chroma. They cannot directly serve this new RGB boundary and are
unchanged. C-4A follows the existing PQ RGB writer's convert/reduce/quantize order,
but uses target BT.709 coefficients rather than its source BT.2020 coefficients.

## Evidence and closure ledger

The machine-readable [ledger](../tests/baselines/tone-map/stage53c4a.json) binds
source hashes, commands, tool versions and persistent evidence. Final code
parity is **exact**, on the identical C-3 f32 signal promoted to f64 for the CPU
packing oracle. It is not a claim of exact end-to-end f64 tone-map pixels.
There is no ±1 tolerance, dithering, epsilon, half-bin snapping or CPU fallback.

### Failed experiments and corrections

The [first ordinary-f32 experiment](../tests/baselines/tone-map/c4a-boundary-initial-f32.json)
failed 928 NV12 and 3739 P010 codes. For example, a gray input with f32 bits
991272996 has f64 NV12 pre-code 16.49999992828816175: CPU 16, initial GPU 17.
These are packer errors, not C-3 errors, because both consume identical inputs.
The [intermediate compensated experiment](../tests/baselines/tone-map/c4a-boundary-intermediate.json)
still failed 9/30 codes on lavapipe at analytical chroma ties. Neither is PASS.

The final shader uses two f32 limbs, precise error-free sums/products and split
constants, with no FMA or Float64 capability. Chroma uses the canceled difference
form above; rounding compares the retained expansion with the exact half-code
boundary. An independent literal test exposed a further defect in the new,
unsealed CPU oracle: RGB [.4765625,.5078125,.5078125] has exact Cr=-1/64 and
NV12 pre-code 124.5, so its expected result is 125. The test failed before the
same algebraic cancellation corrected CPU evaluation. No sealed C-1/C-2B math
or test contract changed. Final Intel boundary evidence: 2293 NV12 vectors /
13758 codes and 6280 P010 vectors / 37680 codes, zero differences.

An early teardown also exposed Vulkan device-memory leakage and a crash:
the allocator's final reference outlived the context. Explicit allocator
release after buffer destruction, while the device still lives, fixed it.
The subsequent surface, fault, alternating, stress and teardown runs are clean.
Unknown GPU completion abandons the device and quarantines imported resources
and the owning VAAPI surface instead of permitting reuse; this branch is
code-reviewed, not a claim that a real device hang was induced.

### Reproduction

```bash
bash scripts/qualify-sdr-output-vulkan.sh /tmp/asciiflow-c4a-new-run
```

This must be a new output directory. It regenerates C-1/C-2B input using their
existing pinned generators, runs CPU vectors and then the actual Vulkan/surface
tests with Validation enabled; isolated performance follows with Validation off.
It requires the identity-checked fixtures from
`tests/fixtures/codecs/c3-pq-legal-v1-identity.json`, the installed FreeType font,
and real accessible `/dev/dri`. Runtime `boundary.json`, `canonical.json`,
`stress.json`, `performance.json` map to checked-in `c4a-boundary-final.json`,
`c4a-canonical.json`, `c4a-stress.json`, `c4a-performance.json` respectively.
Retained production regressions and workspace/SPIR-V validation are separate
obligations, with the exact commands recorded in the ledger. The successful
closure ran these component commands individually, not this wrapper as a whole.

### 56-item final closure report

| # | Requirement | Observed result / boundary |
| --- | --- | --- |
| 1 | Architecture | Resident C-3 RGB buffer → GPU pack buffer → imported FFmpeg-owned VAAPI surface; no CPU upload of packed pixels. |
| 2 | C-3 input | Unchanged finite [0,1] nonlinear BT.709 RGB, sealed P2/N3. Invalid values hard-fail before surface writes. |
| 3 | Matrix | BT.709 NCL equations above; not source BT.2020. |
| 4 | NV12 constants | Y=16+219Y′, C=128+224C′; legal 16..235 / 16..240. |
| 5 | P010 constants | Y=64+876Y′, C=512+896C′; legal 64..940 / 64..960; code<<6 LE. |
| 6 | Rounding | Nearest code, exact halves upward; no truncation/dither. |
| 7 | Chroma | Row-major 2×2 box average of unquantized nonlinear CbCr, then quantize. |
| 8 | Black | CPU literal vectors and actual dual-slot surface alternation pass; Y16/64, chroma128/512. |
| 9 | White | Literal endpoint vectors pass; Y235/940, neutral chroma. |
| 10 | Neutral | 4097-value CPU ramp and GPU vectors pass; exact neutral chroma. |
| 11 | Primary/secondary | Independent literal red/green/blue/cyan/magenta/yellow vectors pass both formats. |
| 12 | 2×2 / 4×4 | Mixed-color, checker, neutral-plus-red reduction and UV indexing vectors pass. |
| 13 | Boundaries | Below/at/above legal Y halves, chroma axes and varied neutral-base ties: 8573 GPU vectors total. |
| 14 | CPU NV12 | Independent permanent f64 oracle and rejection tests pass. |
| 15 | CPU P010 | Same oracle, explicit storage semantics and padding checks pass. |
| 16 | NV12 shader | Qualification-only `sdr_pack_nv12.comp`, precise two-limb f32. |
| 17 | P010 shader | Qualification-only `sdr_pack_p010.comp`, same independent oracle. |
| 18 | NV12 parity | Zero boundary and 1920×1080 canonical packed-byte differences. |
| 19 | P010 parity | Zero boundary and 1920×1080 canonical packed-byte differences. |
| 20 | Bin mismatches | Historical failed experiments preserved above; final mismatch count zero, exact gate unchanged. |
| 21 | Mono | Real HEVC/AV1, builtin/FreeType mono surfaces have Cb=Cr=128/512 everywhere. |
| 22 | Low bits | All P010 words have low six bits zero; active sample bounds pass. |
| 23 | HEVC→NV12 | Real legal PQ VAAPI decode, C-3, pack and actual surface readback pass. |
| 24 | HEVC→P010 | Same real-input test passes. |
| 25 | AV1→NV12 | Same real-input test passes. |
| 26 | AV1→P010 | Same real-input test passes. |
| 27 | Builtin | All codec/format/color combinations pass. |
| 28 | FreeType | All codec/format/color combinations pass; no synthetic atlas substitute. |
| 29 | Color | 16-case real-input matrix includes color; exact independent pack oracle. |
| 30 | Mono | Same matrix includes mono; exact neutral chroma. |
| 31 | Single slot | Reused slot, full-frame canonical and real input/surface tests pass. |
| 32 | Dual slot | Two C-3 slots staged and submitted before either completion; pack/copy synchronous on shared queue. |
| 33 | Cross-slot | 256 alternating black/saturated rounds; opposite slot assignment; RGB/diagnostics and both surface formats exact. |
| 34 | NV12 owner | FFmpeg/VAAPI-allocated encoder-style writable surface; no encoder opened (§24 scope). |
| 35 | P010 owner | Same owner model and actual downloaded contents pass. |
| 36 | DRM descriptor | Runtime format/modifier/object/offset/pitch consumed, not hardcoded; values below. |
| 37 | NV12 stress | HEVC legal input, 10×300=3000 actual surface readbacks, 300 f64-oracle frames, repeated hashes exact. |
| 38 | P010 stress | AV1 legal input, same 3000/300 evidence, padding zero. |
| 39 | FD lifecycle | Both stress runs: 4 before → sampled peak22 → 4 after; cycles stable at18. |
| 40 | GPU lifecycle | Explicit per-slot buffer bytes constant, RSS plateaus, teardown clean; unknown completion quarantined. |
| 41 | Validation | Zero errors for boundary, canonical, real surfaces, alternating slots, faults and stress through teardown. |
| 42 | Faults | Pipeline/buffer/descriptor/dispatch/after-fence/readback/import/copy/external-after-fence injection, healthy retry and FD cleanup pass both formats. No fallback. |
| 43 | NV12 speed | GPU pack mean1.5651ms, median1.3874ms; GPU external copy mean0.0966ms. |
| 44 | P010 speed | GPU pack mean1.5572ms, median1.3744ms; GPU external copy mean0.1791ms. |
| 45 | Whole path | C-3 only7.7321fps; C-3+NV126.5490fps; C-3+P0106.3907fps. Qualification readback included, decode/encode excluded. |
| 46 | Memory | C-3 75,596,984 bytes/slot; pack+diagnostics NV123,110,412 / P0106,220,812 bytes/slot. Both cached formats in two slots add18,662,448 bytes. |
| 47 | SDR regressions | Fresh 3 runs each H.2648/HEVC8/AV18/HEVC10/AV110: all15 file hashes equal retained post-polarity gates. |
| 48 | PQ preserve | Fresh 3 runs each HEVC10/AV110: all6 hashes and full packet/decoded/structure/metadata verifier tiers pass. |
| 49 | C-1/C-2B/C-3 | C-1/C-2B three retained hashes each unchanged; fresh C-3 canonical P2/N3 and real render tests pass. Old UNORM16 failure remains failure. |
| 50 | Historical H264 | Preserved pair re-compared: raw packet Tier1A FAIL, coded1B/decoded1C/structure2 PASS, whole-file DIFFERENT; version-SEI limitation unchanged. Not a fresh encode or raw PASS. |
| 51 | SPIR-V | All740 cached modules validate for Vulkan1.3; ordinary current OUT_DIR24 modules. Both new shaders Shader capability / f32 only / no FMA. Selected C-3 module hash unchanged. |
| 52 | Static | Release workspace build; default and measurement-feature workspace tests; both all-target clippy -Dwarnings; fmt, shell syntax and diff checks pass. Ignored hardware tests run separately as above. |
| 53 | Scope | Intel Arc MTL8086:7d55; ANV Mesa26.2.3-1.fc44, iHD26.1.5-1.fc44; FFmpeg8.1.3-1.fc44/libavcodec62.28.103; Rust1.97.1; SPIRV-Tools2026.1. |
| 54 | Limitations | Hardware-scoped finite-vector qualification, not universal f64 emulation; sampled resources, not driver-allocation tracing; no final encode/metadata proof or real induced device hang. |
| 55 | C-4B | HDR→SDR production planner/encode integration is justified; not started. |
| 56 | Final | **SEALED**. No remaining C-4A gate. Production HDR→SDR is still rejected. |

### Surface layout, timing and resource interpretation

Observed modifier was 72057594037927945 (0x100000000000009). NV12 used one
3,194,880-byte object, Y offset0 / UV offset2,088,960, both pitch1920;
P010 used one 6,389,760-byte object, Y offset0 / UV offset4,177,920, pitch3840.
The imports consume runtime plane kinds, dimensions, bounds and pitches. These
numbers identify the observed driver allocation, not an ABI requirement.

Performance used five warmups then 300 samples per variant, with no competing
GPU qualification job and Validation disabled only for timing. Per-pass raw GPU
timestamps and wall durations are retained in `c4a-performance.json`. Total wall
means: C-3 only129.3310ms, NV12152.6940ms, P010156.4782ms. C-3 readback dominates
(roughly119–127ms); pack wall means14.8777/17.9413ms include synchronization and
readback and must not be presented as GPU kernel times. Actual surface download
is checked before timing but excluded from the loop; input decode is excluded.
This is correctness-first qualification throughput, not production throughput.

RSS after cycles plateaus at195,472KiB (NV12) /196,884KiB (P010). Explicit bytes
exclude driver-owned VAAPI allocations, allocator granularity, descriptors,
pipelines and host oracle/readback vectors. FD peaks are sampled, not exhaustive
instrumentation. Healthy failures unwind imports and permit retry; unknown
completion intentionally retains unsafe-to-release allocations and the surface.

Production CLI PQ→default-SDR rejection was rerun against the real fixture:
the destination sentinel survives and no staging output is created. All prior
metadata signaling and production codec legality stay unchanged. Fresh SDR
outputs are byte-identical to the retained baseline; their earlier full decode,
PTS and metadata evidence is reused on that basis, not claimed as a new probe.

The existing canonical architecture file is `docs/architecture.md`; the former
v2-prefixed filename is not recreated after the Rust-only consolidation.
