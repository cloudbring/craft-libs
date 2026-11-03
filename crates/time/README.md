# craft-time

Shared, dependency-free rational media time for film and effects applications. `RationalTime`
(`Duration` is an alias) stores normalized seconds as an `i64` numerator and nonzero `u32`
denominator. Checked arithmetic returns `TimeError::Overflow` instead of silently drifting or
wrapping. `FrameRate` uses exact frame-rate fractions; `Timecode` validates labels and converts
to/from signed frame counts without floating-point arithmetic. Sample conversions accept any
nonzero `u32` sample rate, and floor toward negative infinity. Timecodes use unbounded hours;
callers needing 24-hour rollover should explicitly reduce their frame count before conversion.
The drop-frame flag belongs to `Timecode`, not `FrameRate`: both DF and NDF use the same
30000/1001 or 60000/1001 physical frame rate.

**Verification pilot:** The drop-frame conversion is the intended Verus/Lean pilot. Its spec:
for nominal 30 fps at 30000/1001, omit labels `;00` and `;01` at the start of every minute
except multiples of ten; for nominal 60 fps at 60000/1001 omit `;00` through `;03` instead.
Physical frames are never omitted, the tenth minute has no skipped labels, and converting any
valid label to a frame and back must preserve it (within the unbounded-hours convention).
The inverse counts `drop * (total_minutes - total_minutes / 10)` skipped labels before a
timecode; the forward map inserts those labels in ten-minute blocks. Differential tests compare
the routines in `filmcraft/crates/time/src/lib.rs` and `effectcraft/crates/time/src/lib.rs`.

SoundCraft's `crates/time/src/tempo.rs` also owns musical meters, bar reflow and f64 BPM/sample
conversions; it is not a small standalone tempo-segment extraction and remains app-specific.
