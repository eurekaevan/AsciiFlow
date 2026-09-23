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

# 10-bit counterparts are intentionally unsupported/rejection fixtures.
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

sha256sum ./*.mp4 > SHA256SUMS
"$tool" -version > generator-version.txt
python3 --version >> generator-version.txt
