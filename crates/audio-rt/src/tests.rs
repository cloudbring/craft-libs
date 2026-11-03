use super::{AtomicParam, PanicGuard, Spsc, StallDetector, Triple};
use proptest::prelude::*;
use std::time::{Duration, Instant};

struct RtAllocCheck;

impl RtAllocCheck {
    fn assert_no_alloc(run: impl FnOnce()) {
        let measured = allocation_counter::measure(run);
        assert_eq!(measured.count_total, 0, "callback allocated: {measured:?}");
    }
}

#[test]
fn spsc_capacity_wrap_and_slices() {
    let (mut tx, mut rx) = Spsc::new(3).split();
    assert_eq!(rx.pop(), None);
    assert_eq!(tx.push_slice(&[1, 2, 3, 4]), 3);
    assert_eq!(tx.push(5), Err(5));
    assert_eq!(rx.pop(), Some(1));
    assert_eq!(rx.pop(), Some(2));
    assert_eq!(tx.push_slice(&[5, 6, 7]), 2);
    let mut samples = [0; 5];
    assert_eq!(rx.pop_slice(&mut samples), 3);
    assert_eq!(samples, [3, 5, 6, 0, 0]);
    assert_eq!(rx.pop(), None);
    let (mut empty_tx, mut empty_rx) = Spsc::<u8>::new(0).split();
    assert_eq!(empty_tx.push(1), Err(1));
    assert_eq!(empty_rx.pop(), None);
}

#[test]
fn spsc_million_values_between_threads() {
    const ITEMS: usize = 1_000_000;
    let (mut tx, mut rx) = Spsc::new(37).split();
    let producer = std::thread::spawn(move || {
        for index in 0..ITEMS {
            while tx.push(index).is_err() {
                std::hint::spin_loop();
            }
        }
    });
    for expected in 0..ITEMS {
        let actual = loop {
            if let Some(value) = rx.pop() {
                break value;
            }
            std::hint::spin_loop();
        };
        assert_eq!(actual, expected);
    }
    assert!(producer.join().is_ok());
    assert_eq!(rx.pop(), None);
}

#[test]
fn atomic_ramp_reaches_target_and_preserves_empty_block() {
    let param = AtomicParam::new(1.0);
    let mut previous = 0.0;
    param.ramp(&mut previous, &mut []);
    assert_eq!(previous, 0.0);
    let mut block = [0.0; 4];
    param.ramp(&mut previous, &mut block);
    assert_eq!(block, [0.25, 0.5, 0.75, 1.0]);
    param.store(-1.0);
    param.ramp(&mut previous, &mut block);
    assert_eq!(block, [0.5, 0.0, -0.5, -1.0]);
    param.store(f32::from_bits(0x7fc0_0042));
    assert_eq!(param.load().to_bits(), 0x7fc0_0042);
}

proptest! {
    #[test]
    fn triple_never_returns_torn_snapshot(versions in proptest::collection::vec(1_u64..100_000, 1..256)) {
        let (mut edit, mut audio) = Triple::new([0; 64], [0; 64], [0; 64]).split();
        for version in versions {
            for cell in edit.edit_mut().iter_mut() {
                *cell = version;
            }
            prop_assert!(edit.publish());
            prop_assert!(audio.poll());
            prop_assert!(audio.current().iter().all(|&cell| cell == version));
        }
    }
}

#[test]
fn triple_million_thread_handoffs() {
    const ITEMS: u64 = 1_000_000;
    let (mut edit, mut audio) = Triple::new([0_u64; 16], [0; 16], [0; 16]).split();
    let worker = std::thread::spawn(move || {
        for version in 1..=ITEMS {
            edit.edit_mut().fill(version);
            while !edit.publish() {
                std::hint::spin_loop();
            }
        }
    });
    for expected in 1..=ITEMS {
        while !audio.poll() {
            std::hint::spin_loop();
        }
        assert!(audio.current().iter().all(|&value| value == expected));
    }
    assert!(worker.join().is_ok());
}

#[test]
fn panic_guard_silences_and_latches() {
    let guard = PanicGuard::new();
    let mut output = [1.0; 8];
    let mut callback = |out: &mut [f32]| {
        out[0] = 100.0;
        std::panic::panic_any(5_u8);
    };
    assert!(!guard.render(&mut callback, &mut output));
    assert_eq!(output, [0.0; 8]);
    assert!(guard.failed());
    let mut calls = 0;
    let mut callback = |_: &mut [f32]| calls += 1;
    assert!(!guard.render(&mut callback, &mut output));
    assert_eq!(calls, 0);
}

#[test]
fn stall_threshold_and_pause_reset() {
    let now = Instant::now();
    let (mut detector, callback) = StallDetector::new(Duration::from_millis(50), now);
    assert!(!detector.observe(true, now + Duration::from_millis(49)));
    assert!(detector.observe(true, now + Duration::from_millis(50)));
    callback.beat();
    assert!(!detector.observe(true, now + Duration::from_millis(100)));
    assert!(detector.observe(true, now + Duration::from_millis(150)));
    assert!(!detector.observe(false, now + Duration::from_millis(200)));
    assert!(!detector.observe(true, now + Duration::from_millis(201)));
    assert!(detector.observe(true, now + Duration::from_millis(250)));
}

#[test]
fn callback_paths_do_not_allocate() {
    let (mut tx, mut rx) = Spsc::new(8).split();
    let (mut edit, mut audio) = Triple::new([0_u32; 32], [0; 32], [0; 32]).split();
    RtAllocCheck::assert_no_alloc(|| {
        assert_eq!(tx.push_slice(&[1, 2, 3]), 3);
        let mut block = [0; 3];
        assert_eq!(rx.pop_slice(&mut block), 3);
        assert_eq!(block, [1, 2, 3]);
        edit.edit_mut().fill(42);
        assert!(edit.publish());
        assert!(audio.poll());
        assert_eq!(audio.current()[0], 42);
    });
}

#[test]
fn soundcraft_drain_pattern_drop_in() {
    // soundcraft/crates/playback/src/callback.rs:135 drains commands with `while let Ok(c) = self.rx.try_recv()`.
    struct OldCallback {
        rx: std::sync::mpsc::Receiver<u32>,
    }
    impl OldCallback {
        fn drain(&self, handled: &mut Vec<u32>) {
            while let Ok(c) = self.rx.try_recv() {
                handled.push(c);
            }
        }
    }
    let (old_tx, old_rx) = std::sync::mpsc::channel();
    let old = OldCallback { rx: old_rx };
    let (mut tx, mut rx) = Spsc::new(8).split();
    for command in [1, 2, 3] {
        assert!(old_tx.send(command).is_ok());
        assert_eq!(tx.push(command), Ok(()));
    }
    let mut old_handled = Vec::new();
    old.drain(&mut old_handled);
    let mut handled = Vec::new();
    while let Some(command) = rx.pop() {
        handled.push(command);
    }
    assert_eq!(old_handled, handled);
    assert_eq!(handled, [1, 2, 3]);
    old.drain(&mut old_handled);
    assert_eq!(rx.pop(), None);
}
