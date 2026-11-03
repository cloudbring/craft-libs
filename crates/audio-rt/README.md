# craft-audio-rt

Backend-independent realtime primitives for SoundCraft, FilmCraft, EffectCraft,
DeckCraft and other audio apps. Device discovery, cpal stream setup, sample-format
conversion, reconnect policy, and app-specific mixing stay in the apps. Implement
`AudioCallback` in the app's cpal glue and call `PanicGuard::render` with its output
block; call `StallBeat::beat` on each callback, and `StallDetector::observe` on the UI
thread. For soundcraft's clip index/session snapshot, prepare **three** owned buffers
off the callback and use `Triple::new(edit, audio, spare).split()`; retry `publish` when false.

## Realtime contract

- No locks, allocations, logging, file/device I/O, syscalls or waits in the callback.
  Construct queues, snapshot buffers, heartbeat handles and any DSP state on the
  control thread. Use `Spsc::split()` to give each endpoint one owner.
- `Spsc` wraps `rtrb` (MIT/Apache-2.0). Its safe API provides wait-free push/pop;
  its internally audited unsafe implementation is **not** copied into this crate.
  No unsafe code in craft-audio-rt. `push_slice` and `pop_slice` copy the prefix that
  fits; they never allocate or block. For expensive-to-drop items, send ownership
  back to the control thread instead of destroying them in the callback.
- `Triple` passes ownership of whole snapshots through two bounded SPSC queues;
  it never copies, allocates or tears an in-flight snapshot. At most one snapshot
  is pending. If the audio thread has not returned the spare, publishing fails;
  keep editing or retry after the next callback. Each buffer may hold preallocated
  storage, but the app must ensure its own `T` does not allocate on the callback.
- `AtomicParam` is a relaxed atomic f32 bit pattern. `ramp` samples its target
  once per block and fills a linear ramp (last sample reaches target). DSP code
  should clamp/reject nonfinite targets when finite output is required.
- `PanicGuard` catches an unwind and permanently silences the stream until the
  guard is replaced. A panic hook runs *before* the catch: install an RT-safe hook
  (no stderr/logging/allocations) outside the callback. With `panic=abort`, no
  catch is possible. Unwinding an arbitrary app callback is not guaranteed to
  be allocation-free; the guard is a last-resort failure boundary, not a license
  to panic in realtime code.
- `StallDetector` measures elapsed time only on the UI thread. The audio handle
  increments a heartbeat without querying the OS clock. The UI supplies its
  own `Instant`, threshold and playing state.

## wasm32

The crate builds for `wasm32-unknown-unknown`: its atomics work for single-thread
use. Cross-thread use requires an environment with shared-memory WebAssembly
atomics and the corresponding target/browser runtime support; without those,
do not place queue endpoints on different Web Workers. `Instant`, unwinding and
the device glue depend on the host runtime; use an app-specific browser audio
adapter and don't assume a panic in a wasm callback can be caught.
