#!/usr/bin/env bash
# Fetch the fonts the web build embeds that are not in craft-fonts, into a craft-fonts-shaped
# directory (fonts/manifest.txt), for CRAFT_FONTS_DIR:
#   CRAFT_FONTS_DIR="$(apps/cadcraft-web/web-fonts.sh DIR)" trunk build --release
# Liberation Sans (SIL Open Font License 1.1, https://github.com/liberationfonts/liberation-fonts)
# has Arial's widths, so drawings that use Arial keep their text lengths on the web, where no
# Arial is installed. Nothing is committed to this repository; the archive is checked by sha256.
set -euo pipefail

dir="${1:?usage: web-fonts.sh DIR}"
version=2.1.5
url="https://github.com/liberationfonts/liberation-fonts/files/7261482/liberation-fonts-ttf-$version.tar.gz"
sha=7191c669bf38899f73a2094ed00f7b800553364f90e2637010a69c0e268f25d0

mkdir -p "$dir/fonts/liberation-sans"
dir="$(cd "$dir" && pwd)"
cd "$dir" # relative paths below: GNU tar reads "C:/…" as a remote host
archive="liberation-fonts-ttf-$version.tar.gz"
if [ ! -f "$archive" ] || ! echo "$sha  $archive" | sha256sum -c --status; then
  curl -fsSL -o "$archive" "$url"
  echo "$sha  $archive" | sha256sum -c --status || { echo "web-fonts.sh: checksum mismatch for $url" >&2; exit 1; }
fi
tar -xzf "$archive" -C fonts/liberation-sans --strip-components=1 \
  "liberation-fonts-ttf-$version/LiberationSans-Regular.ttf" \
  "liberation-fonts-ttf-$version/LiberationSans-Bold.ttf" \
  "liberation-fonts-ttf-$version/LiberationSans-Italic.ttf" \
  "liberation-fonts-ttf-$version/LiberationSans-BoldItalic.ttf" \
  "liberation-fonts-ttf-$version/LICENSE"

{
  echo "# family | style | file | scripts | licence | licence file | sha256 | source"
  for style in Regular Bold Italic BoldItalic; do
    f="fonts/liberation-sans/LiberationSans-$style.ttf"
    s=$(sha256sum "$f" | cut -d' ' -f1)
    echo "Liberation Sans | $style | $f | Latn,Cyrl,Grek | OFL-1.1 | fonts/liberation-sans/LICENSE | $s | $url"
  done
} > fonts/manifest.txt

echo "$dir"
