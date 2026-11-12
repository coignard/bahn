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

use jiff::civil::DateTime;
use tiny_skia::Pixmap;

use crate::board::{Agenda, Board, Row, Station};
use crate::canvas::{Canvas, HEIGHT, Rgb, Style, WIDTH};
use crate::font::Fonts;
use crate::monitor::{Bezel, Monitor};
use crate::motion::{Fade, ease, ease_out, progress};

const BLUE: Rgb = [17, 25, 132];
const WHITE: Rgb = [255, 255, 255];
const NAVY: Rgb = [18, 24, 96];
const YELLOW: Rgb = [244, 198, 88];
const GREEN: Rgb = [36, 168, 70];

const LINE: Style = Style::regular(51.0, WHITE);
const DESTINATION: Style = Style::regular(33.0, WHITE);
const CLOCK: Style = Style::regular(48.0, WHITE);
const LABEL: Style = Style::regular(34.0, NAVY);
const ARRIVAL: Style = Style::bold(47.0, NAVY);
const DEPARTURE: Style = Style::bold(47.0, GREEN);
const NAME: Style = Style::bold(46.0, NAVY);
const SMALLEST_NAME: f32 = 36.0;
const HINT: Style = Style::regular(35.0, NAVY);
const HEADING: Style = Style::regular(34.0, WHITE);
const STOP: Style = Style::regular(33.0, WHITE);
const STOP_DEPARTURE: Style = Style::regular(33.0, YELLOW);

const LEFT: f32 = 33.0;
const RIGHT: f32 = WIDTH - 38.0;
const DESTINATION_X: f32 = LEFT + 41.0;
const BAND_DEPARTURE: f32 = 185.0;
const LIST_DEPARTURE: f32 = 156.0;
const TRACK: f32 = 293.0;
const NAMES: f32 = 338.0;
const NAMES_WIDTH: f32 = RIGHT - NAMES;

const LINE_Y: f32 = 46.0;
const DESTINATION_Y: f32 = 99.0;
const CLOCK_Y: f32 = 42.0;
const BAND_TOP: f32 = 188.0;
const BAND_BOTTOM: f32 = 372.0;
const LABEL_Y: f32 = BAND_TOP + 32.0;
const NAME_Y: f32 = BAND_TOP + 89.0;
const HINT_Y: f32 = BAND_TOP + 152.0;
const HEADING_Y: f32 = BAND_BOTTOM + 39.0;
const ROLL_EDGE: f32 = BAND_BOTTOM + 64.0;
const FIRST_STOP_Y: f32 = BAND_BOTTOM + 93.0;
const LAST_STOP_Y: f32 = HEIGHT - 40.0;
const STOP_SPACING: f32 = 59.0;
const DAY_GAP: f32 = 26.0;

const POINTER: [(f32, f32); 3] = [
    (LEFT + 6.0, DESTINATION_Y - 5.25),
    (LEFT + 21.0, DESTINATION_Y),
    (LEFT + 6.0, DESTINATION_Y + 5.25),
];

const TRACK_WIDTH: f32 = 4.0;
const TRACK_FADE: f32 = 20.0;
const TRACK_BOTTOM: f32 = HEIGHT - 4.0;
const SQUARE: f32 = 28.0;
const BORDER: f32 = 3.5;

const ROLL: Duration = Duration::from_millis(1200);
const SWAP: f32 = 0.42;
const TRAVEL: f32 = 70.0;
const SECONDS_PER_HINT: u64 = 4;
const BLINK: u32 = 500_000_000;

#[derive(Default)]
pub struct Tafel {
    agenda: Agenda,
    fonts: Fonts,
    bezel: Bezel,
    shown: Option<Shown>,
}

impl Tafel {
    pub fn plan(&mut self, agenda: Agenda) {
        self.agenda = agenda;
    }

    pub fn reset(&mut self) {
        self.shown = None;
    }

    pub fn render(
        &mut self,
        now: DateTime,
        at: Instant,
        monitor: Monitor,
        scale: u32,
    ) -> Option<Pixmap> {
        let mut canvas = Canvas::new(&self.fonts, monitor, scale)?;
        let board = match &self.agenda {
            Agenda::Read(appointments) => Board::at(appointments, now),
            Agenda::Denied => Board::denied(),
        };
        let shown = self
            .shown
            .get_or_insert_with(|| Shown::new(board.clone(), at));
        shown.update(board, at);
        shown.paint(&mut canvas, now, at);
        let mut pixmap = canvas.into_pixmap();
        self.bezel.draw(&mut pixmap, monitor, scale);
        Some(pixmap)
    }

    pub fn is_moving(&self, at: Instant) -> bool {
        self.shown.as_ref().is_some_and(|shown| shown.is_moving(at))
    }

    pub fn until_change(now: DateTime) -> Duration {
        let blinked = now.subsec_nanosecond().unsigned_abs() % BLINK;
        Duration::from_nanos(u64::from(BLINK - blinked))
    }

    pub fn next_start(&self, after: DateTime) -> Option<DateTime> {
        let Agenda::Read(appointments) = &self.agenda else {
            return None;
        };
        appointments
            .iter()
            .map(|appointment| appointment.start)
            .filter(|&start| start > after)
            .min()
    }
}

struct Shown {
    board: Board,
    band: Band,
    leaving: Option<Leaving>,
    destination: Fade<String>,
    heading: Fade<String>,
}

struct Band {
    label: Fade<&'static str>,
    hint: Fade<String>,
    since: Instant,
}

struct Leaving {
    board: Board,
    band: Band,
    since: Instant,
}

#[derive(Clone, Copy)]
struct Slide {
    offset: f32,
    alpha: f32,
}

impl Slide {
    const STILL: Self = Self {
        offset: 0.0,
        alpha: 1.0,
    };
}

impl Shown {
    fn new(board: Board, at: Instant) -> Self {
        Self {
            band: Band::new(&board.station, at),
            leaving: None,
            destination: Fade::new(board.destination.clone(), at),
            heading: Fade::new(board.heading.clone(), at),
            board,
        }
    }

    fn update(&mut self, board: Board, at: Instant) {
        if board.id == self.board.id {
            self.board = board;
        } else {
            if self.board.rows.contains(&Row::Day(board.heading.clone())) {
                self.heading = Fade::new(board.heading.clone(), at);
            }
            self.leaving = Some(Leaving {
                band: mem::replace(&mut self.band, Band::new(&board.station, at)),
                board: mem::replace(&mut self.board, board),
                since: at,
            });
        }
        self.leaving = self
            .leaving
            .take()
            .filter(|leaving| progress(leaving.since, at, ROLL) < 1.0);
        self.band.update(&self.board.station, at);
        self.destination.set(self.board.destination.clone(), at);
        self.heading.set(self.board.heading.clone(), at);
    }

    fn is_moving(&self, at: Instant) -> bool {
        self.leaving.is_some()
            || self.band.is_moving(at)
            || self.destination.is_moving(at)
            || self.heading.is_moving(at)
    }

    fn paint(&self, canvas: &mut Canvas, now: DateTime, at: Instant) {
        canvas.clear(BLUE);
        self.paint_list(canvas, at);
        canvas.fill(0.0, BAND_TOP, WIDTH, BAND_BOTTOM - BAND_TOP, WHITE, 1.0);
        canvas.clip(Some(BAND_TOP..BAND_BOTTOM));
        if let Some(leaving) = &self.leaving {
            let roll = progress(leaving.since, at, ROLL);
            let out = (roll / SWAP).min(1.0);
            let into = ease_out((roll - SWAP) / (1.0 - SWAP));
            let gone = Slide {
                offset: -TRAVEL * ease(out),
                alpha: 1.0 - out,
            };
            let coming = Slide {
                offset: TRAVEL * (1.0 - into),
                alpha: into,
            };
            paint_band(canvas, &leaving.board.station, &leaving.band, at, gone);
            paint_band(canvas, &self.board.station, &self.band, at, coming);
        } else {
            paint_band(canvas, &self.board.station, &self.band, at, Slide::STILL);
        }
        canvas.clip(None);
        self.paint_header(canvas, now, at);
    }

    fn paint_header(&self, canvas: &mut Canvas, now: DateTime, at: Instant) {
        canvas.text("S 7", LINE, LEFT, LINE_Y, 1.0);
        for (destination, alpha) in self.destination.layers(at) {
            canvas.triangle(POINTER, WHITE, alpha);
            let destination = canvas.fit(destination, DESTINATION, RIGHT - DESTINATION_X);
            canvas.text(
                &destination,
                DESTINATION,
                DESTINATION_X,
                DESTINATION_Y,
                alpha,
            );
        }
        let colon = now.subsec_nanosecond().unsigned_abs() < BLINK;
        let clock = now
            .strftime(if colon { "%H:%M" } else { "%H %M" })
            .to_string();
        canvas.text(
            &clock,
            CLOCK,
            RIGHT - canvas.width(&clock, CLOCK),
            CLOCK_Y,
            1.0,
        );
    }

    fn paint_list(&self, canvas: &mut Canvas, at: Instant) {
        let stops = place(&self.board.rows);
        let (before, roll) = self.leaving.as_ref().map_or((Vec::new(), 1.0), |leaving| {
            let roll = ease(progress(leaving.since, at, ROLL));
            (place(&leaving.board.rows), roll)
        });
        let shift = stops
            .first()
            .and_then(|&(row, y)| {
                let &(_, was) = before.iter().find(|&&(old, _)| old == row)?;
                Some(was - y)
            })
            .unwrap_or(STOP_SPACING);
        let lift = shift * (1.0 - roll);
        let promoted = before
            .iter()
            .find(|(row, _)| matches!(row, Row::Day(day) if *day == self.board.heading));

        if let (Some(leaving), Some(&(day, y))) = (&self.leaving, promoted) {
            let fading = 1.0 - (2.5 * roll).min(1.0);
            canvas.text(&leaving.board.heading, HEADING, NAMES, HEADING_Y, fading);
            paint_row(canvas, day, y + (HEADING_Y - y) * roll, 1.0);
        } else {
            for (heading, alpha) in self.heading.layers(at) {
                canvas.text(heading, HEADING, NAMES, HEADING_Y, alpha);
            }
        }

        let reach = if stops.len() < self.board.rows.len() {
            Some((TRACK_BOTTOM, TRACK_FADE))
        } else {
            stops.last().map(|&(_, y)| (y + lift, 0.0))
        };
        if let Some((end, fade)) = reach {
            canvas.rule(
                TRACK,
                BAND_BOTTOM..end,
                TRACK_WIDTH,
                [TRACK_FADE, fade],
                WHITE,
            );
        }

        canvas.clip(Some(ROLL_EDGE..HEIGHT));
        for &(row, y) in &before {
            let stays = stops.iter().any(|&(stop, _)| stop == row)
                || promoted.is_some_and(|&(day, _)| day == row);
            if !stays {
                let fading = 1.0 - (1.8 * roll).min(1.0);
                paint_row(canvas, row, y - shift * roll, fading);
            }
        }
        for &(row, y) in &stops {
            let arriving = self.leaving.is_some() && !before.iter().any(|&(old, _)| old == row);
            paint_row(canvas, row, y + lift, if arriving { roll } else { 1.0 });
        }
        canvas.clip(None);
    }
}

impl Band {
    fn new(station: &Station, at: Instant) -> Self {
        Self {
            label: Fade::new(station.label, at),
            hint: Fade::new(station.note.clone(), at),
            since: at,
        }
    }

    fn update(&mut self, station: &Station, at: Instant) {
        let turn = at.saturating_duration_since(self.since).as_secs() / SECONDS_PER_HINT;
        let hint = match &station.location {
            Some(location) if turn % 2 == 1 => location,
            _ => &station.note,
        };
        self.label.set(station.label, at);
        self.hint.set(hint.clone(), at);
    }

    fn is_moving(&self, at: Instant) -> bool {
        self.label.is_moving(at) || self.hint.is_moving(at)
    }
}

fn place(rows: &[Row]) -> Vec<(&Row, f32)> {
    rows.iter()
        .scan(FIRST_STOP_Y - STOP_SPACING, |y, row| {
            *y += spacing(row);
            Some((row, *y))
        })
        .take_while(|&(_, y)| y <= LAST_STOP_Y)
        .collect()
}

fn spacing(row: &Row) -> f32 {
    match row {
        Row::Stop { .. } => STOP_SPACING,
        Row::Day(_) => STOP_SPACING + DAY_GAP,
    }
}

fn paint_band(canvas: &mut Canvas, station: &Station, band: &Band, at: Instant, slide: Slide) {
    let Slide { offset, alpha } = slide;
    if let Some(time) = &station.time {
        canvas.text("Geplant", LABEL, LEFT, LABEL_Y + offset, alpha);
        canvas.text("Abfahrt", LABEL, BAND_DEPARTURE, LABEL_Y + offset, alpha);
        canvas.text(&time.start, ARRIVAL, LEFT, NAME_Y + offset, alpha);
        canvas.text(&time.end, DEPARTURE, BAND_DEPARTURE, NAME_Y + offset, alpha);
    }
    for (label, fade) in band.label.layers(at) {
        canvas.text(label, LABEL, NAMES, LABEL_Y + offset, alpha * fade);
    }
    let shrunk = NAME.size * NAMES_WIDTH / canvas.width(&station.name, NAME);
    let style = NAME.sized(shrunk.clamp(SMALLEST_NAME, NAME.size));
    let name = canvas.fit(&station.name, style, NAMES_WIDTH);
    canvas.text(&name, style, NAMES, NAME_Y + offset, alpha);
    for (hint, fade) in band.hint.layers(at) {
        let hint = canvas.fit(hint, HINT, NAMES_WIDTH);
        canvas.text(&hint, HINT, NAMES, HINT_Y + offset, alpha * fade);
    }
}

fn paint_row(canvas: &mut Canvas, row: &Row, y: f32, alpha: f32) {
    match row {
        Row::Day(day) => canvas.text(day, HEADING, NAMES, y, alpha),
        Row::Stop { time, name } => {
            canvas.text(&time.start, STOP, LEFT, y, alpha);
            canvas.text(&time.end, STOP_DEPARTURE, LIST_DEPARTURE, y, alpha);
            canvas.square(TRACK, y, SQUARE - 2.0 * BORDER, BLUE, alpha);
            canvas.ring(TRACK, y, SQUARE, BORDER, WHITE, alpha);
            let name = canvas.fit(name, STOP, NAMES_WIDTH);
            canvas.text(&name, STOP, NAMES, y, alpha);
        }
    }
}

#[cfg(test)]
mod tests {
    use jiff::SignedDuration;
    use jiff::civil::date;

    use super::*;
    use crate::board::Appointment;

    fn appointment(title: &str, start: DateTime, minutes: i64) -> Appointment {
        Appointment {
            id: title.to_owned(),
            title: title.to_owned(),
            location: String::new(),
            start,
            end: start + SignedDuration::from_mins(minutes),
        }
    }

    fn morning() -> Tafel {
        let day = date(2026, 11, 11);
        let mut tafel = Tafel::default();
        tafel.plan(Agenda::Read(vec![
            appointment("Stand-up", day.at(9, 30, 0, 0), 15),
            appointment("Review", day.at(10, 0, 0, 0), 60),
        ]));
        tafel
    }

    fn screen_color(pixmap: &Pixmap, scale: u32, x: u32, y: u32) -> Rgb {
        let pixel = pixmap.pixel((16 + x) * scale, (17 + y) * scale).unwrap();
        [pixel.red(), pixel.green(), pixel.blue()]
    }

    #[test]
    fn paints_a_white_band_between_blue_halves_behind_the_bezel() {
        let monitor = Monitor::wide(1056.0);
        let pixmap = morning()
            .render(
                date(2026, 11, 11).at(9, 0, 0, 0),
                Instant::now(),
                monitor,
                1,
            )
            .unwrap();
        assert_eq!(screen_color(&pixmap, 1, 512, 10), BLUE);
        assert_eq!(screen_color(&pixmap, 1, 1010, 200), WHITE);
        assert_eq!(screen_color(&pixmap, 1, 512, 760), BLUE);
        assert_eq!(pixmap.pixel(0, 0).unwrap().alpha(), 0);
        assert_eq!(pixmap.pixel(8, 300).unwrap().red(), 192);
    }

    #[test]
    fn the_track_fades_in_below_the_band_at_any_scale() {
        let mut tafel = morning();
        let monitor = Monitor::wide(1056.0);
        for scale in [1, 2] {
            let pixmap = tafel
                .render(
                    date(2026, 11, 11).at(9, 0, 0, 0),
                    Instant::now(),
                    monitor,
                    scale,
                )
                .unwrap();
            let red = |y| screen_color(&pixmap, scale, 293, y)[0];
            assert!(red(374) < red(382) && red(382) < red(390));
            assert_eq!(screen_color(&pixmap, scale, 293, 420), WHITE);
        }
    }

    #[test]
    fn rolls_on_when_an_appointment_ends_and_then_rests() {
        let day = date(2026, 11, 11);
        let mut tafel = morning();
        let monitor = Monitor::default();
        let start = Instant::now();
        let halting = day.at(9, 44, 59, 0);
        tafel.render(halting, start, monitor, 1);
        assert!(!tafel.is_moving(start));
        assert_eq!(Tafel::until_change(halting), Duration::from_millis(500));
        assert_eq!(tafel.next_start(halting), Some(day.at(10, 0, 0, 0)));
        let rolling = start + Duration::from_secs(1);
        tafel.render(day.at(9, 45, 0, 0), rolling, monitor, 1);
        assert!(tafel.is_moving(rolling));
        tafel.render(day.at(9, 45, 0, 0), rolling + ROLL, monitor, 1);
        assert!(!tafel.is_moving(rolling + ROLL));
    }

    #[test]
    fn a_reset_board_comes_back_showing_the_present_at_rest() {
        let day = date(2026, 11, 11);
        let mut tafel = morning();
        let monitor = Monitor::default();
        let start = Instant::now();
        tafel.render(day.at(9, 0, 0, 0), start, monitor, 1);
        tafel.reset();
        let later = start + Duration::from_hours(1);
        tafel.render(day.at(9, 50, 0, 0), later, monitor, 1);
        assert!(!tafel.is_moving(later));
        assert_eq!(
            tafel
                .shown
                .as_ref()
                .map(|shown| shown.board.station.name.as_str()),
            Some("Review")
        );
    }

    #[test]
    fn a_closed_calendar_halts_the_line() {
        let mut tafel = Tafel::default();
        tafel.plan(Agenda::Denied);
        let now = date(2026, 11, 11).at(9, 0, 0, 0);
        assert!(
            tafel
                .render(now, Instant::now(), Monitor::default(), 1)
                .is_some()
        );
        assert_eq!(tafel.next_start(now), None);
        assert_eq!(
            tafel.shown.as_ref().map(|shown| shown.board.station.label),
            Some("Störung")
        );
    }
}
