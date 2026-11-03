#![cfg(feature = "filters")]
//! Differential fixtures copied verbatim from pdfcraft commit 3936f93a1479366a951206fc6d3094680fe2d3dc:
//! `crates/filters/src/lzw.rs` and `crates/filters/src/flate.rs`.

use craft_pdf::filters::{Filter, FilterChain, Limits, Mode};

#[allow(warnings, clippy::all)]
#[rustfmt::skip]
#[path = "pdfcraft_reference/lzw.rs"]
mod lzw;
#[allow(warnings, clippy::all)]
#[rustfmt::skip]
#[path = "pdfcraft_reference/flate.rs"]
mod flate;

#[allow(dead_code)]
#[derive(Debug)]
enum FilterError {
    Corrupt { filter: &'static str, detail: String },
    LimitExceeded(usize),
}

#[allow(dead_code)]
#[derive(Debug)]
struct Failure {
    error: FilterError,
    partial: Vec<u8>,
    discarded: usize,
}

impl Failure {
    fn corrupt(filter: &'static str, detail: impl Into<String>, partial: Vec<u8>) -> Self {
        Self { error: FilterError::Corrupt { filter, detail: detail.into() }, partial, discarded: 0 }
    }
    fn limit(max: usize) -> Self {
        Self { error: FilterError::LimitExceeded(max), partial: Vec::new(), discarded: 0 }
    }
}

type Step = Result<Vec<u8>, Failure>;

#[test]
fn random_valid_inputs_match_pdfcraft() -> Result<(), Box<dyn std::error::Error>> {
    let mut seed = 0x4d59_5df4_d0f3_3173u64;
    for length in [0, 1, 31, 512, 4096, 20_000] {
        for repetition in 0..4 {
            let mut plain = Vec::with_capacity(length);
            for _ in 0..length {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                plain.push(if repetition == 0 { (seed % 8) as u8 } else { seed as u8 });
            }
            let chain = |filter| FilterChain {
                filters: vec![filter],
                limits: Limits { max_input: 100_000, max_output: 100_000, max_ratio: 100_000, max_depth: 1 },
                mode: Mode::Strict,
            };
            let encoded = flate::encode(&plain);
            let reference = flate::decode(&encoded, 100_000).map_err(|error| format!("{error:?}"))?;
            assert_eq!(chain(Filter::Flate).decode(&encoded).map_err(|error| format!("{error:?}"))?.data, reference);
            for early_change in [false, true] {
                let encoded = lzw::encode(&plain, early_change);
                let reference = lzw::decode(&encoded, early_change, 100_000).map_err(|error| format!("{error:?}"))?;
                assert_eq!(chain(Filter::Lzw { early_change }).decode(&encoded).map_err(|error| format!("{error:?}"))?.data, reference);
            }
        }
    }
    Ok(())
}
