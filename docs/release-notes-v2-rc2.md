# AsciiFlow 2.0.0-rc.2 — LGPL prebuilt candidate

This candidate uses the explicit `lgpl-prebuilt` Cargo feature and isolated
FFmpeg 8.1.3 shared libraries. Qualification status and exact artifact identities
are recorded separately; this document alone is not a release approval.

The official LGPL prebuilt release does not support software H.264 encoding.
Software decode and qualified VAAPI H.264/HEVC/AV1 encoding remain eligible.
The libx264 backend, base support states and historical software qualifications
remain in the source tree. Ordinary developer builds do not enable this profile;
their capability depends on the actual linked FFmpeg. No GPL Full flavor is
provided. Requests for software encoding reject early; no generic encoder
fallback substitutes another encoder.

The unchanged production boundary is Linux, recorded Intel Arc Meteor Lake
stacks, one process/one job. Supported HDR semantics are canonical limited
BT.2020 NCL/PQ preserve, or explicit PQ→BT.709 SDR in the qualified decoded
<=1000 cd/m² domain. HLG/full range remain unsupported. Audio is compressed
packet copy or none, not transcoding, under existing CFR/container conditions.
Persistent multi-job memory remains unqualified; no constant-memory, real-time
or 24/7 guarantee is made.

Base contract plus the `lgpl-prebuilt` constraints generate the support matrix.
All 17 historical retained hardware paths remain applicable (51 conversions).
`portable-h264-sdr` is a separate base case excluded for this release profile;
its historical PASS is not reclassified as FAIL.

Build the isolated dependency using
`third_party/ffmpeg/scripts/build-lgpl-prebuilt.sh`, then build with its sysroot,
locked dependencies and `--features asciiflow-cli/lgpl-prebuilt`.
The release archive must include the exact shared libraries, license/notices,
source archive and build recipe described in SOURCE.md. Replaceable dynamic
libraries must remain separate from the executable. No system libraries are
overwritten.

2.0.0-rc.1 remains InternalQualificationOnly, Superseded, NotForRedistribution.
Its package SHA is
`d2f62b91234abc2da09cba53bfb90cc759b3cb720e6f47fbc77bc9d12d22199b`;
binary SHA is
`07ae232d997593d2fee5292c3659caf1cb02b7f25bce118a1f1e453a429e8d70`.
Archived source/qualification results are not overwritten.
