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

use crate::monitor::Monitor;

const EDGE: f64 = 8.0;
const CORNER: f64 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grip {
    Left,
    Right,
    Bottom,
    Top,
    BottomLeft,
    BottomRight,
    TopLeft,
    TopRight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Low,
    High,
}

impl Frame {
    #[must_use]
    pub fn settle(self, bounds: Self) -> Self {
        let monitor = Monitor::wide(self.width).min(Monitor::fitting(bounds));
        let height = f64::from(monitor.height());
        Self {
            x: self.x.round(),
            y: (self.y + self.height - height).round(),
            width: f64::from(monitor.width()),
            height,
        }
        .inside(bounds)
    }

    fn inside(self, bounds: Self) -> Self {
        let right_most = bounds.x + bounds.width - self.width;
        let top_most = bounds.y + bounds.height - self.height;
        Self {
            x: self.x.min(right_most).max(bounds.x),
            y: self.y.min(top_most).max(bounds.y),
            ..self
        }
    }
}

impl Grip {
    const ALL: [Self; 8] = [
        Self::Left,
        Self::Right,
        Self::Bottom,
        Self::Top,
        Self::BottomLeft,
        Self::BottomRight,
        Self::TopLeft,
        Self::TopRight,
    ];

    pub fn at(x: f64, y: f64, monitor: Monitor) -> Option<Self> {
        let (width, height) = (f64::from(monitor.width()), f64::from(monitor.height()));
        if !(0.0..width).contains(&x) || !(0.0..height).contains(&y) {
            return None;
        }
        let horizontal = Side::of(x, width, EDGE);
        let vertical = Side::of(y, height, EDGE);
        let sides = (
            horizontal.or(vertical.and(Side::of(x, width, CORNER))),
            vertical.or(horizontal.and(Side::of(y, height, CORNER))),
        );
        Self::ALL.into_iter().find(|grip| grip.sides() == sides)
    }

    pub fn stretch(self, frame: Frame, by: (f64, f64), bounds: Frame) -> Frame {
        let (horizontal, vertical) = self.sides();
        let wide = horizontal.map(|side| Monitor::wide(frame.width + side.outward(by.0)));
        let tall = vertical.map(|side| Monitor::tall(frame.height + side.outward(by.1)));
        let monitor = wide
            .max(tall)
            .unwrap_or_else(|| Monitor::wide(frame.width))
            .min(Monitor::fitting(bounds));
        let (width, height) = (f64::from(monitor.width()), f64::from(monitor.height()));
        Frame {
            x: anchor(horizontal, frame.x, frame.width, width),
            y: anchor(vertical, frame.y, frame.height, height),
            width,
            height,
        }
        .inside(bounds)
    }

    fn sides(self) -> (Option<Side>, Option<Side>) {
        use Side::{High, Low};
        match self {
            Self::Left => (Some(Low), None),
            Self::Right => (Some(High), None),
            Self::Bottom => (None, Some(Low)),
            Self::Top => (None, Some(High)),
            Self::BottomLeft => (Some(Low), Some(Low)),
            Self::BottomRight => (Some(High), Some(Low)),
            Self::TopLeft => (Some(Low), Some(High)),
            Self::TopRight => (Some(High), Some(High)),
        }
    }
}

impl Side {
    fn of(position: f64, length: f64, reach: f64) -> Option<Self> {
        if position < reach {
            Some(Self::Low)
        } else if position >= length - reach {
            Some(Self::High)
        } else {
            None
        }
    }

    fn outward(self, by: f64) -> f64 {
        match self {
            Self::Low => -by,
            Self::High => by,
        }
    }
}

fn anchor(pulled: Option<Side>, start: f64, length: f64, new: f64) -> f64 {
    match pulled {
        Some(Side::Low) => start + length - new,
        Some(Side::High) => start,
        None => start + (length - new) / 2.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Frame = Frame {
        x: 0.0,
        y: 0.0,
        width: 1512.0,
        height: 982.0,
    };

    fn board(x: f64, y: f64) -> Frame {
        let monitor = Monitor::default();
        Frame {
            x,
            y,
            width: f64::from(monitor.width()),
            height: f64::from(monitor.height()),
        }
    }

    fn is_on(frame: Frame, bounds: Frame) -> bool {
        frame.x >= bounds.x
            && frame.y >= bounds.y
            && frame.x + frame.width <= bounds.x + bounds.width
            && frame.y + frame.height <= bounds.y + bounds.height
    }

    #[test]
    fn grips_the_edges_of_the_bezel_and_wider_corners() {
        let monitor = Monitor::default();
        assert_eq!(Grip::at(3.0, 200.0, monitor), Some(Grip::Left));
        assert_eq!(Grip::at(540.0, 200.0, monitor), Some(Grip::Right));
        assert_eq!(Grip::at(270.0, 2.0, monitor), Some(Grip::Bottom));
        assert_eq!(Grip::at(270.0, 415.0, monitor), Some(Grip::Top));
        assert_eq!(Grip::at(20.0, 2.0, monitor), Some(Grip::BottomLeft));
        assert_eq!(Grip::at(541.0, 400.0, monitor), Some(Grip::TopRight));
        assert_eq!(Grip::at(12.0, 200.0, monitor), None);
        assert_eq!(Grip::at(270.0, 200.0, monitor), None);
        assert_eq!(Grip::at(-1.0, 200.0, monitor), None);
        assert_eq!(Grip::at(270.0, 419.0, monitor), None);
    }

    #[test]
    fn stretches_from_the_far_edge_and_keeps_the_other_axis_centred() {
        let from = board(100.0, 100.0);
        let wider = Grip::Right.stretch(from, (80.0, 0.0), SCREEN);
        assert_eq!(
            wider,
            Frame {
                x: 100.0,
                y: 70.0,
                width: 624.0,
                height: 479.0,
            }
        );
        let taller = Grip::Bottom.stretch(from, (0.0, -60.0), SCREEN);
        assert_eq!(
            taller,
            Frame {
                x: 60.0,
                y: 40.0,
                width: 624.0,
                height: 479.0,
            }
        );
    }

    #[test]
    fn a_corner_follows_the_farther_pull_from_the_opposite_corner() {
        let from = board(600.0, 100.0);
        let pulled = Grip::TopLeft.stretch(from, (-8.0, 300.0), SCREEN);
        assert_eq!(
            pulled,
            Frame {
                x: 200.0,
                y: 100.0,
                width: 944.0,
                height: 719.0,
            }
        );
    }

    #[test]
    fn neither_outgrows_the_screen_nor_leaves_it() {
        let from = board(900.0, 500.0);
        let huge = Grip::BottomLeft.stretch(from, (-5000.0, -5000.0), SCREEN);
        assert_eq!(Monitor::wide(huge.width), Monitor::fitting(SCREEN));
        assert!(is_on(huge, SCREEN));
        let tiny = Grip::TopRight.stretch(from, (-5000.0, -5000.0), SCREEN);
        assert_eq!(Monitor::wide(tiny.width), Monitor::smallest());
        assert!(is_on(tiny, SCREEN));
    }

    #[test]
    fn settles_a_remembered_frame_onto_the_screen() {
        let remembered = Frame {
            x: 1300.5,
            y: 300.0,
            width: 512.0,
            height: 384.0,
        };
        let settled = remembered.settle(SCREEN);
        assert_eq!(
            settled,
            Frame {
                x: 1000.0,
                y: 289.0,
                width: 512.0,
                height: 395.0,
            }
        );
        assert_eq!(settled.settle(SCREEN), settled);
    }
}
