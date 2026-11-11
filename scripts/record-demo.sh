#!/bin/sh
# Records the README's demo GIF (docs/media/demo.gif): the app is driven
# headlessly through a short tour by the ignored test in
# crates/slot-mini/tests/demo.rs and the frames are encoded with ffmpeg.
#
# Usage: scripts/record-demo.sh [out.gif]
set -eu

here=$(cd "$(dirname "$0")/.." && pwd)
out=${1:-$here/docs/media/demo.gif}
frames=$here/target/demo/frames.rgba

command -v ffmpeg >/dev/null || { echo "record-demo: needs ffmpeg" >&2; exit 1; }

DEMO_OUT=$frames cargo test --release -q -p slot-mini --test demo -- --ignored --exact record_demo

# The test keeps every other 16 ms frame: 31.25 fps. A two-pass palette
# keeps the wood and the shells clean at a size GitHub serves quickly.
filters="fps=25,scale=480:-1:flags=lanczos"
ffmpeg -loglevel error -y -f rawvideo -pix_fmt rgba -s 640x480 -r 31.25 -i "$frames" \
    -vf "$filters,split[a][b];[a]palettegen=max_colors=192:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle" \
    -loop 0 "$out"
echo "wrote $out ($(du -h "$out" | cut -f1))"
