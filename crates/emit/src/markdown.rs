//! Stable Markdown explanation rendering.

use rulery_engine::Truth;

/// Ascending-order Markdown renderer.
#[derive(Clone, Copy, Debug, Default)]
pub struct MarkdownRenderer;

impl MarkdownRenderer {
    /// Renders truth states without reinterpreting Unknown or Invalid as failure.
    #[must_use]
    pub fn render_truths(&self, entries: &[(&str, Truth)]) -> String {
        let mut entries = entries.to_vec();
        entries.sort_by_key(|(name, _)| *name);
        let mut output = String::new();
        for (name, truth) in entries {
            output.push_str("- `");
            output.push_str(name);
            output.push_str("`: ");
            output.push_str(truth_label(truth));
            output.push('\n');
        }
        output
    }
}

const fn truth_label(truth: Truth) -> &'static str {
    match truth {
        Truth::True => "True",
        Truth::False => "False",
        Truth::Unknown => "Unknown",
        Truth::Invalid => "Invalid",
    }
}
