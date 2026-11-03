//! Shared constants for encoders using the standard 48 kHz CELT mode.
//!
//! These are the decoder's existing definitions, re-exported without conversion or
//! duplication. See [RFC 6716 §4.3](https://www.rfc-editor.org/rfc/rfc6716.html#section-4.3)
//! for decoding and [§5.3](https://www.rfc-editor.org/rfc/rfc6716.html#section-5.3)
//! for encoding. They do not describe arbitrary Opus Custom modes.
//!
//! Available without a feature flag; generated pulse caches, SILK codebooks, and
//! decoder implementation details remain private.

pub use crate::celt::rate::{ALLOC_STEPS, FINE_OFFSET, LOG_MAX_PSEUDO, MAX_FINE_BITS, MAX_PSEUDO, QTHETA_OFFSET, QTHETA_OFFSET_TWOPHASE};
pub use crate::celt::tables::{
    ALLOC, BETA_COEF, BETA_INTRA, E_MEANS, E_PROB_MODEL, EBANDS, EXP2_TABLE8, LOG2_FRAC_TABLE, MAX_LM, NB_EBANDS, OVERLAP, PRED_COEF, PREEMPH,
    SMALL_ENERGY_ICDF, SPREAD_FACTOR, SPREAD_ICDF, TF_SELECT_TABLE, TRIM_ICDF,
};
