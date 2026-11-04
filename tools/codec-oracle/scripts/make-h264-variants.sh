#!/bin/sh
# Build the variants bisect/ links: copies of crates/h264 (the current tree) in which one of the six
# files changed by the hardening commit e0f337e is reverted to its pre-fix content (commit 935b352),
# plus an unmodified copy ("none"). Usage: scripts/make-h264-variants.sh   (from tools/codec-oracle)
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
root=$(cd "$here/../.." && pwd)
out="$here/bisect/variants"
rm -rf "$out"
for f in decoder dpb picture slice slicedec transform none; do
  d="$out/h264-$f"
  mkdir -p "$d"
  cp -R "$root/crates/h264/src" "$d/src"
  cp -R "$root/crates/h264/test-support" "$d/test-support"
  if [ "$f" != none ]; then
    git -C "$root" show "935b352:crates/h264/src/$f.rs" > "$d/src/$f.rs"
  fi
  cat > "$d/Cargo.toml" <<EOT
[package]
name = "v-$f"
version = "0.1.0"
edition = "2024"
[features]
default = ["threads"]
threads = ["dep:rayon"]
[dependencies]
craft-bitstream = { path = "$root/crates/bitstream" }
thiserror = "2"
rayon = { version = "1", optional = true }
EOT
done
echo "variants written to $out"
