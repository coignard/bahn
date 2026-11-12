// This file is part of Bahn.
//
// Copyright (c) 2026  René Coignard <contact@renecoignard.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use skrifa::charmap::Charmap;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{OutlineGlyphCollection, OutlinePen};
use skrifa::{FontRef, GlyphId, MetadataProvider};
use tiny_skia::{Path, PathBuilder};

#[derive(Clone, Copy)]
pub(crate) enum Weight {
    Regular,
    Bold,
}

pub(crate) struct Fonts {
    regular: Font,
    bold: Font,
}

impl Default for Fonts {
    fn default() -> Self {
        Self {
            regular: Font::new(include_bytes!("../assets/LiberationSans-Regular.ttf")),
            bold: Font::new(include_bytes!("../assets/LiberationSans-Bold.ttf")),
        }
    }
}

impl Fonts {
    pub(crate) fn get(&self, weight: Weight) -> &Font {
        match weight {
            Weight::Regular => &self.regular,
            Weight::Bold => &self.bold,
        }
    }
}

pub(crate) struct Font {
    face: FontRef<'static>,
    charmap: Charmap<'static>,
    outlines: OutlineGlyphCollection<'static>,
    cap_height: f32,
}

impl Font {
    fn new(data: &'static [u8]) -> Self {
        let face = FontRef::new(data).expect("embedded fonts should parse");
        let metrics = face.metrics(Size::unscaled(), LocationRef::default());
        let cap_height = metrics
            .cap_height
            .expect("embedded fonts should know their cap height");
        Self {
            charmap: face.charmap(),
            outlines: face.outline_glyphs(),
            cap_height: cap_height / f32::from(metrics.units_per_em),
            face,
        }
    }

    pub(crate) fn cap_height(&self, size: f32) -> f32 {
        self.cap_height * size
    }

    pub(crate) fn printable(&self, text: &str) -> String {
        let kept: String = text
            .chars()
            .filter(|&character| self.charmap.map(character).is_some() || character.is_whitespace())
            .collect();
        kept.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    pub(crate) fn width(&self, text: &str, size: f32) -> f32 {
        let metrics = self
            .face
            .glyph_metrics(Size::new(size), LocationRef::default());
        self.glyphs(text)
            .filter_map(|glyph| metrics.advance_width(glyph))
            .sum()
    }

    pub(crate) fn outline(&self, text: &str, size: f32, x: f32, baseline: f32) -> Option<Path> {
        let metrics = self
            .face
            .glyph_metrics(Size::new(size), LocationRef::default());
        let mut pen = Pen {
            path: PathBuilder::new(),
            x,
            baseline,
        };
        for glyph in self.glyphs(text) {
            if let Some(outline) = self.outlines.get(glyph) {
                let _ = outline.draw(Size::new(size), &mut pen);
            }
            pen.x += metrics.advance_width(glyph).unwrap_or_default();
        }
        pen.path.finish()
    }

    fn glyphs(&self, text: &str) -> impl Iterator<Item = GlyphId> {
        text.chars()
            .filter_map(|character| self.charmap.map(character))
    }
}

struct Pen {
    path: PathBuilder,
    x: f32,
    baseline: f32,
}

impl Pen {
    fn point(&self, x: f32, y: f32) -> (f32, f32) {
        (self.x + x, self.baseline - y)
    }
}

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.point(x, y);
        self.path.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.point(x, y);
        self.path.line_to(x, y);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let ((cx0, cy0), (x, y)) = (self.point(cx0, cy0), self.point(x, y));
        self.path.quad_to(cx0, cy0, x, y);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let ((cx0, cy0), (cx1, cy1), (x, y)) =
            (self.point(cx0, cy0), self.point(cx1, cy1), self.point(x, y));
        self.path.cubic_to(cx0, cy0, cx1, cy1, x, y);
    }

    fn close(&mut self) {
        self.path.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hidden_colon_keeps_the_clock_in_place() {
        let font = &Fonts::default().regular;
        assert!((font.width("21:27", 46.0) - font.width("21 27", 46.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn tidies_up_what_is_left_of_text_it_cannot_draw() {
        let font = &Fonts::default().regular;
        assert_eq!(font.printable("🎉 Party  🥳\ttime "), "Party time");
        assert_eq!(font.printable("Обед в 13:00"), "Обед в 13:00");
    }

    #[test]
    fn leaves_out_characters_it_cannot_draw() {
        let font = &Fonts::default().regular;
        assert!((font.width("Party 🎉", 30.0) - font.width("Party ", 30.0)).abs() < f32::EPSILON);
        assert!(font.width("Обед", 30.0) > 0.0);
    }
}
