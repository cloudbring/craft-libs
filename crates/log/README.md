# craft-log

Native-only `log` backend. Pass the application's log directory and file stem to `init`;
the caller owns platform directory selection. `<APP>_LOG` overrides the settings filter when
nonempty. Filters accept comma-separated levels and target directives, e.g.
`warn,my_app*=info`. `set_filter` updates the settings filter at runtime; an environment
override remains in effect. `recent` returns up to 512 formatted lines, oldest first.
The bounded writer queue drops its oldest entry on overflow; `dropped` reports the total.
The background thread appends and syncs records, rotating at 16 MiB between `<stem>.log`
and `<stem>.1.log` (two files total). One process must own a given directory and stem.
`flush` waits for queued writes and must not be called from a UI event handler. Individual
records larger than the limit are written whole to an otherwise empty file. I/O errors are
counted by `write_errors`; logging does not panic. Repeated `init` calls return the first
logger; an already-installed external backend returns an error.

The differential test copies `deckcraft/apps/deckcraft/src/logging.rs:129` and `:149`.
The family baseline uses UTC and rotates at startup to two numbered backups; this crate uses
local offset timestamps and rotates by size to one backup, as required by its API.
