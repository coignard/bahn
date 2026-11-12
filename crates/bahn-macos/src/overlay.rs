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

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use bahn::{Frame, Grip, Monitor, Tafel};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use jiff::Zoned;
use jiff::civil::DateTime;
use objc2::rc::Retained;
use objc2::{MainThreadMarker, sel};
use objc2_foundation::{NSDate, NSRunLoop, NSRunLoopCommonModes, NSTimer};

use crate::calendar::Calendar;
use crate::clock;
use crate::delegate::Delegate;
use crate::panel::Panel;
use crate::settings;

const REREAD: Duration = Duration::from_secs(5);
const STALL: Duration = Duration::from_millis(100);

pub(crate) struct Overlay {
    panel: Panel,
    calendar: Calendar,
    tafel: RefCell<Tafel>,
    read_at: Cell<Option<Instant>>,
    painted: Cell<Option<Painted>>,
    awaited: Cell<Option<DateTime>>,
    timer: Retained<NSTimer>,
    _hot_keys: Option<GlobalHotKeyManager>,
}

#[derive(Clone, Copy)]
struct Painted {
    monitor: Monitor,
    scale: u32,
    until: Instant,
}

impl Overlay {
    pub(crate) fn new(delegate: &Delegate) -> Self {
        let timer = unsafe {
            NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
                NSDate::distantFuture().timeIntervalSinceNow(),
                delegate,
                sel!(tick:),
                None,
                true,
            )
        };
        unsafe { NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes) };
        Self {
            panel: Panel::new(delegate),
            calendar: Calendar::new(),
            tafel: RefCell::default(),
            read_at: Cell::new(None),
            painted: Cell::new(None),
            awaited: Cell::new(None),
            timer,
            _hot_keys: listen(),
        }
    }

    pub(crate) fn launch(&self) {
        if settings::is_hidden() {
            self.tick();
        } else {
            self.toggle();
        }
    }

    pub(crate) fn toggle(&self) {
        if !self.panel.is_shown() {
            self.read_calendar(&clock::now());
        }
        self.slide(&mut self.tafel.borrow_mut(), Instant::now());
        self.tick();
    }

    pub(crate) fn show(&self) {
        if !self.panel.is_shown() {
            self.toggle();
        }
    }

    pub(crate) fn hide(&self) {
        if self.panel.is_shown() {
            self.toggle();
        }
    }

    pub(crate) fn switch_showing_when_appointments_start(&self) {
        settings::set_showing_when_appointments_start(!settings::shows_when_appointments_start());
        self.poke();
    }

    pub(crate) fn reread(&self) {
        self.read_at.set(None);
        self.poke();
    }

    pub(crate) fn poke(&self) {
        self.timer.setFireDate(&NSDate::now());
    }

    pub(crate) fn moved(&self) {
        self.panel.moved();
    }

    pub(crate) fn rescreen(&self) {
        self.panel.rescreen();
        self.poke();
    }

    pub(crate) fn rescue(&self) {
        self.panel.rescue();
        self.poke();
    }

    pub(crate) fn stretch(&self, grip: Grip, from: Frame, by: (f64, f64)) {
        if self.panel.stretch(grip, from, by) {
            self.tick();
        }
    }

    pub(crate) fn tick(&self) {
        let zoned = clock::now();
        let due = self
            .read_at
            .get()
            .is_none_or(|read| read.elapsed() >= REREAD);
        if due && !self.is_moving(Instant::now()) {
            self.read_calendar(&zoned);
        }
        let at = Instant::now();
        let now = zoned.datetime();
        let mut tafel = self.tafel.borrow_mut();
        let arrived = self.awaited.get().is_some_and(|start| start <= now);
        if arrived && !self.panel.is_shown() && settings::shows_when_appointments_start() {
            self.slide(&mut tafel, at);
        }
        self.awaited.set(tafel.next_start(now));
        let gliding = self.panel.glide(at);
        let shown = self.panel.is_shown();
        let (monitor, scale) = (self.panel.monitor(), self.panel.scale());
        let outdated = self.painted.get().is_none_or(|painted| {
            (painted.monitor, painted.scale) != (monitor, scale) || at >= painted.until
        });
        if shown && (outdated || tafel.is_moving(at)) {
            if let Some(pixmap) = tafel.render(now, at, monitor, scale) {
                self.panel.present(&pixmap);
            }
            let until = at + Tafel::until_change(now);
            self.painted.set(Some(Painted {
                monitor,
                scale,
                until,
            }));
        }
        let moving = gliding || (shown && tafel.is_moving(at));
        self.panel.animate(moving);
        let wake = if moving {
            Some(STALL)
        } else if shown {
            self.painted
                .get()
                .map(|painted| painted.until.saturating_duration_since(at))
        } else {
            self.awaited
                .get()
                .filter(|_| settings::shows_when_appointments_start())
                .and_then(|start| start.to_zoned(zoned.time_zone().clone()).ok())
                .and_then(|start| zoned.duration_until(&start).try_into().ok())
        };
        self.timer
            .setFireDate(&wake.map_or_else(NSDate::distantFuture, |wake| {
                NSDate::dateWithTimeIntervalSinceNow(wake.as_secs_f64())
            }));
    }

    fn is_moving(&self, at: Instant) -> bool {
        self.panel.is_sliding() || (self.panel.is_shown() && self.tafel.borrow().is_moving(at))
    }

    fn read_calendar(&self, now: &Zoned) {
        let agenda = self.calendar.agenda(now);
        self.tafel.borrow_mut().plan(agenda);
        self.read_at.set(Some(Instant::now()));
    }

    fn slide(&self, tafel: &mut Tafel, at: Instant) {
        if self.panel.is_away() {
            tafel.reset();
            self.painted.set(None);
        }
        self.panel.toggle(at);
        settings::set_hidden(!self.panel.is_shown());
    }
}

fn listen() -> Option<GlobalHotKeyManager> {
    let manager = GlobalHotKeyManager::new()
        .inspect_err(|error| eprintln!("Bahn cannot listen for shortcuts: {error}"))
        .ok()?;
    let shortcut = settings::shortcut();
    if let Err(error) = manager.register(shortcut) {
        eprintln!("Bahn cannot take {shortcut}: {error}");
    }
    GlobalHotKeyEvent::set_event_handler(Some(|event: GlobalHotKeyEvent| {
        if event.state() == HotKeyState::Pressed
            && let Some(mtm) = MainThreadMarker::new()
            && let Some(delegate) = Delegate::shared(mtm)
        {
            delegate.with(Overlay::toggle);
        }
    }));
    Some(manager)
}
