# craft-cfb

Read-only, safe CFB/OLE2 reader. `Cfb::open(&bytes)` validates the header, FAT,
DIFAT, mini-FAT and directory tree. `root()` and `entries()` expose directory
metadata; `read_stream("Storage/Stream")` reads a stream by case-insensitive path.
Limits: 1 GiB per stream, 1024 directory sectors, and at most the physical
sector count for any FAT/DIFAT chain. Malformed references, cycles, duplicate
FAT sectors, and truncated streams return `Error` rather than panicking.

Derived from GridCraft `crates/xlsx/src/cfb.rs` (stream reader), compared with
DeckCraft `crates/pptx/src/cfb.rs` (directory-name detector). The latter is a
trimmed copy: it has no regular/mini-stream reader or directory-tree traversal.
GridCraft's fallback UTF-16 byte scan on malformed directories was intentionally
not carried into this strict API. WordCraft `crates/doc/src/cfb_guard.rs` supplied
the duplicate and overlarge FAT-list preflight. Neither application's production
XLS/PPT export uses a CFB writer: GridCraft writes XLSX as ZIP and DeckCraft
reads CFB only. A writer is therefore out of scope.

Tolerated writer quirks (each seen in real files; `tools/container-oracle` finds them against the
pre-swap readers): a root entry whose name is empty, mis-sized or a path (names are only read for
entries the tree reaches); FREESECT instead of ENDOFCHAIN as the first-DIFAT field when the header's 109
entries suffice; more FAT sectors listed than the file strictly needs (they must still be distinct
sectors of the file); a mini-FAT count with no start sector. Header magic, version, byte order,
mini-stream cutoff, FAT/DIFAT counts and directory structure stay strict.
