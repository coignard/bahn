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

use std::ops::Range;

use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Mask, Paint, Path, PathBuilder, Pixmap, Point,
    Rect, SpreadMode, Stroke, Transform,
};

use crate::font::{Fonts, Weight};
use crate::monitor::Monitor;

pub(crate) const WIDTH: f32 = 1024.0;
pub(crate) const HEIGHT: f32 = 768.0;

pub(crate) type Rgb = [u8; 3];

#[derive(Clone, Copy)]
pub(crate) struct Style {
    pub(crate) size: f32,
    weight: Weight,
    color: Rgb,
}

impl Style {
    pub(crate) const fn regular(size: f32, color: Rgb) -> Self {
        Self {
            size,
            weight: Weight::Regular,
            color,
        }
    }

    pub(crate) const fn bold(size: f32, color: Rgb) -> Self {
        Self {
            size,
            weight: Weight::Bold,
            color,
        }
    }

    pub(crate) const fn sized(self, size: f32) -> Self {
        Self { size, ..self }
    }
}

pub(crate) struct Canvas<'a> {
    pixmap: Pixmap,
    fonts: &'a Fonts,
    transform: Transform,
    mask: Option<Mask>,
}

impl<'a> Canvas<'a> {
    #[expect(clippy::cast_precision_loss)]
    pub(crate) fn new(fonts: &'a Fonts, monitor: Monitor, scale: u32) -> Option<Self> {
        let pixmap = Pixmap::new(monitor.width() * scale, monitor.height() * scale)?;
        let [left, top, width, height] = monitor.screen().map(|points| (points * scale) as f32);
        Some(Self {
            pixmap,
            fonts,
            transform: Transform::from_row(width / WIDTH, 0.0, 0.0, height / HEIGHT, left, top),
            mask: None,
        })
    }

    pub(crate) fn into_pixmap(self) -> Pixmap {
        self.pixmap
    }

    pub(crate) fn clear(&mut self, color: Rgb) {
        self.fill(0.0, 0.0, WIDTH, HEIGHT, color, 1.0);
    }

    pub(crate) fn clip(&mut self, rows: Option<Range<f32>>) {
        self.mask = rows.and_then(|rows| {
            let area = self.area(0.0, rows.start, WIDTH, rows.end - rows.start)?;
            let mut mask = Mask::new(self.pixmap.width(), self.pixmap.height())?;
            let path = PathBuilder::from_rect(area);
            mask.fill_path(&path, FillRule::Winding, false, Transform::identity());
            Some(mask)
        });
    }

    pub(crate) fn fill(&mut self, x: f32, y: f32, width: f32, height: f32, color: Rgb, alpha: f32) {
        if let Some(area) = self.area(x, y, width, height)
            && alpha > 0.0
        {
            let paint = paint(color, alpha);
            let mask = self.mask.as_ref();
            self.pixmap
                .fill_rect(area, &paint, Transform::identity(), mask);
        }
    }

    pub(crate) fn rule(
        &mut self,
        x: f32,
        span: Range<f32>,
        width: f32,
        fades: [f32; 2],
        color: Rgb,
    ) {
        let length = span.end - span.start;
        let Some(area) = self.area(x - width / 2.0, span.start, width, length) else {
            return;
        };
        let [head, tail] = fades.map(|fade| (fade / length).clamp(0.0, 0.5));
        let stops = vec![
            GradientStop::new(0.0, color_of(color, 0.0)),
            GradientStop::new(head, color_of(color, 1.0)),
            GradientStop::new(1.0 - tail, color_of(color, 1.0)),
            GradientStop::new(1.0, color_of(color, 0.0)),
        ];
        let start = Point::from_xy(area.left(), area.top());
        let end = Point::from_xy(area.left(), area.bottom());
        if let Some(shader) =
            LinearGradient::new(start, end, stops, SpreadMode::Pad, Transform::identity())
        {
            let paint = Paint {
                shader,
                ..Paint::default()
            };
            let mask = self.mask.as_ref();
            self.pixmap
                .fill_rect(area, &paint, Transform::identity(), mask);
        }
    }

    pub(crate) fn square(&mut self, x: f32, y: f32, size: f32, color: Rgb, alpha: f32) {
        self.fill(x - size / 2.0, y - size / 2.0, size, size, color, alpha);
    }

    pub(crate) fn ring(
        &mut self,
        x: f32,
        y: f32,
        size: f32,
        thickness: f32,
        color: Rgb,
        alpha: f32,
    ) {
        let middle = size - thickness;
        if let Some(area) = Rect::from_xywh(x - middle / 2.0, y - middle / 2.0, middle, middle)
            && alpha > 0.0
        {
            let stroke = Stroke {
                width: thickness,
                ..Stroke::default()
            };
            let path = PathBuilder::from_rect(area);
            let mask = self.mask.as_ref();
            self.pixmap
                .stroke_path(&path, &paint(color, alpha), &stroke, self.transform, mask);
        }
    }

    pub(crate) fn triangle(&mut self, corners: [(f32, f32); 3], color: Rgb, alpha: f32) {
        let [(x0, y0), (x1, y1), (x2, y2)] = corners;
        let mut path = PathBuilder::new();
        path.move_to(x0, y0);
        path.line_to(x1, y1);
        path.line_to(x2, y2);
        path.close();
        self.shape(path.finish(), color, alpha);
    }

    pub(crate) fn text(&mut self, text: &str, style: Style, x: f32, y: f32, alpha: f32) {
        let font = self.fonts.get(style.weight);
        let baseline = y + font.cap_height(style.size) / 2.0;
        self.shape(
            font.outline(text, style.size, x, baseline),
            style.color,
            alpha,
        );
    }

    pub(crate) fn width(&self, text: &str, style: Style) -> f32 {
        self.fonts.get(style.weight).width(text, style.size)
    }

    pub(crate) fn fit(&self, text: &str, style: Style, width: f32) -> String {
        let text = self.fonts.get(style.weight).printable(text);
        if self.width(&text, style) <= width {
            return text;
        }
        text.char_indices()
            .rev()
            .map(|(end, _)| format!("{}…", text[..end].trim_end()))
            .find(|shortened| self.width(shortened, style) <= width)
            .unwrap_or_default()
    }

    fn area(&self, x: f32, y: f32, width: f32, height: f32) -> Option<Rect> {
        Rect::from_xywh(x, y, width, height)?.transform(self.transform)
    }

    fn shape(&mut self, path: Option<Path>, color: Rgb, alpha: f32) {
        if let Some(path) = path
            && alpha > 0.0
        {
            let mask = self.mask.as_ref();
            let paint = paint(color, alpha);
            self.pixmap
                .fill_path(&path, &paint, FillRule::Winding, self.transform, mask);
        }
    }
}

fn paint(color: Rgb, alpha: f32) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(color_of(color, alpha));
    paint
}

fn color_of([red, green, blue]: Rgb, alpha: f32) -> Color {
    let mut color = Color::from_rgba8(red, green, blue, u8::MAX);
    color.apply_opacity(alpha);
    color
}
