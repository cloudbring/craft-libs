#!/usr/bin/env python3
"""Generate expected/<codec>.tsv: the reference output digests for the H.264 and HEVC suites.

Run:  python3 -I crates/conformance/scripts/gen_expected.py h264 ~/ArtCraft/corpus/conformance-h264 > crates/conformance/expected/h264.tsv

The FFmpeg FATE mirror ships the bitstreams only, so the reference is ffmpeg's own decode (FATE
checks it against the conformance suites' yuv MD5s on every ffmpeg change). For each stream:
    name <TAB> frames <TAB> md5
where md5 is the MD5 of every output frame concatenated, cropped, in the stream's native planar
format (8-bit, or 16-bit little endian), and frames is the number of frames ffmpeg wrote
(`-fps_mode passthrough`: no duplication, no dropping). A stream ffmpeg rejects is `name <TAB> - <TAB>`.
"""
import hashlib, os, subprocess, sys

codec, root = sys.argv[1], os.path.expanduser(sys.argv[2])
ff = os.environ.get("FFMPEG", "ffmpeg")
version = subprocess.run([ff, "-version"], capture_output=True, text=True).stdout.splitlines()[0]
print(f"# {codec} reference digests: ffmpeg decode of the pinned conformance-{codec} corpus ({version}).")
print("# name<TAB>frames<TAB>md5 of all output frames (native planar format, cropped). Regenerate with scripts/gen_expected.py.")


def streams():
    for dp, _, fns in os.walk(root):
        for fn in fns:
            if not fn.startswith("."):
                yield os.path.relpath(os.path.join(dp, fn), root)


for rel in sorted(streams()):
    path = os.path.join(root, rel)
    base = [ff, "-v", "error", "-nostdin", "-i", path, "-map", "0:v:0", "-fps_mode", "passthrough", "-noautoscale"]
    p = subprocess.run(base + ["-f", "rawvideo", "-"], capture_output=True)
    if p.returncode != 0 or not p.stdout:
        print(f"{rel}\t-\t")
        continue
    fm = subprocess.run(base + ["-f", "framemd5", "-"], capture_output=True, text=True).stdout
    frames = sum(1 for l in fm.splitlines() if l and not l.startswith("#"))
    print(f"{rel}\t{frames}\t{hashlib.md5(p.stdout).hexdigest()}")
