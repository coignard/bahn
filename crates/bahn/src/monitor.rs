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

use tiny_skia::Pixmap;

use crate::frame::Frame;

const LEFT: u32 = 16;
const TOP: u32 = 17;
const RIGHT: u32 = 16;
const BOTTOM: u32 = 18;
const SMALLEST_SCREEN: u32 = 256;
const SCREEN_STEP: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Monitor {
    width: u32,
    height: u32,
}

impl Default for Monitor {
    fn default() -> Self {
        Self::with_screen(512)
    }
}

impl Monitor {
    pub fn wide(width: f64) -> Self {
        Self::with_screen(snap(width - f64::from(LEFT + RIGHT)))
    }

    pub fn tall(height: f64) -> Self {
        Self::with_screen(snap((height - f64::from(TOP + BOTTOM)) * 4.0 / 3.0))
    }

    pub fn fitting(bounds: Frame) -> Self {
        Self::wide(bounds.width).min(Self::tall(bounds.height))
    }

    pub fn smallest() -> Self {
        Self::with_screen(SMALLEST_SCREEN)
    }

    pub fn width(self) -> u32 {
        self.width
    }

    pub fn height(self) -> u32 {
        self.height
    }

    pub(crate) fn screen(self) -> [u32; 4] {
        [
            LEFT,
            TOP,
            self.width - LEFT - RIGHT,
            self.height - TOP - BOTTOM,
        ]
    }

    fn with_screen(width: u32) -> Self {
        let width = width.max(SMALLEST_SCREEN);
        Self {
            width: width + LEFT + RIGHT,
            height: width / 4 * 3 + TOP + BOTTOM,
        }
    }
}

pub(crate) struct Bezel {
    art: Pixmap,
}

impl Default for Bezel {
    fn default() -> Self {
        Self {
            art: Pixmap::decode_png(include_bytes!("../assets/monitor.png"))
                .expect("the embedded bezel should decode"),
        }
    }
}

impl Bezel {
    pub(crate) fn draw(&self, pixmap: &mut Pixmap, monitor: Monitor, scale: u32) {
        let stride = pixmap.width();
        let pixels = pixmap.pixels_mut();
        let [left, top, width, height] = monitor.screen();
        let screen = (left..left + width, top..top + height);
        for y in 0..monitor.height {
            let row = slice(y, monitor.height, self.art.height(), TOP, BOTTOM);
            for x in (0..monitor.width).filter(|x| !(screen.0.contains(x) && screen.1.contains(&y)))
            {
                let column = slice(x, monitor.width, self.art.width(), LEFT, RIGHT);
                let Some(color) = self
                    .art
                    .pixel(column, row)
                    .filter(|color| color.alpha() > 0)
                else {
                    continue;
                };
                for line in y * scale..(y + 1) * scale {
                    let start = index(line * stride + x * scale);
                    pixels[start..start + index(scale)].fill(color);
                }
            }
        }
    }
}

fn slice(target: u32, target_length: u32, source_length: u32, head: u32, tail: u32) -> u32 {
    if target < head {
        target
    } else if target >= target_length - tail {
        source_length - (target_length - target)
    } else {
        head + (target - head) % (source_length - head - tail)
    }
}

fn index(position: u32) -> usize {
    usize::try_from(position).expect("pixel positions should fit in memory")
}

#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn snap(screen_width: f64) -> u32 {
    (screen_width / f64::from(SCREEN_STEP)).floor().max(0.0) as u32 * SCREEN_STEP
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_screen_four_by_three_inside_a_fixed_bezel() {
        let monitor = Monitor::default();
        assert_eq!((monitor.width(), monitor.height()), (544, 419));
        assert_eq!(monitor.screen(), [16, 17, 512, 384]);
        assert_eq!(Monitor::wide(807.0), Monitor::wide(800.0));
        assert_eq!(Monitor::tall(f64::from(monitor.height())), monitor);
        assert_eq!(Monitor::wide(0.0), Monitor::smallest());
    }

    #[test]
    fn fits_the_largest_monitor_into_a_screen() {
        let screen = Frame {
            x: 0.0,
            y: 0.0,
            width: 1512.0,
            height: 982.0,
        };
        let largest = Monitor::fitting(screen);
        assert_eq!((largest.width(), largest.height()), (1288, 977));
        for width in (0..4000).step_by(53) {
            let monitor = Monitor::wide(f64::from(width));
            assert!(monitor.width() <= width.max(Monitor::smallest().width()));
        }
    }

    #[test]
    fn grows_in_steps_that_keep_the_dither_seamless() {
        for width in (0..2000).step_by(37) {
            let monitor = Monitor::wide(f64::from(width));
            let [_, _, screen_width, _] = monitor.screen();
            assert!(screen_width.is_multiple_of(SCREEN_STEP));
            assert!(screen_width >= SMALLEST_SCREEN);
            assert!(monitor.width().is_multiple_of(2) && !monitor.height().is_multiple_of(2));
        }
    }

    #[test]
    fn draws_the_bezel_pixel_for_pixel_at_any_scale() {
        let bezel = Bezel::default();
        let monitor = Monitor::default();
        for scale in [1, 2] {
            let mut pixmap =
                Pixmap::new(monitor.width() * scale, monitor.height() * scale).unwrap();
            bezel.draw(&mut pixmap, monitor, scale);
            let shade = |x: u32, y: u32| pixmap.pixel(x * scale, y * scale).unwrap();
            assert_eq!(shade(0, 0).alpha(), 0);
            assert_eq!(shade(1, 1).red(), 255);
            assert_eq!(shade(8, 200).red(), 192);
            assert_eq!(shade(16, 17).alpha(), 0);
            assert_eq!(shade(monitor.width() - 1, monitor.height() - 2).red(), 0);
            let block = pixmap.pixel(scale - 1, scale * 2 - 1).unwrap();
            assert_eq!(block, shade(0, 1));
        }
    }
}
