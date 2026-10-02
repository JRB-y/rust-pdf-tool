//! egui's bundled proportional font (Ubuntu-Light) has no arrows: `↑`, `↓` and
//! `→` only exist in the bundled monospace font, and the cross U+2715 is in none
//! of them. So we add the monospace font as a last fallback, and the tests below
//! check every character the interface draws.

use eframe::egui;

/// The symbols the interface draws, so the test below can check all of them.
pub const UP: &str = "↑";
pub const DOWN: &str = "↓";
pub const REMOVE: &str = "✖";
pub const ARROW: &str = "→";

/// The default fonts, with the monospace font appended as a fallback.
pub fn definitions() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    let chain = fonts.families.entry(egui::FontFamily::Proportional).or_default();
    if !chain.iter().any(|font| font == "Hack") {
        chain.push("Hack".to_owned());
    }
    fonts
}

#[cfg(test)]
mod tests {
    use eframe::egui::epaint::{AlphaFromCoverage, text::Fonts};

    use super::*;

    fn fonts(definitions: egui::FontDefinitions) -> Fonts {
        Fonts::new(2048, AlphaFromCoverage::default(), definitions)
    }

    /// A character the fonts lack is drawn as an empty box, which is easy to
    /// ship by accident, so check every character of the interface's source -
    /// labels, symbols and accents alike.
    #[test]
    fn every_character_of_the_interface_has_a_glyph() {
        let mut fonts = fonts(definitions());
        let source = concat!(include_str!("app.rs"), include_str!("fonts.rs"));

        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            let font = egui::FontId::new(14.0, family.clone());
            for character in source.chars().filter(|c| !c.is_whitespace()) {
                assert!(
                    fonts.has_glyph(&font, character),
                    "{family:?} cannot draw {character:?} (U+{:04X})",
                    character as u32
                );
            }
        }
    }

    /// Why the fallback above is needed, and why the remove button is not U+2715.
    #[test]
    fn default_fonts_cannot_draw_the_arrows() {
        let proportional = egui::FontId::new(14.0, egui::FontFamily::Proportional);

        let mut default = fonts(egui::FontDefinitions::default());
        for symbol in [UP, DOWN, ARROW] {
            assert!(
                !default.has_glyphs(&proportional, symbol),
                "{symbol} needs no fallback?"
            );
        }

        let mut ours = fonts(definitions());
        for symbol in [UP, DOWN, ARROW, REMOVE] {
            assert!(
                ours.has_glyphs(&proportional, symbol),
                "the fallback should cover {symbol}"
            );
        }
        assert!(
            !ours.has_glyph(&proportional, '\u{2715}'),
            "U+2715 is in none of the bundled fonts, do not use it"
        );
    }
}
