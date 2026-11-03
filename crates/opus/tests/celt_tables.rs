//! FNV-1a snapshots of the pre-export decoder constants, in array iteration order.
//! Integers use little-endian bytes; floats use their IEEE-754 bits and `usize`
//! uses a fixed u64 encoding so checksums agree on native and wasm32 targets.

use craft_opus::celt_tables;

fn checksum(bytes: impl IntoIterator<Item = u8>) -> u64 {
    bytes.into_iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3))
}

macro_rules! checksum_test {
    ($name:ident, $expected:expr, $bytes:expr) => {
        #[test]
        fn $name() {
            assert_eq!(checksum($bytes), $expected);
        }
    };
}

checksum_test!(nb_ebands, 0xfd8e8595a0626930, (celt_tables::NB_EBANDS as u64).to_le_bytes());
checksum_test!(ebands, 0xa4b79a006c04c149, celt_tables::EBANDS.into_iter().flat_map(i32::to_le_bytes));
checksum_test!(overlap, 0x6fa3822c8c7270fd, (celt_tables::OVERLAP as u64).to_le_bytes());
checksum_test!(max_lm, 0xc7c2bf3b330983e6, (celt_tables::MAX_LM as u64).to_le_bytes());
checksum_test!(preemph, 0x17b61d7caaafaa37, celt_tables::PREEMPH.to_bits().to_le_bytes());
checksum_test!(alloc, 0xab15ddb0a84b8c60, celt_tables::ALLOC.into_iter().flatten());
checksum_test!(e_means, 0xc9a7c54a8e4e0e3f, celt_tables::E_MEANS.into_iter().flat_map(|value| value.to_bits().to_le_bytes()));
checksum_test!(pred_coef, 0xb9292257c5dfe3e1, celt_tables::PRED_COEF.into_iter().flat_map(|value| value.to_bits().to_le_bytes()));
checksum_test!(beta_coef, 0x0bb99dfff4d56c77, celt_tables::BETA_COEF.into_iter().flat_map(|value| value.to_bits().to_le_bytes()));
checksum_test!(beta_intra, 0x28d26e7cb450ffbe, celt_tables::BETA_INTRA.to_bits().to_le_bytes());
checksum_test!(e_prob_model, 0xab8360d1090ae161, celt_tables::E_PROB_MODEL.into_iter().flatten().flatten());
checksum_test!(small_energy_icdf, 0xea9ca21875dc49e4, celt_tables::SMALL_ENERGY_ICDF);
checksum_test!(log2_frac_table, 0xa4e7845551e6ffba, celt_tables::LOG2_FRAC_TABLE.into_iter().flat_map(i32::to_le_bytes));
checksum_test!(tf_select_table, 0xb1001468c0cfb4f2, celt_tables::TF_SELECT_TABLE.into_iter().flatten().flat_map(i8::to_le_bytes));
checksum_test!(spread_icdf, 0x19708c3f093fe1f3, celt_tables::SPREAD_ICDF);
checksum_test!(trim_icdf, 0x6ac6dff3c980049f, celt_tables::TRIM_ICDF);
checksum_test!(exp2_table8, 0xdee123a376879c6f, celt_tables::EXP2_TABLE8.into_iter().flat_map(i32::to_le_bytes));
checksum_test!(spread_factor, 0xc811975e2b273705, celt_tables::SPREAD_FACTOR.into_iter().flat_map(|value| (value as u64).to_le_bytes()));
checksum_test!(max_pseudo, 0x4da555b788c4665d, celt_tables::MAX_PSEUDO.to_le_bytes());
checksum_test!(log_max_pseudo, 0x0d301e6ef1629ad3, celt_tables::LOG_MAX_PSEUDO.to_le_bytes());
checksum_test!(max_fine_bits, 0x4cfad6c24f7bf87d, celt_tables::MAX_FINE_BITS.to_le_bytes());
checksum_test!(fine_offset, 0x2ceadadb521d2e30, celt_tables::FINE_OFFSET.to_le_bytes());
checksum_test!(qtheta_offset, 0xcd3ac65e44f721b1, celt_tables::QTHETA_OFFSET.to_le_bytes());
checksum_test!(qtheta_offset_twophase, 0x4cd037050129dd05, celt_tables::QTHETA_OFFSET_TWOPHASE.to_le_bytes());
checksum_test!(alloc_steps, 0x0d301e6ef1629ad3, celt_tables::ALLOC_STEPS.to_le_bytes());
