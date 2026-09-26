#!/bin/sh
set -eu

# Generation-only toolchain pin. Runtime tests use the linked libav* libraries.
tool=${ASCIIFLOW_FIXTURE_FFMPEG:-ffmpeg}
version=$($tool -version | head -n 1)
case "$version" in 'ffmpeg version 8.1.2 '*) ;; *)
  echo "Expected fixture generator FFmpeg 8.1.2, got: $version" >&2
  exit 1
  ;;
esac

cd "$(dirname "$0")"
video='testsrc2=size=64x64:rate=30:duration=1.2'
audio='sine=frequency=440:sample_rate=48000:duration=1.2'
run() { "$tool" -hide_banner -loglevel error -y -threads 1 "$@"; }
tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT HUP INT TERM

# 36 display frames (1.2 seconds at 30 fps), BT.709 limited-range 4:2:0.
# HEVC deliberately retains B pictures so decode order differs from display order.
run -f lavfi -i "$video" -f lavfi -i "$audio" -map 0:v -map 1:a \
  -c:v libx265 -preset ultrafast \
  -x265-params 'bframes=3:keyint=30:min-keyint=30:open-gop=0' -bf 3 \
  -pix_fmt yuv420p -c:a aac -b:a 64k -shortest \
  -bsf:v 'hevc_metadata=video_full_range_flag=0:colour_primaries=1:transfer_characteristics=1:matrix_coefficients=1' \
  hevc-main8-bframes.mp4

run -f lavfi -i "$video" -f lavfi -i 'sine=frequency=880:sample_rate=48000:duration=1.2' \
  -map 0:v -map 1:a -c:v libaom-av1 -cpu-used 8 -crf 40 -g 30 -bf 0 \
  -pix_fmt yuv420p -c:a aac -b:a 64k -shortest \
  -bsf:v 'av1_metadata=color_primaries=1:transfer_characteristics=1:matrix_coefficients=1:color_range=tv' \
  av1-main8-nofilmgrain.mp4

# Historical 10-bit fixtures retained under their original "reject" filenames.
run -f lavfi -i "$video" -f lavfi -i "$audio" -map 0:v -map 1:a \
  -c:v libx265 -preset ultrafast \
  -x265-params 'bframes=3:keyint=30:min-keyint=30:open-gop=0' -bf 3 \
  -pix_fmt yuv420p10le -c:a aac -b:a 64k -shortest \
  -bsf:v 'hevc_metadata=video_full_range_flag=0:colour_primaries=1:transfer_characteristics=1:matrix_coefficients=1' \
  hevc-main10-reject.mp4

run -f lavfi -i "$video" -f lavfi -i 'sine=frequency=880:sample_rate=48000:duration=1.2' \
  -map 0:v -map 1:a -c:v libaom-av1 -cpu-used 8 -crf 40 -g 30 -bf 0 \
  -pix_fmt yuv420p10le -c:a aac -b:a 64k -shortest \
  -bsf:v 'av1_metadata=color_primaries=1:transfer_characteristics=1:matrix_coefficients=1:color_range=tv' \
  av1-main10-reject.mp4

# Stage 5.2B qualification inputs. The source is generated as true 10-bit
# planar samples (not an 8-bit image shifted into a 10-bit pixel format).
python3 ./generate_10bit_gradient.py "$tmpdir/gradient.yuv"

run -f rawvideo -pixel_format yuv420p10le -video_size 64x64 \
  -framerate 30 -i "$tmpdir/gradient.yuv" -map 0:v:0 -frames:v 36 -an \
  -c:v libx265 -preset ultrafast -x265-params 'lossless=1:bframes=0:keyint=30:min-keyint=30:open-gop=0' \
  -pix_fmt yuv420p10le \
  -bsf:v 'hevc_metadata=video_full_range_flag=0:colour_primaries=1:transfer_characteristics=1:matrix_coefficients=1' \
  hevc-main10-sdr-gradient.mp4

run -f rawvideo -pixel_format yuv420p10le -video_size 64x64 \
  -framerate 30 -i "$tmpdir/gradient.yuv" -map 0:v:0 -frames:v 36 -an \
  -c:v libaom-av1 -cpu-used 8 -crf 0 -b:v 0 -g 30 -bf 0 \
  -pix_fmt yuv420p10le \
  -bsf:v 'av1_metadata=color_primaries=1:transfer_characteristics=1:matrix_coefficients=1:color_range=tv' \
  av1-main10-sdr-gradient.mp4

run -f rawvideo -pixel_format yuv420p10le -video_size 64x64 \
  -framerate 30 -i "$tmpdir/gradient.yuv" -map 0:v:0 -frames:v 36 -an \
  -c:v libx265 -preset ultrafast -x265-params 'lossless=1:bframes=0:keyint=30:min-keyint=30:open-gop=0' \
  -pix_fmt yuv420p10le \
  -bsf:v 'hevc_metadata=video_full_range_flag=0:colour_primaries=9:transfer_characteristics=16:matrix_coefficients=9' \
  hevc-main10-pq-reject.mp4

# Stage 5.3A color-semantics fixtures. Stream-copy retagging preserves the
# deterministic 10-bit gradient while updating both MP4 stream fields and the
# codec bitstream. The latter supplies the decoded AVFrame color fields.
retag_hevc() {
  output=$1 range=$2 primaries=$3 transfer=$4 matrix=$5 full=$6
  primaries_code=$7 transfer_code=$8 matrix_code=$9
  run -i hevc-main10-sdr-gradient.mp4 -map 0:v:0 -c:v copy \
    -color_range "$range" -color_primaries "$primaries" \
    -color_trc "$transfer" -colorspace "$matrix" \
    -bsf:v "hevc_metadata=video_full_range_flag=$full:colour_primaries=$primaries_code:transfer_characteristics=$transfer_code:matrix_coefficients=$matrix_code" \
    "$output"
}

retag_hevc hevc-main10-sdr-full.mp4 pc bt709 bt709 bt709 1 1 1 1
retag_hevc hevc-main10-bt2020-sdr.mp4 tv bt2020 bt709 bt2020nc 0 9 1 9
retag_hevc hevc-main10-hlg.mp4 tv bt2020 arib-std-b67 bt2020nc 0 9 18 9
retag_hevc hevc-main10-unspecified.mp4 tv unknown unknown unknown 0 2 2 2
retag_hevc hevc-main10-pq-bt709-conflict.mp4 tv bt709 smpte2084 bt709 0 1 16 1

# Legacy 8-bit BT.601/170M input is converted by libswscale to BT.709 NV12;
# hardware decode is not a legal candidate for this color-normalization path.
run -i hevc-main8-bframes.mp4 -map 0:v:0 -c:v copy -an \
  -color_range tv -color_primaries smpte170m -color_trc smpte170m \
  -colorspace smpte170m \
  -bsf:v 'hevc_metadata=video_full_range_flag=0:colour_primaries=6:transfer_characteristics=6:matrix_coefficients=6' \
  hevc-main8-bt601.mp4

run -i av1-main10-sdr-gradient.mp4 -map 0:v:0 -c:v copy \
  -color_range tv -color_primaries bt2020 -color_trc smpte2084 \
  -colorspace bt2020nc \
  -bsf:v 'av1_metadata=color_primaries=9:transfer_characteristics=16:matrix_coefficients=9:color_range=tv' \
  av1-main10-pq.mp4

# x265 writes HDR10 static SEI messages into the coded stream. The existing
# PQ fixture above has no mastering-display or content-light side data.
run -i hevc-main10-sdr-gradient.mp4 -map 0:v:0 -frames:v 36 -an \
  -c:v libx265 -preset ultrafast \
  -x265-params 'lossless=1:bframes=0:keyint=30:min-keyint=30:open-gop=0:master-display=G(13250,34500)B(7500,3000)R(34000,16000)WP(15635,16450)L(10000000,50):max-cll=1000,400:log-level=error' \
  -pix_fmt yuv420p10le -color_range tv -color_primaries bt2020 \
  -color_trc smpte2084 -colorspace bt2020nc \
  -bsf:v 'hevc_metadata=video_full_range_flag=0:colour_primaries=9:transfer_characteristics=16:matrix_coefficients=9' \
  "$tmpdir/pq-static.mp4"
run -i "$tmpdir/pq-static.mp4" -map 0:v:0 -c:v copy \
  -color_range tv -color_primaries bt2020 -color_trc smpte2084 \
  -colorspace bt2020nc \
  -bsf:v 'hevc_metadata=video_full_range_flag=0:colour_primaries=9:transfer_characteristics=16:matrix_coefficients=9' \
  hevc-main10-pq-static-metadata.mp4

sha256sum ./*.mp4 > SHA256SUMS
"$tool" -version > generator-version.txt
python3 --version >> generator-version.txt
