//! Lightweight span helpers for YAML parsing.

use rulery_contracts::{SourceKey, SourceMap, Span};

/// Returns a best-effort span for the first match of `needle`.
pub fn span_for(source_map: &SourceMap, source_key: SourceKey, text: &str, needle: &str) -> Span {
    let start = text
        .find(needle)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0);
    let width = u32::try_from(needle.len()).unwrap_or(1);
    source_map
        .span(source_key, start, start.saturating_add(width))
        .or_else(|_| source_map.span(source_key, 0, 0))
        .expect("source map has anchor span")
}
