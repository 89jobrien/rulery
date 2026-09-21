//! Source document and bundle integrity helpers.

use rulery_contracts::{ContentHash, SourceBundle, SourceIntegrity};

/// Computes source-document and source-bundle integrity values.
#[derive(Clone, Debug, Default)]
pub struct IntegrityCalculator;

impl IntegrityCalculator {
    /// Computes the bundle integrity digest using path/bytes framing.
    #[must_use]
    pub fn bundle_integrity(bundle: &SourceBundle) -> SourceIntegrity {
        let frame = Self::bundle_frame_bytes(bundle);
        SourceIntegrity::new(*ContentHash::digest(&frame).as_bytes())
    }

    /// Encodes the canonical bundle frame bytes for testing and diagnostics.
    ///
    /// Documents must already be in [`SourceBundle`] path order. Paths and source content are
    /// length-prefixed and source bytes, including line endings, are retained verbatim. Changing
    /// this frame invalidates existing source-integrity values and lock files.
    #[must_use]
    pub fn bundle_frame_bytes(bundle: &SourceBundle) -> Vec<u8> {
        let mut buffer = Vec::new();
        for document in bundle.documents() {
            let path = document.path().as_str().as_bytes();
            buffer.extend_from_slice(&(path.len() as u64).to_be_bytes());
            buffer.extend_from_slice(path);
            let content = document.content().as_bytes();
            buffer.extend_from_slice(&(content.len() as u64).to_be_bytes());
            buffer.extend_from_slice(content);
        }
        buffer
    }
}
