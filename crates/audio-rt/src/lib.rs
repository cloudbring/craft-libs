#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

//! Realtime callback building blocks; no device backend or application policy.
//! Construct buffers on the control thread and move the split handles to their owners.
//! The audio callback must never block, allocate, or perform I/O.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// A bounded, owned single-producer/single-consumer queue.
///
/// Construction allocates; subsequent `push` and `pop` are wait-free. The returned
/// handles must each be used by only one thread. A zero capacity is not useful.
/// Destructors of queued values run on the thread that pops them: use `Copy`
/// samples or transfer expensive-to-drop values back to a control thread.
pub struct Spsc<T> {
    producer: SpscProducer<T>,
    consumer: SpscConsumer<T>,
}

/// The sole writing endpoint of an [`Spsc`].
pub struct SpscProducer<T>(rtrb::Producer<T>);
/// The sole reading endpoint of an [`Spsc`].
pub struct SpscConsumer<T>(rtrb::Consumer<T>);

impl<T> Spsc<T> {
    /// Allocate `capacity` slots on a non-realtime thread.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let (producer, consumer) = rtrb::RingBuffer::new(capacity);
        Self { producer: SpscProducer(producer), consumer: SpscConsumer(consumer) }
    }

    /// Move the endpoints to their respective threads.
    pub fn split(self) -> (SpscProducer<T>, SpscConsumer<T>) {
        (self.producer, self.consumer)
    }
}

impl<T> SpscProducer<T> {
    /// Return the value unchanged when full; never waits or grows.
    pub fn push(&mut self, value: T) -> Result<(), T> {
        self.0.push(value).map_err(|rtrb::PushError::Full(value)| value)
    }

    /// Currently available slots (may change concurrently).
    pub fn slots(&self) -> usize {
        self.0.slots()
    }
}

impl<T: Copy> SpscProducer<T> {
    /// Push as many values as fit and return the number accepted.
    pub fn push_slice(&mut self, source: &[T]) -> usize {
        let count = source.len().min(self.slots());
        let mut pushed = 0;
        for &value in &source[..count] {
            if self.push(value).is_err() {
                break;
            }
            pushed += 1;
        }
        pushed
    }
}

impl<T> SpscConsumer<T> {
    /// Return `None` when empty; never waits.
    pub fn pop(&mut self) -> Option<T> {
        self.0.pop().ok()
    }

    /// Currently readable slots (may change concurrently).
    pub fn slots(&self) -> usize {
        self.0.slots()
    }
}

impl<T: Copy> SpscConsumer<T> {
    /// Copy available values into the destination and return the number read.
    pub fn pop_slice(&mut self, destination: &mut [T]) -> usize {
        let count = destination.len().min(self.slots());
        let mut popped = 0;
        for slot in &mut destination[..count] {
            if let Some(value) = self.pop() {
                *slot = value;
            } else {
                break;
            }
            popped += 1;
        }
        popped
    }
}

/// A shared, lock-free f32 parameter. Stores exact IEEE-754 bits, including NaN.
pub struct AtomicParam(AtomicU32);

impl AtomicParam {
    pub const fn new(value: f32) -> Self {
        Self(AtomicU32::new(value.to_bits()))
    }

    pub fn store(&self, value: f32) {
        self.0.store(value.to_bits(), Ordering::Relaxed);
    }

    pub fn load(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }

    /// Fill a block with a linear ramp from `previous` to one sampled target.
    /// The last sample is the target; an empty block leaves `previous` unchanged.
    /// Call only with finite parameter values when a finite ramp is required.
    pub fn ramp(&self, previous: &mut f32, block: &mut [f32]) {
        if block.is_empty() {
            return;
        }
        let target = self.load();
        let step = (target - *previous) / block.len() as f32;
        for (index, sample) in block.iter_mut().enumerate() {
            *sample = *previous + step * (index + 1) as f32;
        }
        *previous = target;
    }
}

/// Three owned snapshot buffers: one editable, one audible, and one spare.
///
/// No clones or drops happen in the audio handoff. Publication can fail if a
/// previous snapshot is still pending; the editor keeps its unchanged buffer
/// and can retry later. The audio thread never observes a partially edited T.
pub struct Triple<T> {
    edit: TripleEdit<T>,
    audio: TripleAudio<T>,
}

/// Sole edit-thread endpoint of [`Triple`].
pub struct TripleEdit<T> {
    edit: T,
    spare: Option<T>,
    to_audio: SpscProducer<T>,
    from_audio: SpscConsumer<T>,
}

/// Sole audio-thread endpoint of [`Triple`].
pub struct TripleAudio<T> {
    audio: T,
    from_edit: SpscConsumer<T>,
    to_edit: SpscProducer<T>,
}

impl<T> Triple<T> {
    /// Allocate the two one-slot transfer queues outside the callback.
    pub fn new(edit: T, audio: T, spare: T) -> Self {
        let (to_audio, from_edit) = Spsc::new(1).split();
        let (mut to_edit, from_audio) = Spsc::new(1).split();
        let _ = to_edit.push(spare);
        Self { edit: TripleEdit { edit, spare: None, to_audio, from_audio }, audio: TripleAudio { audio, from_edit, to_edit } }
    }

    /// Move each endpoint to its owning thread.
    pub fn split(self) -> (TripleEdit<T>, TripleAudio<T>) {
        (self.edit, self.audio)
    }
}

impl<T> TripleEdit<T> {
    /// Mutate the private, unpublished snapshot on the control thread.
    pub fn edit_mut(&mut self) -> &mut T {
        &mut self.edit
    }

    /// Hand off the entire snapshot; false means retry after an audio poll.
    pub fn publish(&mut self) -> bool {
        if self.to_audio.slots() == 0 {
            return false;
        }
        let Some(spare) = self.spare.take().or_else(|| self.from_audio.pop()) else { return false };
        let published = std::mem::replace(&mut self.edit, spare);
        match self.to_audio.push(published) {
            Ok(()) => true,
            Err(published) => {
                let spare = std::mem::replace(&mut self.edit, published);
                self.spare = Some(spare);
                false
            }
        }
    }
}

impl<T> TripleAudio<T> {
    /// Immutable access to the last fully published snapshot.
    pub fn current(&self) -> &T {
        &self.audio
    }

    /// Swap in a pending snapshot; return false if there is no new snapshot.
    /// The previous snapshot is sent back to the edit thread to be reused.
    pub fn poll(&mut self) -> bool {
        if self.to_edit.slots() == 0 {
            return false;
        }
        let Some(next) = self.from_edit.pop() else { return false };
        let previous = std::mem::replace(&mut self.audio, next);
        let _ = self.to_edit.push(previous);
        true
    }
}

/// Thin device-independent contract for an app's cpal callback adapter.
pub trait AudioCallback {
    fn render(&mut self, output: &mut [f32]);
}

impl<F: FnMut(&mut [f32])> AudioCallback for F {
    fn render(&mut self, output: &mut [f32]) {
        self(output);
    }
}

/// Silences the current and all later blocks after a callback panic.
///
/// The panic hook executes before unwind is caught: apps must install a
/// realtime-safe panic hook (no logging or I/O) before starting the stream.
/// Compile with `panic=unwind`; `panic=abort` cannot be caught.
#[derive(Default)]
pub struct PanicGuard {
    failed: AtomicBool,
}

impl PanicGuard {
    pub const fn new() -> Self {
        Self { failed: AtomicBool::new(false) }
    }

    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }

    /// Returns true if this block was rendered, false if silence was written.
    pub fn render(&self, callback: &mut impl AudioCallback, output: &mut [f32]) -> bool {
        if self.failed() {
            output.fill(0.0);
            return false;
        }
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback.render(output))).is_err() {
            self.failed.store(true, Ordering::Relaxed);
            output.fill(0.0);
            return false;
        }
        true
    }
}

/// A callback-side atomic heartbeat and a UI-side interval watchdog.
/// Construct and observe the detector on the UI thread; only `beat` belongs
/// in the callback. A heartbeat is not proof the audio content is correct.
pub struct StallDetector {
    beats: Arc<AtomicU64>,
    threshold: Duration,
    seen: u64,
    since: Instant,
}

/// Cloneable callback-side heartbeat; clone only on the control thread.
pub struct StallBeat(Arc<AtomicU64>);

impl StallBeat {
    /// One atomic increment, no clock read or syscall.
    pub fn beat(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

impl StallDetector {
    pub fn new(threshold: Duration, now: Instant) -> (Self, StallBeat) {
        let beats = Arc::new(AtomicU64::new(0));
        (Self { beats: Arc::clone(&beats), threshold, seen: 0, since: now }, StallBeat(beats))
    }

    pub fn beats(&self) -> u64 {
        self.beats.load(Ordering::Relaxed)
    }

    /// Call on the UI thread. A stopped stream resets the interval.
    pub fn observe(&mut self, playing: bool, now: Instant) -> bool {
        let beats = self.beats();
        if !playing || beats != self.seen {
            self.seen = beats;
            self.since = now;
            return false;
        }
        now.saturating_duration_since(self.since) >= self.threshold
    }
}

#[cfg(test)]
mod tests;
