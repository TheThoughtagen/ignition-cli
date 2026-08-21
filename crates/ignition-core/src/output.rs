//! Success envelope rendering — the LOCKED Phase-1 output shape.
//!
//! Success: `{"ok":true,"profile":<name|null>,"data":{...}}` with exactly
//! those top-level fields (the failure twin lives in [`crate::error`]).
//! Field order is declaration order and part of the golden-file contract.
//!
//! Core NEVER prints — these functions return `String`s; the binary owns
//! stdout/stderr (ARCHITECTURE.md layering invariant).

use serde::Serialize;

use crate::error::ErrorEnvelope;

/// LOCKED success envelope: exactly the top-level fields `ok`, `profile`,
/// `data` — changing the set is a breaking change for agents.
#[derive(Debug, Serialize)]
pub struct JsonEnvelope<'a, T: Serialize + ?Sized> {
    /// Always `true` in this envelope.
    pub ok: bool,
    /// Active profile echoed in every output (CORE-01); `None` until config
    /// resolution lands.
    pub profile: Option<&'a str>,
    /// The command's payload.
    pub data: &'a T,
}

/// Render a success envelope: pretty (default `--json`) or one-line compact
/// (`--compact`). Field order: `ok`, `profile`, `data`.
///
/// # Panics
/// Panics only if serialization of a well-formed model fails — impossible
/// for the crate's output models (no map keys, no IO); a violation is a bug
/// worth surfacing loudly rather than an empty-but-successful render.
pub fn render_success<T>(profile: Option<&str>, data: &T, compact: bool) -> String
where
    T: Serialize + ?Sized,
{
    let envelope = JsonEnvelope {
        ok: true,
        profile,
        data,
    };
    serialize(&envelope, compact)
}

/// Render a failure envelope: pretty or compact. The caller routes the
/// result to stderr in both modes.
///
/// # Panics
/// See [`render_success`].
pub fn render_failure(envelope: &ErrorEnvelope<'_>, compact: bool) -> String {
    serialize(envelope, compact)
}

fn serialize<T>(value: &T, compact: bool) -> String
where
    T: Serialize + ?Sized,
{
    if compact {
        serde_json::to_string(value)
    } else {
        serde_json::to_string_pretty(value)
    }
    .expect("envelope serialization cannot fail for well-formed models")
}
