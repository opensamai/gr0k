//! Ephemeral tips: one hint line at a time, rendered in the banner rect above the prompt input and cleared after a TTL.
//!
//! Unlike the toast, an ephemeral tip survives typing: only TTL expiry, prompt-box submission, or an explicit clear removes it.
//! A tip that carries a seen-count key stops appearing once `AppView::tip_seen_counts` says it has shown often enough this run.
//! That map is in-memory only, so the counts reset every run.

pub mod clear_detector;
pub mod clipboard_focus;
pub mod ephemeral;
pub mod export_copy;
pub mod plan_nudge;
pub mod render;
pub mod send_now;
pub mod small_screen;
pub mod ssh_wrap;
pub mod word_select;

pub use ephemeral::{DEFAULT_TIP_TICKS, EphemeralTip, EphemeralTipState, tip_row_renderable};

use ratatui::style::Style;
use ratatui::text::{Line, Span};

/// Build a styled [`Line`] from a localized tip template by splitting the
/// translated text around each `styled_value` and styling those segments with
/// `key_style` (bold/key chord), while the surrounding text gets `dim`.
///
/// Each value in `styled_values` is expected to appear at most once in the
/// translated template (in order); the template's literal placeholder args
/// (`key` / `cmd` / `path`) are the chord/command/path tokens themselves, NOT
/// prose, so they stay untranslated. The split is value-based (not
/// `%{key}`-based) so a locale bundle that reorders the sentence still styles
/// the right substring.
pub(crate) fn styled_placeholder_line(
    template: &str,
    styled_values: &[&str],
    dim: Style,
    key_style: Style,
) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut rest: &str = template;
    for value in styled_values {
        let Some(idx) = rest.find(value) else {
            continue;
        };
        let (head, tail) = rest.split_at(idx);
        if !head.is_empty() {
            spans.push(Span::styled(head.to_owned(), dim));
        }
        spans.push(Span::styled((*value).to_owned(), key_style));
        rest = &tail[value.len()..];
    }
    if !rest.is_empty() {
        spans.push(Span::styled(rest.to_owned(), dim));
    }
    Line::from(spans)
}
