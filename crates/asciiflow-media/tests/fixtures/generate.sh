#!/bin/sh
set -eu

cd "$(dirname "$0")"

ffmpeg -hide_banner -loglevel error -f lavfi -i testsrc2=size=32x32:rate=25 \
  -frames:v 1 -c:v libx264 -pix_fmt yuv420p -movflags +faststart -y one-frame.mp4
ffmpeg -hide_banner -loglevel error -f lavfi -i testsrc2=size=32x32:rate=25 \
  -frames:v 3 -c:v libx264 -pix_fmt yuv420p -movflags +faststart -y three-frame.mp4
ffmpeg -hide_banner -loglevel error -f lavfi -i testsrc2=size=32x32:rate=25 \
  -frames:v 1 -c:v libx264 -pix_fmt yuv420p10le -movflags +faststart -y ten-bit.mp4
ffmpeg -hide_banner -loglevel error -f lavfi -i testsrc2=size=32x32:rate=25 \
  -vf 'setpts=PTS+1/TB' -frames:v 5 -c:v libx264 -pix_fmt yuv420p \
  -use_editlist 1 -movflags +faststart -y edit-list.mp4
ffmpeg -hide_banner -loglevel error -f lavfi -i sine=frequency=1000:sample_rate=8000 \
  -t 0.1 -c:a aac -movflags +faststart -y no-video.mp4

cp three-frame.mp4 truncated-tail.mp4
truncate -s 1700 truncated-tail.mp4
cp three-frame.mp4 truncated-probe.mp4
truncate -s 64 truncated-probe.mp4
cp three-frame.mp4 corrupt-packet.mp4
dd if=/dev/zero of=corrupt-packet.mp4 bs=1 seek=1500 count=180 conv=notrunc status=none
