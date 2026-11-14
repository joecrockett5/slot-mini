#!/bin/sh
# Build the PC release zip, dist/slot-mini-pc-<version>-windows-x64.zip,
# around an exe built on Windows (CI's `windows` job):
#
#     SLOT_MINI_PC_EXE=path/to/slot-mini-pc.exe scripts/package-pc.sh
#
# The zip is the exe, a README, and the licences. It runs from wherever
# it is unzipped; there is no installer.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' crates/slot-mini-pc/Cargo.toml | head -1)
exe=${SLOT_MINI_PC_EXE:-$root/target/x86_64-pc-windows-msvc/release/slot-mini-pc.exe}
[ -f "$exe" ] || { echo "no $exe (it is built on Windows; see .github/workflows/ci.yml)" >&2; exit 1; }

# What a released exe must be: 64-bit, a windowed program (no console),
# carrying its icon and version resource.
python3 - "$exe" "$version" <<'PY'
import struct, sys
data = open(sys.argv[1], "rb").read()
version = sys.argv[2]
pe = struct.unpack_from("<I", data, 0x3c)[0]
assert data[:2] == b"MZ" and data[pe:pe + 4] == b"PE\0\0", "not a Windows exe"
machine, sections = struct.unpack_from("<HH", data, pe + 4)
assert machine == 0x8664, f"not x64 (machine {machine:#x})"
opt = pe + 24
assert struct.unpack_from("<H", data, opt)[0] == 0x20b, "not PE32+"
subsystem = struct.unpack_from("<H", data, opt + 68)[0]
assert subsystem == 2, f"not the Windows subsystem ({subsystem}): a console would open"
names = [data[opt + struct.unpack_from("<H", data, pe + 20)[0] + 40 * i:][:8].rstrip(b"\0")
         for i in range(sections)]
assert b".rsrc" in names, "no resources: the icon and version are missing"
wide = lambda s: s.encode("utf-16-le")
assert wide("ProductVersion") in data and wide(version) in data, f"no version resource for {version}"
assert b"longPathAware" in data, "no manifest"
print(f"{sys.argv[1]}: x64, Windows subsystem, version {version}, {len(data)} bytes")
PY

name=slot-mini-pc-$version-windows-x64
stage=$root/dist/stage-pc
dir=$stage/$name
rm -rf "$stage"
mkdir -p "$dir/licenses"

cp "$exe" "$dir/slot-mini-pc.exe"
cp LICENSE "$dir/LICENSE.txt"
cp licenses/slot-GPL-3.0.txt "$dir/licenses/slot-GPL-3.0.txt"
cp crates/slot-mini-ui/assets/label.ttf.OFL.txt "$dir/licenses/OpenSans-OFL-1.1.txt"
cp crates/slot-mini-ui/assets/SymbolsNerdFontMono-Regular.ttf.LICENSE.txt "$dir/licenses/NerdFontsSymbols-LICENSE.txt"
python3 scripts/third-party.py --pc "$dir/licenses/THIRD-PARTY.txt"
cp package/pc/CREDITS.txt "$dir/CREDITS.txt"
cp package/pc/README.txt "$dir/README.txt"
# Notepad's line ends, for the files a Windows user opens.
for f in "$dir"/*.txt "$dir"/licenses/*.txt; do
    sed -i 's/\r$//; s/$/\r/' "$f"
done

zip=$root/dist/$name.zip
rm -f "$zip"
(cd "$stage" && zip -q -r -X "$zip" "$name")
echo "$zip"
unzip -l "$zip"
