//! `Polyphase::process_from`: outputs at an arbitrary upsampled start index equal the literal
//! direct form `y[n] = (h * u)[start + n*M]`. Integer-valued data keeps every sum exact, so the
//! comparison is bit-for-bit.

use craft_resample::Polyphase;

fn reference(x: &[f64], h: &[f64], up: usize, down: usize, start: usize, count: usize) -> Vec<f64> {
    let mut u = vec![0.0; x.len().saturating_sub(1) * up + 1];
    for (j, &v) in x.iter().enumerate() {
        u[j * up] = v;
    }
    (0..count)
        .map(|n| {
            let m = start + n * down;
            h.iter().enumerate().filter_map(|(k, &c)| m.checked_sub(k).and_then(|j| u.get(j)).map(|&v| c * v)).sum()
        })
        .collect()
}

#[test]
fn start_offsets_match_the_direct_form() -> Result<(), Box<dyn std::error::Error>> {
    let x: Vec<f64> = (0..40).map(|i| f64::from((i * 7 + 3) % 11) - 5.0).collect();
    for (up, down) in [(1, 1), (2, 1), (3, 2), (160, 147), (147, 160), (5, 7), (1, 3)] {
        let h: Vec<f64> = (0..up * 6).map(|k| f64::from((k as i32 * 5 + 1) % 9) - 4.0).collect();
        let pp = Polyphase::from_prototype(&h, up, down).map_err(|e| e.to_string())?;
        for lead in [0usize, 1, 5, 9] {
            for first in [0usize, 3] {
                let start = lead * up + first * down;
                let got = pp.process_from(&x, start as u128, 30);
                assert_eq!(got, reference(&x, &h, up, down, start, 30), "L={up} M={down} lead={lead} first={first}");
            }
        }
        // The non-allocating form writes the same samples.
        let mut buf = vec![0.0; 30];
        pp.process_from_into(&x, (2 * up) as u128, &mut buf);
        assert_eq!(buf, pp.process_from(&x, (2 * up) as u128, 30));
        // `process` is `process_from` at `first * M`.
        assert_eq!(pp.process(&x, 4, 20), pp.process_from(&x, (4 * down) as u128, 20));
    }
    Ok(())
}
