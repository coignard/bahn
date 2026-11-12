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

use std::fs::{self, File};
use std::num::NonZeroU8;
use std::path::Path;
use std::time::{Duration, Instant};

use bahn::{Agenda, Appointment, Monitor, Pixmap, Tafel};
use jiff::civil::{Date, DateTime, date};
use jiff::{SignedDuration, ToSpan};
use serde::Serialize;

use crate::Failure;

const FRAME: Duration = Duration::from_millis(40);
const LEAD: SignedDuration = SignedDuration::from_millis(1200);
const MANIFEST: &str = "frames.json";
const SCRIPT: [(Step, Duration); 9] = [
    (Step::Wait, Duration::from_millis(5200)),
    (Step::Arrive, Duration::from_secs(3)),
    (Step::Depart, Duration::from_millis(3400)),
    (Step::Arrive, Duration::from_secs(3)),
    (Step::Depart, Duration::from_millis(3400)),
    (Step::Arrive, Duration::from_secs(3)),
    (Step::Depart, Duration::from_millis(3400)),
    (Step::Arrive, Duration::from_secs(3)),
    (Step::Depart, Duration::from_millis(3400)),
];

#[derive(Clone, Copy)]
enum Step {
    Wait,
    Arrive,
    Depart,
}

#[derive(Serialize)]
struct Frame {
    image: String,
    milliseconds: u64,
}

pub(crate) fn demo(scale: NonZeroU8, out: &Path) -> Result<(), Failure> {
    let frames = film(u32::from(scale.get()))?;
    fs::create_dir_all(out)?;
    let mut manifest = Vec::new();
    for (index, (pixmap, duration)) in frames.iter().enumerate() {
        let image = format!("{index:04}.png");
        pixmap.save_png(out.join(&image))?;
        manifest.push(Frame {
            image,
            milliseconds: u64::try_from(duration.as_millis())?,
        });
    }
    serde_json::to_writer_pretty(File::create(out.join(MANIFEST))?, &manifest)?;
    Ok(())
}

pub(crate) fn still(scale: u32) -> Result<Pixmap, Failure> {
    let (mut tafel, now, _) = opening()?;
    tafel
        .render(now, Instant::now(), Monitor::default(), scale)
        .ok_or_else(|| "the board should render".into())
}

fn film(scale: u32) -> Result<Vec<(Pixmap, Duration)>, Failure> {
    let (mut tafel, mut now, appointments) = opening()?;
    let start = Instant::now();
    let mut elapsed = Duration::ZERO;
    let mut frames: Vec<(Pixmap, Duration)> = Vec::new();
    for (step, hold) in SCRIPT {
        now = step.jump(&appointments, now);
        let end = elapsed + hold;
        while elapsed < end {
            let pixmap = tafel
                .render(now, start + elapsed, Monitor::default(), scale)
                .ok_or("the board should render")?;
            match frames.last_mut() {
                Some((last, duration)) if last.data() == pixmap.data() => *duration += FRAME,
                _ => frames.push((pixmap, FRAME)),
            }
            elapsed += FRAME;
            now = now.checked_add(FRAME)?;
        }
    }
    Ok(frames)
}

fn opening() -> Result<(Tafel, DateTime, Vec<Appointment>), Failure> {
    let day = date(2026, 11, 11);
    let appointments = schedule(day)?;
    let mut tafel = Tafel::default();
    tafel.plan(Agenda::Read(appointments.clone()));
    Ok((tafel, day.at(9, 24, 0, 0), appointments))
}

impl Step {
    fn jump(self, appointments: &[Appointment], now: DateTime) -> DateTime {
        let target = match self {
            Self::Wait => None,
            Self::Arrive => appointments
                .iter()
                .map(|appointment| appointment.start)
                .filter(|&start| start > now)
                .min(),
            Self::Depart => appointments
                .iter()
                .filter(|appointment| (appointment.start..appointment.end).contains(&now))
                .map(|appointment| appointment.end)
                .min(),
        };
        target.map_or(now, |target| target - LEAD)
    }
}

fn schedule(day: Date) -> Result<Vec<Appointment>, Failure> {
    let appointment = |days: i64, (hour, minute), minutes: i64, title: &str, location: &str| {
        let start = day.checked_add(days.days())?.at(hour, minute, 0, 0);
        Ok::<_, Failure>(Appointment {
            id: format!("{title} {days}"),
            title: title.to_owned(),
            location: location.to_owned(),
            start,
            end: start.checked_add(SignedDuration::from_mins(minutes))?,
        })
    };
    [
        appointment(0, (9, 30), 15, "Daily Stand-up", "Besprechungsraum Saale"),
        appointment(0, (10, 0), 60, "Design-Review", "Videokonferenz"),
        appointment(0, (11, 15), 30, "Code-Review mit Anton", "Raum 3.14"),
        appointment(0, (12, 0), 60, "Mittagspause", "Kantine, Erdgeschoss"),
        appointment(0, (13, 30), 30, "1:1 mit Daria", ""),
        appointment(0, (14, 15), 75, "Sprint Planning", "Großer Saal"),
        appointment(0, (16, 0), 45, "Retrospektive", "Raum 1.02"),
        appointment(0, (17, 30), 60, "Yoga", "Studio am Markt"),
        appointment(1, (9, 30), 15, "Daily Stand-up", ""),
        appointment(1, (11, 0), 60, "Kundentermin Leipzig", "Augustusplatz 9"),
        appointment(1, (15, 0), 30, "Zahnarzt", ""),
    ]
    .into_iter()
    .collect()
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn films_the_same_day_every_time() {
        let (one, other) = (film(1).unwrap(), film(1).unwrap());
        assert_eq!(one.len(), other.len());
        assert!(
            one.iter()
                .zip(&other)
                .all(|((one, first), (other, second))| {
                    first == second && one.data() == other.data()
                })
        );
        let total: Duration = one.iter().map(|(_, duration)| *duration).sum();
        assert_eq!(
            total,
            SCRIPT.iter().map(|&(_, hold)| hold).sum::<Duration>()
        );
    }

    #[test]
    fn the_still_is_the_first_frame_of_the_film() {
        let (first, _) = film(1).unwrap().into_iter().next().unwrap();
        assert_eq!(still(1).unwrap().data(), first.data());
    }

    #[test]
    fn writes_frames_with_their_durations() {
        let out = TempDir::new().unwrap();
        demo(NonZeroU8::MIN, out.path()).unwrap();
        let manifest: Vec<serde_json::Value> =
            serde_json::from_reader(File::open(out.path().join(MANIFEST)).unwrap()).unwrap();
        assert!(manifest.len() > 50);
        for frame in &manifest {
            assert!(out.path().join(frame["image"].as_str().unwrap()).exists());
            assert!(frame["milliseconds"].as_u64().unwrap() >= 40);
        }
    }
}
