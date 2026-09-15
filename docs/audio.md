# Stage 4.2 audio passthrough

Stage 4.2 adds media completeness without adding an audio processing pipeline.
AsciiFlow copies compatible compressed audio packets from the input demuxer to
the MP4 muxer. It does not decode, encode, resample, remix, normalize, filter,
or otherwise inspect audio samples.

## User policy

`--audio auto|copy|none` is independent of the video policy and defaults to
`auto`.

- `auto` selects every input audio stream that FFmpeg reports as compatible
  with the MP4 muxer. Incompatible streams are skipped with a warning.
- `copy` selects every audio stream and fails during planning if any stream is
  not reported compatible. It never silently produces a partial selection.
- `none` selects no audio streams.

Input audio stream order is retained. For each selected stream the output
copies `AVCodecParameters`, including profile, sample rate, channel layout,
bit rate, extradata, and other codec configuration. The input container's
`codec_tag` is cleared so the MP4 muxer chooses the correct tag. Language
metadata and stream disposition, including the default flag, are preserved.

Compatibility is not a hard-coded AAC/MP3/ALAC allow-list. The media layer
asks the actual FFmpeg MP4 output format through `avformat_query_codec`. Header
creation and packet writing remain authoritative and return structured mux
errors when the detailed stream configuration is not acceptable.

## Ownership and bounded flow

```text
input AVFormatContext (decoder/demux owner)
  ├─ selected video packet -> decoder -> ASCII -> H.264 encoder ─┐
  └─ selected audio packet -> move-ref, no payload copy ─────────┤
                                                                v
                                              bounded mux queue (64 packets)
                                                                |
                                             one mux worker / one AVFormatContext
                                                                |
                                             av_interleaved_write_frame -> MP4
```

The decoder remains the only input demux owner. A selected audio packet is
moved into its own RAII `AVPacket` and sent through a bounded channel. The
encoder moves each produced H.264 packet into the same channel. The mux worker
is the only code that touches the output format context, so audio and video can
never concurrently call FFmpeg's mux API.

The queue has a fixed 64-packet bound, each packet is limited to 16 MiB, and both producers use timed sends
that observe cooperative cancellation. This prevents a non-interleaved input
or an audio-dense region from accumulating the whole compressed track in
memory. The worker also flushes FFmpeg's interleaving queue after 64 packets or
8 MiB, whichever comes first, so sparse or non-interleaved streams cannot
accumulate an entire track inside libavformat. MP4's sample tables still grow
with stream length; the packet-buffer bound does not imply constant container
index memory. Packet payload bytes remain reference-counted by FFmpeg and are not
copied by AsciiFlow.

All output streams are created before `avformat_write_header`. Each packet has
an explicit input-stream to output-stream mapping. Audio packet PTS, DTS, and
duration are rescaled from the input stream time base to the output stream time
base with `av_packet_rescale_ts`; AsciiFlow does not clamp, stretch, trim, or
invent timestamps. A selected audio packet missing PTS or DTS fails at mux
runtime with its input stream number.

The current video contract retains the Stage 4.1 ordered CFR sequence based on
the probed source frame-rate rational. With audio selected, the mux layer adds
the first decoded video timestamp to that sequence, preserving its origin
relative to audio. Subsequent decoded video timestamps must match that CFR
sequence within one input time-base tick. Missing, variable-rate, or
discontinuous video timestamps produce an explicit error rather than audio
drift; `--audio none` retains the existing video-only CFR behavior. Audio retains
the demuxed packet timeline, including negative timestamps and edit-list
effects. FFmpeg handles their MP4 representation.

`--max-frames` limits video processing only. Source finalization continues
demuxing the remaining selected audio without decoding additional video frames.
Audio may begin before video or end after it; output container duration can
therefore follow the longer stream. Source completion queues an audio-done
marker before the decoded-frame channel closes. The mux worker rejects trailer
requests without that marker, and consumes queued packets before finalization.

## Failure, cancellation, and output safety

Mux header, packet-write, and trailer failures are terminal. They do not cause
audio-only, video-only, decoder, encoder, or backend fallback after processing
has started. The first mux failure is shared with producers so a subsequent
channel disconnect does not replace the native FFmpeg context.

On Ctrl+C the common cancellation token stops producers and the mux worker.
Queued packet wrappers are dropped, no trailer is treated as successful, and
the Stage 4.1 transactional output guard deletes the staging MP4. An existing
destination is replaced only after successful video drain, mux trailer, and
worker completion.

## Diagnostics and metrics

`--capabilities` lists each audio stream's codec/profile, sample rate, channel
count, language, default disposition, and MP4-copy result. `--explain-plan`
prints the selected and skipped audio streams separately from the video plan.

The final metrics include copied audio packet count, compressed byte count,
and CPU wall time spent submitting audio packets to the muxer. These are audio
passthrough metrics, not per-frame render metrics and not part of the reported
video FPS.

Stage 4.2 deliberately adds no automated audio tests. Validation uses the
existing workspace suite plus a manual AAC-in-MP4 conversion and independent
`ffprobe` inspection. Audio transcoding, resampling, DSP, audio/video trimming,
new containers, and new codecs remain out of scope.

## Manual validation, 2026-09-15

The existing workspace tests passed (58 passed, 23 existing opt-in tests
ignored). Release workspace build, all-target Clippy with warnings denied,
format checking, and whitespace checking passed. No audio tests or media
fixtures were added to the repository.

The manual AAC LC/MP4 smoke succeeded on both the software path and Intel Arc
MTL's full VAAPI/Vulkan auto path. The final hardware output contains 90 H.264
video packets and 142 AAC packets, 48 kHz mono audio, 3-second stream durations,
Japanese language metadata (`jpn`), and the original default disposition. The
audio packet payload total was 26,317 bytes. Khronos validation was enabled;
the run completed without validation error output.

SIGINT during the full hardware path returned 130; the existing destination's
SHA-256 remained unchanged and staging was removed. A subsequent hardware
initialization and conversion succeeded. This was a manual cancellation smoke,
not an expanded stress suite.

A final 1080p, width-80, 300-frame auto-path comparison with validation disabled
used the same short remuxed input for both policies:

| Audio policy | Run 1 FPS | Run 2 FPS | Run 3 FPS | Median FPS |
| --- | ---: | ---: | ---: | ---: |
| none | 502.84 | 514.59 | 518.98 | 514.59 |
| copy | 512.43 | 520.84 | 522.58 | 520.84 |

The difference is small and does not establish a speed improvement. There was
no observed audio-related throughput regression. This is a short A/B check,
not a controlled rerun of the Stage 4.1 benchmark. The copied track contains
259 AAC packets, 96,583 payload bytes, 44.1 kHz stereo, language `eng`, and
6.013968 seconds of audio alongside 6 seconds of video. Audio mux submission
wall time was 0.253–0.321 ms for the entire run.

## Automated regression coverage (Stage 4.2.1)

Hardware-independent tests now lock auto/copy/none policy, strict incompatibility,
multi-track ordering, language/default disposition, exact compressed packet
payloads, timestamps within one output tick, relative A/V offset, unequal EOF,
short video, complete audio under a video frame limit, ignored subtitle streams,
and no-video/discontinuous-timeline rejection. Native tests cover missing and
negative timestamps, bounded backpressure/cancellation, and audio mux root-cause
preservation. Linux process tests cover SIGINT status 130 and safe existing-output
preservation after cancellation or buffered output failure. Output AAC and video
are decoded by test oracles; production remains compressed audio passthrough.
See `testing.md` for fixtures, normal tests, stress, and hardware limitations.

The remuxed input's extra 301st video frame had a discontinuous timestamp;
the final implementation rejected it explicitly. The comparison above uses
the first 300 consecutive video frames and still drains all selected audio.
