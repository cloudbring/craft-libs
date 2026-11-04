#!/usr/bin/env python3
"""Regenerate expected.toml from a run's details.tsv (CODEC_ORACLE_OUT/details.tsv).

Every distinct (scenario, set, metric) that the run reports as a difference is mapped by the RULES
below to the hardening commit, fork-provenance fact or intended change that explains it. A group
that no rule explains is printed and makes the script fail: it is a regression to root-cause, not
something to register. The rules are reviewed text, not heuristics hidden in the harness; the
document docs/verification/codec-oracle.md lists the evidence behind each.

Usage: gen_expected.py details.tsv > expected.toml
"""
import re, sys, collections

H264_FIX = "craft-libs e0f337e (h264: upstream deckcraft and soundcraft hardening, #48)"
HEVC_FIX = "craft-libs 0d907d5 (hevc: upstream deckcraft hardening, #48)"
AV1_FIX = "craft-libs 2f6b4f1 (av1: upstream deckcraft hardening, #48)"
VP9_131 = "filmcraft 409f5418 (bitstream: one boolean decoder for vp8 and vp9, #131), carried into craft-libs 9aafc3f"
OPUS_49 = "craft-libs 2688653 (craft-opus on craft-fft, #49): MDCT through craft-fft instead of the private transform"
DRIFT_H264 = ("the forks were snapshots older than the filmcraft code the shared crate was moved from; filmcraft added these checks later "
              "(ee871ca0 #2894 long-term DPB limits, 1ef29605 #1441 PPS validation, 4328e49a #2915 bounded picture allocation)")


def rule(scenario, set_, metric):
    """Return (fix_id, reason, evidence) or None."""
    attr = metric.split("|")[1] if "|" in metric else ""
    sig = "|".join(metric.split("|")[2:])
    aspects = metric.split("|")[0]
    codec = scenario.split("-")[0]
    app = scenario.split("-")[-1]
    avcc = "avcc" in scenario
    hvcc = "hvcc" in scenario

    if codec == "h264":
        if avcc:
            if attr in ("fix", "both") or "avcC" in sig:
                return ("h264-avcc-validation", "decoder.rs: configure_avcc validates parameter-set NAL types, lengths and truncation before touching state, and keeps the previous configuration on failure (the unhardened fork records the NAL length size / parameter sets of a record it then rejects)", H264_FIX + "; deckcraft 3e178d8 decoder.rs; test avcc_validates_types_and_keeps_configuration_on_failure")
            if attr == "drift":
                return ("h264-fork-snapshot", "the soundcraft fork lacked filmcraft's PPS validation (message and order differ); " + DRIFT_H264, "filmcraft 1ef29605; soundcraft crates/video/MIGRATION-48.md")
        if attr == "drift":
            why = ("the deckcraft and soundcraft forks lack filmcraft's later checks: " if "MMCO long-term" in sig or "pic_init_q" in sig else "the fork ends differently from the filmcraft code the crate was moved from (the pre-fix shared crate already equals the new one): ")
            if aspects == "plane_bytes":
                why = "PPS weighted_bipred_idc = 3 (a bit flip of qp50.h264 byte 35): filmcraft conceals the invalid value, the fork keeps 3 (verified by parsing the PPS with both crates): "
            return ("h264-fork-snapshot", why + DRIFT_H264, "filmcraft ee871ca0, 1ef29605, 4328e49a; deckcraft docs/h264-fork-divergence.md (10 stale files); soundcraft crates/video/MIGRATION-48.md")
        if "panic@dpb.rs" in sig or "abs_diff_pic_num" in sig or "too many ref_pic_list_modification" in sig:
            return ("h264-dpb-bounds", "dpb.rs: reference-list modification and MMCO arithmetic is bounded (the unhardened fork panics on overflow, or accepts an out-of-range abs_diff_pic_num)", H264_FIX + "; soundcraft b35dd0f crates/video/src/h264/dpb.rs; crates/h264/tests/upstream_regressions.rs ref_list_mod_huge_value_does_not_panic")
        if "panic@slice.rs" in sig or "panic@accum.rs" in sig or "slice QP overflow" in sig or "slice QP out of range" in sig:
            return ("h264-qp-poc-checked", "slice.rs: checked QP addition and saturating POC arithmetic (the unhardened fork panics on overflow)", H264_FIX + "; deckcraft 3e178d8 slice.rs, soundcraft b35dd0f slice.rs; upstream_regressions.rs huge_slice_qp_delta_is_invalid, poc0_huge_bottom_delta_does_not_overflow, poc_extreme_offsets_do_not_overflow")
        if "frame number gap exceeds" in sig:
            return ("h264-frame-num-gaps", "decoder.rs: large frame_num gaps are bounded and decodable instead of rejected with 'frame number gap exceeds the supported reference limit' (a deckcraft-only fix, absent from the filmcraft code the crate came from)", H264_FIX + "; deckcraft 3e178d8 decoder.rs; synth_tests large_frame_num_gap_remains_bounded_and_decodable")
        if aspects == "plane_bytes" and attr in ("fix", "both") and not sig:
            return ("h264-transform-clamp", "transform.rs: coefficients and the DC shortcut are bounded before the inverse transform; on hostile coefficient levels the pictures differ. File-level bisect: reverting only transform.rs of the shared crate to its pre-fix version restores the old output", H264_FIX + "; deckcraft 3e178d8 transform.rs, soundcraft b35dd0f transform.rs; tools/codec-oracle/bisect")
        if attr in ("drift", "both"):
            return ("h264-fork-snapshot", "difference between a fork and the filmcraft code the shared crate was moved from (pre-fix craft-h264 equals the new crate here): " + DRIFT_H264, "filmcraft ee871ca0, 1ef29605, 4328e49a; deckcraft docs/h264-fork-divergence.md (10 stale files); soundcraft crates/video/MIGRATION-48.md")
    if codec == "hevc":
        if "budget" in scenario:
            return ("hevc-budget-accounting", "decoder.rs F2: the shared crate budgets one byte per sample when both planes are at most 8 bits, the deckcraft fork budgeted u16 for every sample, so for a given budget the two accept at different thresholds (about half apart on 8-bit streams); the error wording differs. Documented in the craft-hevc README ('Unlike the fork's all-u16 storage'); the threshold, not the exactness of the check, differs", HEVC_FIX + "; crates/hevc/README.md memory budget paragraph")
        if hvcc:
            return ("hevc-hvcc-preflight", "decoder.rs F1: typed two-pass hvcC preflight (array NAL type, header agreement, truncation) with state untouched on failure; the unhardened code accepts mismatched/AUD/VCL arrays, or mutates the NAL length size / parameter stores of a record it rejects", HEVC_FIX + "; deckcraft 3e178d8 hevc decoder.rs; README table F1")
        if "init_qp" in sig and "panic@params.rs" in sig or "init_qp_minus" in sig:
            return ("hevc-pps-qp", "params.rs F3: PPS init_qp_minus26 range check (the unhardened code accepts -75, or panics at 26 + read_se())", HEVC_FIX + "; deckcraft 3e178d8 hevc params.rs; README table F3")
        if "panic@slice.rs" in sig or "SliceQpY" in sig:
            return ("hevc-slice-qp", "slice.rs F4: slice QP range check in header parsing (the unhardened code panics on i32::MAX)", HEVC_FIX + "; README table F4")
        if "delta_chroma_logN_weight_denom" in sig or "ChromaLogNWeightDenom" in sig:
            return ("hevc-weight-denominator", "slice.rs F5: chroma weight denominator checked (message and check location differ; same error class and unit)", HEVC_FIX + "; README table F5")
        if attr == "fix" and "slice data overrun" in sig:
            return ("hevc-pps-qp", "params.rs F3: PPS init_qp range is now rejected at the PPS, before the slice header is read", HEVC_FIX + "; README table F3")
        if attr in ("drift", "both"):
            return ("hevc-fork-snapshot", "difference between the deckcraft hevc fork and the filmcraft code the shared crate came from", "deckcraft docs/hevc-fork-divergence.md (8 stale files)")
    if codec == "av1":
        if app == "deckcraft" and attr in ("drift", "both"):
            return ("av1-fork-snapshot", "the deckcraft av1 fork predates filmcraft's hostile-input hardening: it turns a block-size/subsampling panic into 'frame decode panicked' (new: an explicit 'block size has no chroma size' error, or 'decoder panicked'), and words its frame-size budget error differently; same error class, same unit", "filmcraft cb2ac66c (Harden AV1 and MPEG-2 decoders against hostile input), 62c16fcb (frame worker panic becomes a decoding error); deckcraft docs/av1-fork-divergence.md (16 stale files)")
        if "tile group out of order" in sig or "tile_group" in sig:
            return ("av1-tile-group-order", "decoder.rs: tile groups must be contiguous and in order ('tile group out of order'); the unhardened code reports 'missing tile coverage' or 'truncated' instead", AV1_FIX + "; deckcraft 3e178d8 av1 decoder.rs")
        if "tile" in sig:
            return ("av1-tile-bounds", "header.rs: explicit tile-column and tile-row bounds", AV1_FIX)
        if attr == "fix" and aspects in ("err_at",):
            return ("av1-tile-group-order", "decoder.rs: the same tile-group ordering check fires one temporal unit earlier", AV1_FIX)
    if codec == "vp9":
        if "frame has no tile data" in sig:
            return ("vp9-fork-snapshot", "the deckcraft vp9 fork accepts a frame with no tile data (corrupt after a header bit flip) that the filmcraft code rejects ('frame has no tile data'); the pre-fix shared crate already equals the new one here", "filmcraft 99ec34de (security(vp9): bound frame allocation, #2930); deckcraft docs/vp9-fork-divergence.md (11 stale files)")
        if attr in ("drift", "both") and ("Boolean" in sig or "err-Boolean" in aspects or "end_ok_err-Boolean" in aspects or "Boolean" in metric):
            return ("vp9-strict-bool-decoder", "VP9 boolean partitions are now validated (marker bit, exhaustion, nonzero padding) and the error propagates; the forks decoded corrupt partitions with implicit zero extension. Valid streams are unaffected (every libvpx vector and ffmpeg fixture is identical). NOT on the #48 hardening list: it comes from #131", VP9_131)
        if "Boolean" in sig or "Boolean" in aspects:
            return ("vp9-strict-bool-decoder", "same as above, class change only", VP9_131)
    if codec == "opus":
        if aspects == "pcm_bytes" and attr == "drift":
            return ("opus-celt-mdct", "float-level differences from the MDCT now computed by craft-fft (largest sample difference stays below 1e-5 of full scale, range states equal); both decoders stay far above the opus_compare quality threshold", OPUS_49)
    return None


def slug(s):
    return re.sub(r"[^A-Za-z0-9]+", "-", s).strip("-").lower()


def esc(s):
    return s.replace("\\", "\\\\").replace('"', '\\"')


def main():
    groups = collections.OrderedDict()
    for line in open(sys.argv[1]):
        name, outcome, _detail = line.rstrip("\n").split("\t", 2)
        m = re.search(r'metric: "((?:[^"\\]|\\.)*)"', outcome)
        if not m:
            groups.setdefault((name.split("/")[0], name.split("/")[1], "OUTCOME " + outcome[:60]), 0)
            continue
        scenario, set_ = name.split("/")[:2]
        groups[(scenario, set_, m.group(1).replace('\\"', '"'))] = groups.get((scenario, set_, m.group(1).replace('\\"', '"')), 0) + 1
    unexplained = []
    out = ["# Intended old-vs-new differences of the #48 codec de-fork, over the ffmpeg matrix, conformance vectors,",
           "# hostile mutants, header sweeps, value-extreme havoc, synthetic streams and avcC/hvcC records of tools/codec-oracle.",
           "#",
           "# GENERATED by scripts/gen_expected.py from a reviewed run: one entry per (scenario, set, metric). `metric` is",
           "# `<aspects>|<attribution>[|old=<site>|new=<site>]`; attribution is `fix` (old == shared crate before the hardening",
           "# commit, != after), `drift` (old != shared crate before the hardening commit, == after: the fork differed from the",
           "# code the crate was moved from) or `both`. Root causes and counts per entry: docs/verification/codec-oracle.md.",
           ""]
    ids = set()
    for (scenario, set_, metric), n in groups.items():
        r = rule(scenario, set_, metric)
        if r is None:
            unexplained.append((scenario, set_, metric, n))
            continue
        fid, reason, evidence = r
        eid = f"{scenario}.{set_}:{fid}:{slug(metric)[:60]}"
        base, k = eid, 2
        while eid in ids:
            eid, k = f"{base}-{k}", k + 1
        ids.add(eid)
        out += ["[[expected]]", f'id = "{esc(eid)}"', f'reason = "{esc(reason)}"', f'evidence = "{esc(evidence)}"', f'glob = "{scenario}/{set_}/**"', f'metric = "{esc(metric)}"', ""]
    sys.stdout.write("\n".join(out))
    if unexplained:
        sys.stderr.write("UNEXPLAINED (regressions to root-cause):\n")
        for u in unexplained:
            sys.stderr.write(f"  {u[3]:5d} {u[0]}/{u[1]} {u[2]}\n")
        sys.exit(1)


main()
