# craft-resample

Dependency-free rational-ratio polyphase FIR resampler (`L/M`), generic over `f32`/`f64`.

* `Polyphase::from_prototype(h, L, M)` splits any prototype FIR into `L` phases; the caller
  designs the filter (or uses `kaiser_prototype`, a Kaiser-windowed sinc normalised so every
  phase sums to 1: DC passes unchanged and the prototype sums to `L`).
* `process(x, first, count)` computes output samples `first .. first+count` with an exact integer
  position accumulator (no floating-point phase, no drift); `process_all` gives the full output.
* `direct_form` is the literal upsample -> FIR -> downsample reference.

The polyphase form equals the direct form for every input: proved in Lean 4 (`lean/`, see
`lean/TRUST.md`), and checked against exact rational vectors in `tests/lean_vectors.rs`.
This is the shared kernel behind the polyphase paths in soundcraft (`dsp::offline::Polyphase`,
`playback` sinc resampler) and filmcraft (`audio-dsp::oversample`); migrating them is separate work.
