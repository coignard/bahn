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

use std::mem;
use std::time::{Duration, Instant};

const CROSSFADE: Duration = Duration::from_millis(600);
const GLIDE: Duration = Duration::from_millis(250);

pub(crate) fn progress(since: Instant, at: Instant, duration: Duration) -> f32 {
    at.saturating_duration_since(since)
        .div_duration_f32(duration)
        .min(1.0)
}

pub(crate) fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (2.0 - 2.0 * t).powi(3) / 2.0
    }
}

pub(crate) fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

pub(crate) struct Fade<T> {
    from: Option<T>,
    to: T,
    since: Instant,
}

impl<T: PartialEq> Fade<T> {
    pub(crate) fn new(value: T, at: Instant) -> Self {
        Self {
            from: None,
            to: value,
            since: at,
        }
    }

    pub(crate) fn set(&mut self, value: T, at: Instant) {
        if value != self.to {
            self.from = Some(mem::replace(&mut self.to, value));
            self.since = at;
        }
    }

    pub(crate) fn layers(&self, at: Instant) -> impl Iterator<Item = (&T, f32)> {
        let half = 2.0 * progress(self.since, at, CROSSFADE);
        let fading = self.from.as_ref().map(|from| (from, 1.0 - ease(half)));
        let appearing = if fading.is_some() {
            ease(half - 1.0)
        } else {
            1.0
        };
        fading
            .into_iter()
            .chain([(&self.to, appearing)])
            .filter(|&(_, alpha)| alpha > 0.0)
    }

    pub(crate) fn is_moving(&self, at: Instant) -> bool {
        self.from.is_some() && progress(self.since, at, CROSSFADE) < 1.0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Glide {
    from: f64,
    to: f64,
    since: Instant,
}

impl Glide {
    pub fn new(from: f64, to: f64, at: Instant) -> Self {
        Self {
            from,
            to,
            since: at,
        }
    }

    pub fn position(&self, at: Instant) -> f64 {
        let eased = f64::from(ease(progress(self.since, at, GLIDE)));
        self.from + (self.to - self.from) * eased
    }

    pub fn is_over(&self, at: Instant) -> bool {
        progress(self.since, at, GLIDE) >= 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_old_value_fades_out_before_the_new_one_fades_in() {
        let start = Instant::now();
        let mut fade = Fade::new("alt", start);
        fade.set("neu", start);
        let alphas = |after: Duration| -> Vec<(&str, f32)> {
            fade.layers(start + after)
                .map(|(&text, alpha)| (text, alpha))
                .collect()
        };
        assert_eq!(alphas(Duration::ZERO), [("alt", 1.0)]);
        assert_eq!(alphas(CROSSFADE / 2), Vec::<(&str, f32)>::new());
        assert_eq!(alphas(CROSSFADE), [("neu", 1.0)]);
        assert!(fade.is_moving(start + CROSSFADE / 4));
        assert!(!fade.is_moving(start + CROSSFADE));
    }

    #[test]
    fn a_glide_speeds_up_and_slows_down_on_its_way() {
        let start = Instant::now();
        let glide = Glide::new(0.0, 100.0, start);
        let at = |part: u32| glide.position(start + GLIDE * part / 4);
        assert!(at(0).abs() < f64::EPSILON);
        assert!(at(1) < 25.0 && at(3) > 75.0);
        assert!((at(2) - 50.0).abs() < 1e-3);
        assert!(!glide.is_over(start + GLIDE / 2) && glide.is_over(start + GLIDE));
    }
}
