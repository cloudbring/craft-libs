#!/usr/bin/env python3
"""Build pinned craft-corpus manifests for the Office and DXF corpora (ticket #181).

Run:  python3 -I crates/corpus/scripts/build_manifest.py [poi|libreoffice|openxml-sdk|ezdxf ...]

For each source it lists the files of a pinned commit through the git trees API (`gh api`), selects
by extension, downloads every selected file from raw.githubusercontent.com at that commit into a
temp dir OUTSIDE the repo, hashes it, derives feature tags (path/name heuristics plus a peek inside
the zip package or the DXF text) and writes crates/corpus/manifests/<name>.toml. Downloaded files
are never committed and are deleted on exit. Bump a pin by editing SHA below and re-running.
"""
import collections, concurrent.futures as cf, hashlib, json, os, re, shutil, subprocess, sys
import tempfile, time, urllib.request, zipfile

MANIFEST_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "manifests")
MAX_BYTES = 20 * 1024 * 1024
OFFICE = {"docx", "xlsx", "pptx", "doc", "xls", "ppt", "docm", "xlsm", "pptm"}
# Raw URLs are not percent-encoded by the fetcher, so only URL-safe paths are selectable.
SAFE_PATH = re.compile(r"^[A-Za-z0-9._/+-]+$")
# Malicious / fuzz-crash names. `poc` only as a word so "epoch" is not caught.
BAD_NAME = re.compile(r"crash|fuzz|clusterfuzz|cve|(?<![a-z])poc(?![a-z])", re.I)

SOURCES = {
    "office-poi": dict(
        repo="apache/poi", branch="trunk", sha="7f4d36854109efa17152b27a5fb1b2c7d842c314",
        exts=OFFICE, subtrees=None, keep=lambda p: p.startswith(("test-data/document/", "test-data/spreadsheet/", "test-data/slideshow/")),
        globs=["test-data/document/**", "test-data/spreadsheet/**", "test-data/slideshow/**"],
        strip="test-data/", licence="Apache-2.0", kind="office",
        description="Apache POI test-data: Word, Excel and PowerPoint documents in OOXML and legacy binary (OLE2) formats, collected by POI from bug reports and unit tests.",
        provenance="https://github.com/apache/poi/tree/{sha}/test-data"),
    "office-libreoffice": dict(
        repo="LibreOffice/core", branch="master", sha="695017480642732e582a811886569d7c6362c056",
        exts=OFFICE, subtrees=["sw", "sc", "sd", "oox", "chart2", "svx", "xmloff", "filter", "starmath", "sfx2", "vcl"],
        keep=lambda p: "/qa/" in p,
        globs=[d + "/qa/**" for d in ["sw", "sc", "sd", "oox", "chart2", "svx", "xmloff", "filter", "starmath", "sfx2", "vcl"]],
        strip=None, licence="MPL-2.0", kind="office",
        description="LibreOffice core QA documents (sw/sc/sd and shared module qa data): regression documents for the Writer, Calc and Impress OOXML and binary import filters.",
        provenance="https://github.com/LibreOffice/core/tree/{sha}"),
    "office-openxml-sdk": dict(
        repo="dotnet/Open-XML-SDK", branch="main", sha="431ab05cf160248cc3885a4a766026d4f8243792",
        exts=OFFICE, subtrees=None, keep=lambda p: True, globs=["**"],
        strip=None, licence="MIT", kind="office",
        description="dotnet/Open-XML-SDK test files: OOXML packages (docx/xlsx/pptx and macro-enabled variants) used by the SDK's validation and round-trip tests.",
        provenance="https://github.com/dotnet/Open-XML-SDK/tree/{sha}"),
    "cad-ezdxf": dict(
        repo="mozman/ezdxf", branch="master", sha="c498560bb491c357deda538360ed0c8fdb11f5a7",
        exts={"dxf"}, subtrees=None, keep=lambda p: True, globs=["**"],
        strip=None, licence="MIT", kind="dxf",
        description="ezdxf test files and examples: ASCII and binary DXF drawings from AutoCAD R12 to R2018 covering entities, blocks, layouts, dimensions and hatches.",
        provenance="https://github.com/mozman/ezdxf/tree/{sha}"),
}


def gh_tree(repo, sha):
    out = subprocess.run(["gh", "api", f"repos/{repo}/git/trees/{sha}?recursive=1"], check=True, capture_output=True, text=True).stdout
    d = json.loads(out)
    if d.get("truncated"):
        raise SystemExit(f"tree {repo}@{sha} truncated")
    return d["tree"]


def list_files(s):
    """[(repo_path, size)] of blobs, walking per-directory subtrees when the repo is too big."""
    if not s["subtrees"]:
        return [(t["path"], t["size"]) for t in gh_tree(s["repo"], s["sha"]) if t["type"] == "blob"]
    root = json.loads(subprocess.run(["gh", "api", f"repos/{s['repo']}/git/trees/{s['sha']}"], check=True, capture_output=True, text=True).stdout)["tree"]
    shas = {t["path"]: t["sha"] for t in root if t["type"] == "tree"}
    out = []
    for d in s["subtrees"]:
        out += [(d + "/" + t["path"], t["size"]) for t in gh_tree(s["repo"], shas[d]) if t["type"] == "blob"]
    return out


def download(url, dest):
    for i in range(5):
        try:
            with urllib.request.urlopen(url, timeout=60) as r, open(dest, "wb") as f:
                shutil.copyfileobj(r, f)
            return
        except Exception:
            if i == 4:
                raise
            time.sleep(1 + i * 2)


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


NAME_TAGS = [
    ("charts", r"chart|graph"), ("pivots", r"pivot"), ("tables", r"(?<![a-z])tables?(?![a-z])"),
    ("comments", r"comment|annot"), ("tracked-changes", r"track|revision|(?<![a-z])ins(?:ert)?del|redline"),
    ("rtl", r"rtl|bidi|arabic|hebrew|farsi|right.?to.?left"), ("fields", r"field|(?<![a-z])toc(?![a-z])|hyperlink"),
    ("math", r"math|equation|omml"), ("macros", r"macro|vba"), ("embedded-objects", r"embed|(?<![a-z])ole(?![a-z])"),
    ("images", r"image|picture|(?<![a-z])img"), ("encrypted", r"encrypt|password|protected"),
]


def office_tags(path, ext, local, size):
    tags = {ext}
    low = path.lower()
    for t, rx in NAME_TAGS:
        if re.search(rx, low):
            tags.add(t)
    if ext in ("docm", "xlsm", "pptm"):
        tags.add("macros")
    if size > 1 << 20:
        tags.add("large")
    with open(local, "rb") as f:
        head = f.read(8)
    if ext in ("doc", "xls", "ppt"):
        tags.add("ole2" if head == b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1" else "non-ole2")
        if size <= 8 << 20:
            data = open(local, "rb").read()
            if "VBA".encode("utf-16-le") in data or b"_VBA_PROJECT" in data:
                tags.add("macros")
        return sorted(tags)
    tags.add("ooxml")
    try:
        z = zipfile.ZipFile(local)
        names = z.namelist()
    except Exception:
        tags.add("not-zip")
        return sorted(tags)

    def has(sub):
        return any(sub in n for n in names)

    def body(n):
        try:
            if z.getinfo(n).file_size > MAX_BYTES:
                return ""
            return z.read(n).decode("utf-8", "replace")
        except Exception:
            return ""

    if has("vbaProject.bin"):
        tags.add("macros")
    if has("/embeddings/") or has("/oleObject"):
        tags.add("embedded-objects")
    if has("/charts/chart") or has("/chart"):
        tags.add("charts")
    if has("/media/"):
        tags.add("images")
    if has("/diagrams/"):
        tags.add("smartart")
    if has("EncryptedPackage") or has("EncryptionInfo"):
        tags.add("encrypted")
    if ext in ("docx", "docm"):
        if has("word/comments"):
            tags.add("comments")
        doc = body("word/document.xml")
        if "<w:ins " in doc or "<w:del " in doc or "<w:moveFrom " in doc:
            tags.add("tracked-changes")
        if "<w:tbl>" in doc or "<w:tbl " in doc:
            tags.add("tables")
        if "<w:bidi" in doc or "<w:rtl" in doc:
            tags.add("rtl")
        if "<w:fldChar" in doc or "<w:fldSimple" in doc or "<w:instrText" in doc:
            tags.add("fields")
        if "<m:oMath" in doc:
            tags.add("math")
        if has("word/footnotes.xml") and "<w:footnote " in body("word/footnotes.xml"):
            tags.add("footnotes")
    elif ext in ("xlsx", "xlsm"):
        if has("xl/pivotTables") or has("xl/pivotCache"):
            tags.add("pivots")
        if has("xl/tables/"):
            tags.add("tables")
        if has("xl/comments") or has("threadedComments"):
            tags.add("comments")
        if has("xl/drawings/") and "charts" not in tags:
            tags.add("drawings")
        for n in names:
            if re.match(r"xl/worksheets/sheet\d+\.xml$", n):
                s = body(n)
                if 'rightToLeft="1"' in s:
                    tags.add("rtl")
                if "<f>" in s or "<f " in s:
                    tags.add("formulas")
                if "<conditionalFormatting" in s:
                    tags.add("conditional-formatting")
                if "<dataValidation" in s:
                    tags.add("data-validation")
        if has("xl/sharedStrings.xml") and "r:id" not in tags:
            pass
    else:
        if has("ppt/notesSlides/"):
            tags.add("notes")
        if has("ppt/comments") or has("ppt/commentAuthors"):
            tags.add("comments")
        for n in names:
            if re.match(r"ppt/slides/slide\d+\.xml$", n):
                s = body(n)
                if 'rtl="1"' in s:
                    tags.add("rtl")
                if "<p:timing" in s:
                    tags.add("animations")
                if "<a:tbl>" in s or "<a:tbl " in s:
                    tags.add("tables")
                if "<p:transition" in s:
                    tags.add("transitions")
    return sorted(tags)


DXF_ENTITIES = ["HATCH", "DIMENSION", "MTEXT", "INSERT", "SPLINE", "LWPOLYLINE", "3DFACE", "MESH", "IMAGE", "LEADER",
                "MLEADER", "VIEWPORT", "3DSOLID", "ELLIPSE", "TEXT", "ATTRIB", "POLYLINE", "SOLID", "UNDERLAY", "MLINE"]


def dxf_tags(path, local, size):
    tags = {"dxf"}
    if size > 1 << 20:
        tags.add("large")
    data = open(local, "rb").read()
    if data.startswith(b"AutoCAD Binary DXF"):
        tags.add("binary")
        return sorted(tags)
    tags.add("ascii")
    text = data.decode("latin-1")
    m = re.search(r"\$ACADVER\s*\r?\n\s*1\s*\r?\n\s*(AC\d+)", text)
    if m:
        tags.add(m.group(1).lower())
    lines = [l.strip() for l in text.splitlines()]
    present = {lines[i + 1].upper() for i in range(0, len(lines) - 1) if lines[i] == "0"}
    for e in DXF_ENTITIES:
        if e in present:
            tags.add(e.lower())
    if "BLOCK" in present:
        tags.add("blocks")
    if "LAYOUT" in present or "OBJECTS" in lines:
        tags.add("layouts")
    if re.search(r"(?i)crash|fuzz|invalid|corrupt|broken", path):
        tags.add("malformed")
    return sorted(tags)


def build(name, s, work):
    sha = s["sha"]
    want, excluded = [], collections.OrderedDict()
    for p, size in sorted(list_files(s)):
        ext = p.rsplit(".", 1)[-1].lower() if "." in p.rsplit("/", 1)[-1] else ""
        if ext not in s["exts"] or not s["keep"](p):
            continue
        if BAD_NAME.search(p):
            excluded[p] = "malicious/fuzz name"
        elif size == 0:
            excluded[p] = "empty file (raw.githubusercontent.com answers 501)"
        elif size > MAX_BYTES:
            excluded[p] = f"over 20 MB ({size} bytes)"
        elif not SAFE_PATH.match(p):
            excluded[p] = "path not URL-safe (space/unicode/special character)"
        else:
            want.append((p, size, ext))
    d = os.path.join(work, name)
    os.makedirs(d)

    def one(item):
        p, size, ext = item
        local = os.path.join(d, hashlib.md5(p.encode()).hexdigest())
        download(f"https://raw.githubusercontent.com/{s['repo']}/{sha}/{p}", local)
        h = sha256(local)
        t = office_tags(p, ext, local, size) if s["kind"] == "office" else dxf_tags(p, local, size)
        os.remove(local)
        return p, h, t

    with cf.ThreadPoolExecutor(12) as ex:
        res = list(ex.map(one, want))
    out = [f"# Generated by crates/corpus/scripts/build_manifest.py -- do not edit by hand; bump the pin there and re-run.",
           f"# Source: {s['repo']} @ {sha} ({s['branch']} at generation time).",
           "# Selection: by extension (" + ", ".join(sorted(s["exts"])) + "), files <= 20 MB, URL-safe paths.",
           f"# Excluded ({len(excluded)}):"]
    out += [f"#   {p}: {why}" for p, why in excluded.items()] or ["#   (none)"]
    out += ["", f'name = "{name}"', f'description = "{s["description"]}"', "", "[[git]]", f'repo = "{s["repo"]}"', f'commit = "{sha}"',
            "paths = [" + ", ".join(f'"{g}"' for g in s["globs"]) + "]"]
    if s["strip"]:
        out.append(f'strip_prefix = "{s["strip"]}"')
    out += [f'licence = "{s["licence"]}"', f'provenance = "{s["provenance"].format(sha=sha)}"', "redistributable = true", ""]
    for p, h, t in res:
        out += ["[[git.file]]", f'path = "{p}"', f'sha256 = "{h}"', "tags = [" + ", ".join(f'"{x}"' for x in t) + "]", ""]
    with open(os.path.join(MANIFEST_DIR, name + ".toml"), "w") as f:
        f.write("\n".join(out))
    exts = collections.Counter(p.rsplit(".", 1)[-1].lower() for p, _, _ in res)
    tags = collections.Counter(x for _, _, t in res for x in t)
    print(json.dumps(dict(name=name, files=len(res), excluded=len(excluded), exts=exts, tags=tags), sort_keys=True))


def main():
    names = sys.argv[1:] or list(SOURCES)
    work = tempfile.mkdtemp(prefix="craft-corpus-build-")
    try:
        for n in names:
            key = n if n in SOURCES else {"poi": "office-poi", "libreoffice": "office-libreoffice", "openxml-sdk": "office-openxml-sdk", "ezdxf": "cad-ezdxf"}[n]
            build(key, SOURCES[key], work)
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()
