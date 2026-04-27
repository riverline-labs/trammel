// Copyright (c) 2026 Riverline Labs LLC. Licensed under Apache 2.0.
//! Marker attributes consumed by `trammel`. All no-ops at compile time.
//!
//! The `trammel` binary parses source with `syn` and recognizes these
//! attributes to selectively suppress its static checks. The attributes
//! themselves expand to their annotated item unchanged, so they cost nothing
//! at runtime or at compile time beyond the proc-macro invocation.

use proc_macro::TokenStream;

/// Suppresses trammel's `n_plus_one` rule inside the annotated function.
///
/// Use only when the loop genuinely cannot be batched: fan-out writes,
/// polling/retry loops, stream consumers that process one element at a
/// time. Every use is a claim that batching is impossible, not merely
/// inconvenient.
#[proc_macro_attribute]
pub fn allow_n_plus_one(_args: TokenStream, item: TokenStream) -> TokenStream {
    item
}
