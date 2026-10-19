//! The colours of the interface.
//!
//! The scheme is a matrix: a near-black ground, moss green for everything that carries
//! meaning, a yellow line for the keys, and a mandarin title over it. The greens are the
//! colour of moss, so the whole thing reads like a phosphor screen rather than a neon one.

use ratatui::style::Color;

/// The palette the interface is painted with.
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    /// The ground the whole interface stands on.
    pub background: Color,
    /// The ground of the panels.
    pub surface: Color,
    /// The lines around the panels.
    pub border: Color,
    /// The lines around the panel that has the attention.
    pub border_focus: Color,
    /// The titles of the panels.
    pub title: Color,
    /// The text itself.
    pub text: Color,
    /// The text that only carries the small print.
    pub dim: Color,
    /// The empty part of the progress bar.
    pub track: Color,
    /// The part of the progress bar that has been played.
    pub fill: Color,
    /// The head of the progress bar.
    pub head: Color,
    /// The keys.
    pub accent: Color,
    /// The title over the keys.
    pub accent_title: Color,
    /// The ground of the song under the cursor.
    pub highlight_bg: Color,
    /// The text of the song under the cursor.
    pub highlight_fg: Color,
}

/// The matrix: moss green, with a yellow key line and a mandarin title over it.
pub const THEME: Theme = Theme {
    background: Color::Rgb(0x08, 0x0d, 0x08),
    surface: Color::Rgb(0x0f, 0x16, 0x0f),
    border: Color::Rgb(0x2e, 0x4a, 0x2a),
    border_focus: Color::Rgb(0x6e, 0x84, 0x49),
    title: Color::Rgb(0x8a, 0x9a, 0x5b),
    text: Color::Rgb(0xc3, 0xd3, 0xa2),
    dim: Color::Rgb(0x4f, 0x6a, 0x44),
    track: Color::Rgb(0x1b, 0x29, 0x18),
    fill: Color::Rgb(0x6e, 0x84, 0x49),
    head: Color::Rgb(0xc9, 0xe7, 0x9e),
    accent: Color::Rgb(0xe3, 0xb3, 0x41),
    accent_title: Color::Rgb(0xf2, 0x85, 0x22),
    highlight_bg: Color::Rgb(0x8a, 0x9a, 0x5b),
    highlight_fg: Color::Rgb(0x08, 0x0d, 0x08),
};
