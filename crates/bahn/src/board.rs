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

use jiff::civil::{Date, DateTime, DateTimeDifference, Weekday};
use jiff::{RoundMode, Unit};

const TERMINUS: &str = "Feierabend";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Appointment {
    pub id: String,
    pub title: String,
    pub location: String,
    pub start: DateTime,
    pub end: DateTime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Agenda {
    Denied,
    Read(Vec<Appointment>),
}

impl Default for Agenda {
    fn default() -> Self {
        Self::Read(Vec::new())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Board {
    pub(crate) id: Option<String>,
    pub(crate) destination: String,
    pub(crate) station: Station,
    pub(crate) heading: String,
    pub(crate) rows: Vec<Row>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Station {
    pub(crate) label: &'static str,
    pub(crate) time: Option<Time>,
    pub(crate) name: String,
    pub(crate) note: String,
    pub(crate) location: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Time {
    pub(crate) start: String,
    pub(crate) end: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Row {
    Stop { time: Time, name: String },
    Day(String),
}

impl Board {
    pub(crate) fn at(appointments: &[Appointment], now: DateTime) -> Self {
        let mut ahead: Vec<&Appointment> = appointments
            .iter()
            .filter(|appointment| appointment.end > now)
            .collect();
        ahead.sort_by_key(|appointment| appointment.start);
        let Some((&next, later)) = ahead.split_first() else {
            return Self::terminus();
        };
        let day = next.start.date();
        let (label, note) = if next.start <= now {
            (
                "Aktueller Halt",
                format!("Abfahrt in {}", until(now, next.end)),
            )
        } else if day == now.date() {
            (
                "Nächste Station",
                format!("noch {}", until(now, next.start)),
            )
        } else {
            ("Nächste Station", format!("am {}", weekday(day)))
        };
        let heading = match later.first() {
            None => String::new(),
            Some(first) if first.start.date() == day => "Weiterfahrt nach:".to_owned(),
            Some(first) => onward(first.start.date()),
        };
        let rows = later
            .chunk_by(|one, other| one.start.date() == other.start.date())
            .enumerate()
            .flat_map(|(index, stops)| {
                let day = (index > 0).then(|| Row::Day(onward(stops[0].start.date())));
                day.into_iter().chain(stops.iter().map(|&stop| Row::Stop {
                    time: Time::of(stop),
                    name: stop.title.clone(),
                }))
            })
            .collect();
        let last_of_day = ahead
            .iter()
            .rev()
            .find(|appointment| appointment.start.date() == day)
            .unwrap_or(&next);
        Self {
            id: Some(next.id.clone()),
            destination: last_of_day.title.clone(),
            station: Station::of(next, label, note),
            heading,
            rows,
        }
    }

    pub(crate) fn denied() -> Self {
        Self::standstill(
            "Störung",
            "Kein Kalenderzugriff",
            "Datenschutz & Sicherheit › Kalender",
        )
    }

    fn terminus() -> Self {
        Self::standstill("Endstation", TERMINUS, "Bitte alle aussteigen")
    }

    fn standstill(label: &'static str, name: &str, note: &str) -> Self {
        Self {
            id: None,
            destination: TERMINUS.to_owned(),
            station: Station {
                label,
                time: None,
                name: name.to_owned(),
                note: note.to_owned(),
                location: None,
            },
            heading: String::new(),
            rows: Vec::new(),
        }
    }
}

impl Station {
    fn of(appointment: &Appointment, label: &'static str, note: String) -> Self {
        Self {
            label,
            time: Some(Time::of(appointment)),
            name: appointment.title.clone(),
            note,
            location: appointment
                .location
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .map(str::to_owned),
        }
    }
}

impl Time {
    fn of(appointment: &Appointment) -> Self {
        Self {
            start: clock(appointment.start),
            end: clock(appointment.end),
        }
    }
}

fn clock(time: DateTime) -> String {
    time.strftime("%H:%M").to_string()
}

fn until(from: DateTime, to: DateTime) -> String {
    let span = to
        .since(
            DateTimeDifference::new(from)
                .largest(Unit::Hour)
                .smallest(Unit::Minute)
                .mode(RoundMode::Ceil),
        )
        .unwrap_or_default();
    match (span.get_hours(), span.get_minutes()) {
        (0, 1) => "1 Minute".to_owned(),
        (0, minutes) => format!("{minutes} Minuten"),
        (hours, 0) => format!("{hours} Std."),
        (hours, minutes) => format!("{hours} Std. {minutes} Min."),
    }
}

fn onward(date: Date) -> String {
    format!("Weiterfahrt am {} nach:", weekday(date))
}

fn weekday(date: Date) -> &'static str {
    match date.weekday() {
        Weekday::Monday => "Montag",
        Weekday::Tuesday => "Dienstag",
        Weekday::Wednesday => "Mittwoch",
        Weekday::Thursday => "Donnerstag",
        Weekday::Friday => "Freitag",
        Weekday::Saturday => "Samstag",
        Weekday::Sunday => "Sonntag",
    }
}

#[cfg(test)]
mod tests {
    use jiff::SignedDuration;
    use jiff::civil::date;

    use super::*;

    fn at(hour: i8, minute: i8) -> DateTime {
        date(2026, 11, 11).at(hour, minute, 0, 0)
    }

    fn appointment(title: &str, start: DateTime, minutes: i64) -> Appointment {
        Appointment {
            id: title.to_owned(),
            title: title.to_owned(),
            location: String::new(),
            start,
            end: start + SignedDuration::from_mins(minutes),
        }
    }

    fn stop(start: &str, end: &str, name: &str) -> Row {
        Row::Stop {
            time: Time {
                start: start.to_owned(),
                end: end.to_owned(),
            },
            name: name.to_owned(),
        }
    }

    fn week() -> Vec<Appointment> {
        let thursday = date(2026, 11, 12);
        vec![
            appointment("Zahnarzt", thursday.at(15, 0, 0, 0), 30),
            appointment("Stand-up", at(9, 30), 15),
            appointment("Review", at(10, 0), 60),
            appointment("Kunde", thursday.at(11, 0, 0, 0), 60),
            appointment("Yoga", at(17, 30), 60),
        ]
    }

    #[test]
    fn heads_for_the_next_appointment_while_nothing_runs() {
        let board = Board::at(&week(), at(9, 24));
        assert_eq!(board.id.as_deref(), Some("Stand-up"));
        assert_eq!(board.station.label, "Nächste Station");
        assert_eq!(board.station.note, "noch 6 Minuten");
        assert_eq!(board.destination, "Yoga");
    }

    #[test]
    fn halts_at_the_appointment_that_runs() {
        let board = Board::at(&week(), at(9, 35));
        assert_eq!(board.id.as_deref(), Some("Stand-up"));
        assert_eq!(board.station.label, "Aktueller Halt");
        assert_eq!(board.station.note, "Abfahrt in 10 Minuten");
    }

    #[test]
    fn rounds_waiting_up_to_whole_minutes() {
        assert_eq!(
            until(at(9, 0), at(9, 0) + SignedDuration::from_secs(30)),
            "1 Minute"
        );
        assert_eq!(until(at(9, 0), at(10, 0)), "1 Std.");
        assert_eq!(until(at(9, 0), at(11, 5)), "2 Std. 5 Min.");
    }

    #[test]
    fn continues_onto_the_next_day_under_its_own_heading() {
        let board = Board::at(&week(), at(9, 24));
        assert_eq!(board.heading, "Weiterfahrt nach:");
        assert_eq!(
            board.rows,
            [
                stop("10:00", "11:00", "Review"),
                stop("17:30", "18:30", "Yoga"),
                Row::Day("Weiterfahrt am Donnerstag nach:".to_owned()),
                stop("11:00", "12:00", "Kunde"),
                stop("15:00", "15:30", "Zahnarzt"),
            ]
        );
    }

    #[test]
    fn the_last_stop_of_the_day_heads_the_next_day() {
        let board = Board::at(&week(), at(17, 45));
        assert_eq!(board.heading, "Weiterfahrt am Donnerstag nach:");
        assert_eq!(
            board.rows,
            [
                stop("11:00", "12:00", "Kunde"),
                stop("15:00", "15:30", "Zahnarzt")
            ]
        );
        let overnight = Board::at(&week(), at(19, 0));
        assert_eq!(overnight.station.name, "Kunde");
        assert_eq!(overnight.station.note, "am Donnerstag");
    }

    #[test]
    fn shows_until_when_an_appointment_lasts_and_where() {
        let mut planning = appointment("Planning", at(14, 0), 75);
        planning.location = "\n  Großer Saal\nEtage 3".to_owned();
        let station = Board::at(&[planning], at(13, 0)).station;
        let time = station.time.unwrap();
        assert_eq!((time.start.as_str(), time.end.as_str()), ("14:00", "15:15"));
        assert_eq!(station.location.as_deref(), Some("Großer Saal"));
    }

    #[test]
    fn the_line_ends_when_the_calendar_runs_out() {
        let board = Board::at(&week(), date(2026, 11, 12).at(16, 0, 0, 0));
        assert_eq!(board.id, None);
        assert_eq!(board.station.label, "Endstation");
        assert!(board.rows.is_empty());
    }
}
