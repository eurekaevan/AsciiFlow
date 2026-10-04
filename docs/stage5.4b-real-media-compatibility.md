# Stage 5.4B — Real-Media Compatibility Corpus

Status: **SEALED**. Final evidence reviewed on 2026-10-04; no Stage 5.4C work started.

The [durable closure receipt](../tests/corpus/stage54b-closure.json) records the
fixed binary, production source inventory, tools/device, exact commands, source
and output identities, timing records, strict-oracle decisions and validation
census. Full logs/media are retained under `target/stage54b-evidence`; these
ignored artifacts are reconstructible from the checked-in generators and
manifest. Earlier scratch evidence in `/tmp` was lost after restart and is
**not** the authority for this closure: final corpus, retained paths, static and
hardware gates were rerun and recorded.

## Decision and scope

[Contract 1.1.0](../tests/support/production-support-v1.json) extends the previous
1.0.0 contract (schema remains 1), solely from reviewed real-media evidence.
The original contract SHA-256 was
`b68396b38cf93873ea65f6f16de31e231c583f2ba4a6974dc1d3a2f39559bd1f`.
[The corpus](../tests/corpus/real-media-v1.json) contains 82 distinct requests,
not 82 independent sources: 47 **ConditionallySupported**, 24 **Unsupported**,
11 **Unqualified**. No automatic promotion, no unexplained success/rejection,
no FAILED or SKIPPED final gates. Surface counts in runner summaries overlap;
they are not unique-test counts. Unqualified is never PASS.

Four bounded production corrections were necessary:

- Admit actual native frame dimensions/pixel format and display geometry before
  cached scaler/interop reuse. Keep stream codec parameters independent from
  bitstream colors overwritten during codec opening.
- Reject demuxer-marked corrupt compressed audio before passthrough enqueue.
- Preserve passthrough audio display titles across the title/name container spelling.
- Treat static HDR side data with known SDR transfer as conflicting, not new HDR
  support.

No codec, HDR class, pixel format, shader algorithm, tone mapper, audio
transcoding or subtitle transcoding was added. Hardware qualified routes remain
VAAPI decode → NV12/P010 interop → Vulkan GPU mapping/processing → output interop
→ matching VAAPI encode. Portable cases explicitly select CPU/software decode
and H.264 software encode. Width/font/charset/color/audio/output settings are
fixed per manifest request and receipt argv, not hidden defaults.

## Reproduction and admission

Exact checked-in recipes (each creates a fresh directory):

```bash
python3 tests/corpus/generate-real-video.py OUTPUT_DIRECTORY
python3 tests/corpus/generate-real-audio-metadata.py OUTPUT_DIRECTORY
python3 tests/corpus/generate-real-negative.py OUTPUT_DIRECTORY

cargo build --release --workspace
python3 -B tests/corpus/run.py quick --manifest tests/corpus/real-media-v1.json \
  --output target/stage54b-evidence-new
# Full existing CPU references, hardware smokes and 17 retained paths:
python3 -B tests/corpus/run.py full --retained \
  --manifest tests/corpus/real-media-v1.json --output target/stage54b-full-new
```

Generator source identities and full FFmpeg command/configuration are retained,
not an assertion that a similarly named file is the same input. Every group was
generated twice, all 38/29/12 source hashes and sizes matched. Actual full probe
facts must equal the manifest before any production command. Only the probe
filename is normalized; invalid input retains null media facts rather than
inventing a profile, bit depth or timestamp.

The qualified portable tuple explicitly uses width16, builtin-8x8, standard
charset, true color, software decode/encode, CPU backend, interop off, max-frames0.
Hardware corpus tuples use width80, the same font/charset/color, VAAPI
decode/encode, Vulkan GPU mapping, both interops on and renderD128. Output
codec/depth/dynamic-range/audio are explicit per request. Retained matrix
commands keep their existing width80/max-frames300 and canonical input recipes.
Every exact executed CLI is in the receipt; no new golden was generated.

## Timing, color and output contract

All comparisons use rational seconds and actual native time bases. Video-only
CFR retimes to zero; copied audio requires the source video CFR grid and restores
its origin. No duplicate/drop/future VFR policy is introduced. VFR-only retiming
is observed but unqualified, not silently called source-presentation fidelity.

Qualified outputs are fully decoded, with exact frame counts and signal
geometry/depth/profile/color verified on stream and every frame. SDR output is
BT.709 limited; PQ-preserve is BT.2020/PQ/NCL limited. Neither clones/recomputes
source mastering/CLL/dynamic HDR side data. Subtitles and video titles are
explicitly omitted. Language/default and compressed audio payload/timestamps
must pass the old strict oracle for a supported audio tuple. Display titles are
additionally compared semantically across container title/name spellings; a
registered Unqualified limit cannot mask their loss.

## Final report inventory

| # | Required item | Observed result / policy |
|---|---|---|
| 1 | Corpus size/categories | 82 request tuples / 79 unique generated media: 38 video, 29 audio/metadata, 12 negative/corrupt, plus 3 hardware clones. Categories: basic, timing, GOP, container, color, audio, metadata, short, stress, mutation and negative/corruption. |
| 2 | Generated versus retained | All 79 gating sources generated locally; zero external/downloaded samples. Separate retained canonical inputs drive the unchanged 17-path production matrix. |
| 3 | Generator/tool identities | Three checked-in generate-real-*.py recipes; every source SHA/size/generator SHA/probe SHA is pinned in real-media-v1.json. Each group generated twice: all 79 bytes identical. FFmpeg/ffprobe 8.1.3, Fedora 8.1.3-1.fc44; full configuration, library identities and argv in the receipt. |
| 4 | CFR rates | 24000/1001,24,25,30000/1001,30,50,60000/1001,60 pass the exact portable tuples; no hardware Cartesian-product promotion. |
| 5 | Timestamp contract | Decoder best-effort PTS retained through processing; encoder uses sequential index at initialized frame rate. Video-only output is zero-origin CFR, one output per decoded frame. Audio-copy requires source CFR grid (one input-tick tolerance), restores video origin and copies audio PTS/DTS. |
| 6 | B-frame cases | H.264 B0/B2/B4 and HEVC B2 pass; H.264 B2 hardware clone also passes. Actual decoded presentation and packet reorder/negative DTS retained; output packet DTS monotonic and complete. |
| 7 | Long GOP | 250-frame key cadence case fully drains and passes; no generalized open-GOP claim. |
| 8 | All-intra | Full conversion/decode-back passes, no dropped tail. |
| 9 | VFR result/policy | Unqualified video-only retiming: 4 source frames spanning 7/25s become 4 CFR frames spanning 4/25s at 25fps; 3/25s source-duration loss is explicit, not timing fidelity. VFR plus copied AAC rejects DecodeRuntime/Media. |
| 10 | Nonzero PTS | +2s and +86400s video-only inputs retime to zero as documented; source-origin loss is intentional policy, not accidental preservation. |
| 11 | Audio/video offset | +2s video-only, audio-only and both-track offset cases pass the source-origin audio-copy policy, preserving negative AAC priming and signed track offsets. |
| 12 | Unusual time bases | 1/1000,1/90000,1/48000,1/1000000 cases pass. JSON times are exact rational seconds, not rounded floats. |
| 13 | Duration accuracy | All qualified CFR presentation spans and frame grids satisfy explicit source/output tick bounds; no arbitrary 100ms tolerance. Missing final boundary derivation is reported. VFR span mismatch remains unqualified. |
| 14 | A/V sync | Qualified MP4 AAC track starts/ends and signed audio-minus-video end deltas checked within the recorded four source/output-tick endpoint bound, alongside the unchanged stricter packet oracle. |
| 15 | Ordinary MP4 | Qualified only exact reviewed source/request tuples; actual demux/decode/encode/mux/decode-back completed. |
| 16 | Fragmented MP4 | Video-only and AAC fragmented cases pass; fast-start MP4 also passes. |
| 17 | Matroska | Video-only H.264 converts but broad Matroska qualification withheld. Four AAC cases and FLAC retain precise strict-oracle limitations. |
| 18 | Other containers | MOV H.264/AAC remains Unqualified because missing language becomes und; no non-MP4 output support added. |
| 19 | 1-frame | 1 decoded/output frame, complete EOF drain and decode-back pass. |
| 20 | 2-frame | 2 decoded/output frames, complete EOF drain and decode-back pass. |
| 21 | 3-frame | 3 decoded/output frames, complete EOF drain and decode-back pass. |
| 22 | Odd dimensions | Real AV1 129x97 yuv420p rejects InputProbe/Media via existing NV12 even-dimension admission. Separate odd H.264 yuv444p rejects unsupported layout. No padded-source support claim. |
| 23 | Resolution diversity | 2x2,4x4,128x96,720p,1080p portable exact tuples pass. 64x64 PQ/P010 encode probe rejects Planning/InvalidConfig and remains Unqualified; no general hardware resolution claim. |
| 24 | No audio | Explicit audio none outputs contain no audio stream; video-only timing policy verified. |
| 25 | Single AAC | 44.1kHz mono / 48kHz stereo MP4 pass strict copy/decode oracle. 48kHz stereo hardware clone also passes. |
| 26 | Dual AAC | Language/default/channel/sample-rate and ordered payload/timing pass for MP4, including full-GPU clone. |
| 27 | Audio shorter | Video and shorter audio retain their independent endpoints; no padding, trimming or transcoding introduced. |
| 28 | Audio longer | Longer audio fully drains; original compressed payload and timestamps retained, no truncation to video. |
| 29 | Audio payload integrity | Qualified cases use unchanged Rust strict oracle. Six Unqualified cases additionally prove exact every packet payload SHA/size/count/order and PTS/DTS plus full source/output audio decode; metadata/duration failures remain FAILED within their receipts. |
| 30 | Unsupported audio | Real Speex has native MP4 copy eligibility false and rejects Planning/Media. FLAC eligibility is true on this build, but container metadata is unqualified; not mislabeled unsupported codec. |
| 31 | Subtitle policy | mov_text input converts with subtitles deliberately ignored/omitted; no subtitle copy/transcode claim. |
| 32 | Missing primaries | Strict HEVC10 source rejects InputProbe/UnsupportedColor; raw unknown stays unknown. |
| 33 | Missing transfer | Strict HEVC10 source rejects InputProbe/UnsupportedColor. |
| 34 | Missing matrix | Strict HEVC10 source rejects InputProbe/UnsupportedColor. |
| 35 | Missing range | NOT CONSTRUCTED as native unspecified: fixed HEVC attempt still reports limited in stream/frame. Limited conversion passes but case remains Unqualified; no invented missing-range qualification. |
| 36 | Conflicting metadata | Independent BT.709 container descriptor versus PQ frame and retained BT.709/PQ conflict reject InputProbe/UnsupportedColor/Conflicting. |
| 37 | HDR static metadata | 1080p PQ with and without mastering/CLL passes qualified Intel preserve tuple. Output is PQ/BT.2020 limited, with source mastering/MaxCLL/MaxFALL/dynamic HDR not propagated or recomputed. Static fidelity is not claimed. |
| 38 | SDR/HDR conflict | Real SDR with HDR SEI rejects Conflicting; stream/frame content-light conflict unit controls pass. Static metadata does not reclassify SDR as supported HDR. |
| 39 | Rotation | Non-identity display matrix rejects UnsupportedFrame. No silent rotate/reflection/scale/translation stripping; malformed and unaligned matrix controls tested. |
| 40 | SAR | Non-square/invalid SAR rejects UnsupportedFrame. Absent/unspecified SAR retains existing square interpretation; no stretch/aspect correction. |
| 41 | Midstream resolution | Concatenated real elementary stream changing dimensions rejects UnsupportedFrame before cached scaler/interop can consume incompatible frame. |
| 42 | Midstream color | Same-dimension changed SPS color rejects UnsupportedColor/Conflicting; actual independent stream/frame check, not a post-open context comparing with itself. |
| 43 | Other midstream changes | 8-to-10-bit change rejects UnsupportedFrame; level-only same-geometry SPS update drains 8 frames but timestamp-less Annex B timeline remains Unqualified. |
| 44 | Truncated beginning | InputProbe/Media rejection; preexisting target preserved, no staging. |
| 45 | Truncated middle | DecodeRuntime/Media rejection; not accepted because FFmpeg alone can conceal some damage. |
| 46 | Truncated end | MuxRuntime/Media rejection for demuxer-corrupt AAC packet; packet rejected before mux enqueue. |
| 47 | Corrupted packet | DecodeRuntime/Media rejection without panic/abort; sentinel preserved. |
| 48 | Invalid header/extradata | InputProbe/Media rejection, no fake pixel/depth facts when probe has no decoded video. |
| 49 | Garbage input | InputProbe/Media rejection. Zero-frame construction produced audio-only/no video: that actual input rejects, not evidence for a valid zero-frame video stream. |
| 50 | Watchdog/hang | All final media commands completed within bounded deadlines, no unexpected TIMEOUT. Real timeout controls reject and kill the entire process group, including TERM-resistant child after leader exit. |
| 51 | Safe output | Every fatal conversion preserves an existing sentinel destination byte-for-byte and leaves no asciiflow-part file. Unexpected negative success is a test failure. |
| 52 | Failure-stage accuracy | Exact structured stage/category/classification checked against manifest; missing diagnostics, wrong failure code and setup mismatch fail. No English error-string classification. |
| 53 | Medium compatibility smoke | 120s / 3000-video-frame / dual AAC case fully converts/decode-backs with strict audio/timing and preserved display names; sampled conversion process VmHWM 110544KiB. Excludes children/GPU memory; not an FPS/performance benchmark. |
| 54 | Pairwise coverage delta | Existing scenario-pair inventory remains unchanged, not a percentage target. Added actual B2 full-GPU and single/dual AAC hardware tuples plus exact portable timing/container/geometry cases; uncovered cross-products remain unqualified. |
| 55 | Support promotions | 47 exact source/request tuples explicitly reviewed as ConditionallySupported only after full evidence. No blanket codec/profile/container/backend promotion. |
| 56 | Demotions/refinements | No existing supported path demoted. Contract refines VFR retiming, display geometry, mutations, missing strict metadata, corrupt passthrough, static-output policy and container audio limitations. |
| 57 | Remaining Unqualified | 11 precise blockers listed below: 6 strict audio limits, video-only Matroska, video-only VFR, timestamp-less benign SPS update, unconstructed native missing-range, and 64x64 PQ encoder probe. |
| 58 | Contract consistency | Version 1.0.0/schema1 input contract retained by identity; version1.1.0/schema1 source now renders README and production-support.md. Core planner/contract and generated-document checks pass; dimensions are not their Cartesian product. |
| 59 | 17 retained paths | 5 SDR +2 PQ preserve +10 PQ-to-SDR, each 3 runs =51 outputs. Full artifact identities identical; retained packet/coded-signal/decoded-pixel/metadata tiers and repeatability checks pass. |
| 60 | Historical H.264 oracle | Existing Rust coded-semantic controls and exact same-build pair pass unchanged. Old failed raw-packet/build claims and historical baseline provenance remain historical; no hash or allowlist rewritten. New matching bytes do not inherit old build attestation. |
| 61 | SPIR-V | All 995 actual debug/release cache modules validate under Vulkan1.3 after final qualification builds; exact paths/SHA and aggregate census in receipt. Prior 964 census also passed; newly compiled diagnostic modules included, no fixed count shortcut. |
| 62 | Static checks | Release workspace/tests and encode-characterization build; clippy all targets -D warnings with/without encode-characterization; fmt; generated-document consistency; diff; 52 offline corpus controls pass. Real C1/C2B/C3/C4A references and eight Validation/FD/fault/FreeType hardware gates pass. |
| 63 | Exact limitations | Scoped Intel Arc Meteor Lake 8086:7d55 / Mesa ANV26.2.3 / iHD26.1.5 / kernel7.2.8 / FFmpeg8.1.3 only. No hardware/software/profile Cartesian product, no faithful VFR, no MOV/Matroska audio support, no missing-range/valid-zero-frame fixture proof, no new features. A preexisting unused-variable warning in optional p010-output-diagnostic-only build is recorded; production and required characterization clippy gates are clean. |
| 64 | 5.4C justification | Stage 5.4C Toolchain / Driver Portability Baseline is justified. No Stage5.4C implementation started. |
| 65 | Final Stage5.4B status | SEALED: all 47 conditional tuples pass, all 24 Unsupported cases reject as expected, 11 Unqualified blockers are explicit, zero unexplained failures/skips, retained and static/hardware gates green. |

## Remaining Unqualified cases — not PASS

| Case | Exact blocker |
|---|---|
| `real-matroska-mkv` | Video-only H.264 sample passes, but Matroska is not broadly qualified: AAC/FLAC stream language/default/duration semantics fail the strict audio oracle in the adjacent cases. |
| `real-vfr-three-durations-mp4` | Video-only conversion retimes one output frame per decoded input to zero-origin CFR without duplication/drop. Original VFR presentation duration is not preserved; audio-copy VFR rejects. |
| `real-same-dimension-parameter-update-h264` | Benign SPS level update drains all eight frames, but elementary-stream source PTS/DTS are absent. Container/timeline preservation is not qualified. |
| `real-h264-aac-mkv` | Unchanged strict audio oracle fails: undefined_language, default_disposition, millisecond_duration. Exact packet payload/order/count, PTS/DTS and aggregate endpoints remain preserved; not a Supported/PASS audio qualification. |
| `real-hevc-aac-mkv` | Unchanged strict audio oracle fails: undefined_language, default_disposition, millisecond_duration, missing_first_duration. Exact packet payload/order/count, PTS/DTS and aggregate endpoints remain preserved; not a Supported/PASS audio qualification. |
| `real-av1-aac-mkv` | Unchanged strict audio oracle fails: undefined_language, default_disposition, millisecond_duration, missing_first_duration. Exact packet payload/order/count, PTS/DTS and aggregate endpoints remain preserved; not a Supported/PASS audio qualification. |
| `real-h264-aac-mov` | Unchanged strict audio oracle fails: undefined_language. Exact packet payload/order/count, PTS/DTS and aggregate endpoints remain preserved; not a Supported/PASS audio qualification. |
| `real-flac-copy-candidate-mkv` | Unchanged strict audio oracle fails: undefined_language, default_disposition. Exact packet payload/order/count, PTS/DTS and aggregate endpoints remain preserved; not a Supported/PASS audio qualification. |
| `real-pq-static-64x64-mp4` | The actual Intel iHD HEVC Main10 64x64 P010 encoder probe rejects at Planning/InvalidConfig; no blanket resolution qualification. |
| `real-missing-range-attempt-mp4` | The fixed generator could not construct native unspecified range: actual HEVC stream/frame range remains limited. Successful limited-range conversion is not missing-range evidence. |
| `real-negative-source-mkv` | Unchanged strict audio oracle fails: millisecond_duration. Exact packet payload/order/count, PTS/DTS and aggregate endpoints remain preserved; not a Supported/PASS audio qualification. |

Six registered audio cases keep their **original strict oracle failure**.
A fresh typed report must name the exact input/output and expected
MetadataMismatch or TimestampMismatch; compiler errors/missing or unexpected
reports are still FAILED. The characterization checks every payload/size/order,
exact PTS/DTS and narrowly enumerated differences, then completely decodes both
source and output audio. It does not weaken or replace the old oracle.

## Evidence retention and sealing rationale

The final binary SHA-256 is
`7cd7e997cae8975b0f030030e2c08333728cde0614d7c9fb1d3409e421dc17c4`.
Original qualified artifact/build provenance is immutable. Repeated retained
output bytes match; this does not call the new executable the historical build.
C3's selected N3 gate remains separate from historical failed two-code UNORM16
diagnostics. No historical failed gate was relabeled PASS.

After the final title-only native correction, all 82 corpus tuples and the
17×3 production matrix were rerun with this fixed binary. C1/C2B/C3/C4A
reference evidence is reused from the same closure round: all 46 relevant
core/CPU/Vulkan/shader source identities remain equal. The FFmpeg title change
does not alter their math or input vectors. Native lifecycle/Validation gates
were rerun separately rather than inferred from that reuse.

All 995 current SPIR-V modules passed Vulkan1.3 validation. Their sorted
repo-relative path + space + SHA lines (joined without terminal newline) produce
census SHA-256
`7904b61d4e9e380f271fa1f84d4d38b5536a1dde54e92076e4c43818f0ba7979`.

The explicit Unqualified boundaries are allowed by this stage's definition of
done and are retained as such; they are not failed Supported cases hidden by
demotion. Timing, audio, color, safe-output, hang prevention, repeatability and
the existing production matrix have no remaining unexplained gate.
**Stage 5.4B = SEALED.**

Stage 5.4C Toolchain / Driver Portability Baseline is justified.
