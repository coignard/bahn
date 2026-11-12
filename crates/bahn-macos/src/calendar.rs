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

use std::cell::Cell;
use std::ptr::NonNull;

use bahn::{Agenda, Appointment};
use block2::RcBlock;
use jiff::civil::DateTime;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp, Zoned};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_event_kit::{
    EKAuthorizationStatus, EKEntityType, EKEvent, EKEventStatus, EKEventStore,
    EKEventStoreChangedNotification, EKParticipantStatus,
};
use objc2_foundation::{
    NSBundle, NSDate, NSError, NSNotification, NSNotificationCenter, NSOperationQueue, ns_string,
};

use crate::delegate::Delegate;
use crate::overlay::Overlay;

const AHEAD: SignedDuration = SignedDuration::from_hours(7 * 24);

pub(crate) struct Calendar {
    store: Retained<EKEventStore>,
    opened: Cell<bool>,
}

impl Calendar {
    pub(crate) fn new() -> Self {
        let store = unsafe { EKEventStore::new() };
        let explained = NSBundle::mainBundle()
            .objectForInfoDictionaryKey(ns_string!("NSCalendarsFullAccessUsageDescription"))
            .is_some();
        if explained {
            let answered = RcBlock::new(|_granted: Bool, _error: *mut NSError| {
                let reread = RcBlock::new(reread);
                unsafe { NSOperationQueue::mainQueue().addOperationWithBlock(&reread) };
            });
            unsafe { store.requestFullAccessToEventsWithCompletion(RcBlock::as_ptr(&answered)) };
        }
        let changed = RcBlock::new(|_: NonNull<NSNotification>| reread());
        unsafe {
            NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
                Some(EKEventStoreChangedNotification),
                Some(&store),
                Some(&NSOperationQueue::mainQueue()),
                &changed,
            );
        }
        Self {
            store,
            opened: Cell::new(false),
        }
    }

    pub(crate) fn agenda(&self, now: &Zoned) -> Agenda {
        let status = unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Event) };
        if status != EKAuthorizationStatus::FullAccess {
            return Agenda::Denied;
        }
        if !self.opened.replace(true) {
            unsafe { self.store.reset() };
        }
        let from = now.timestamp();
        let events = unsafe {
            let predicate = self
                .store
                .predicateForEventsWithStartDate_endDate_calendars(
                    &date(from),
                    &date(from + AHEAD),
                    None,
                );
            self.store.eventsMatchingPredicate(&predicate)
        };
        Agenda::Read(
            events
                .iter()
                .filter_map(|event| appointment(&event, now.time_zone()))
                .collect(),
        )
    }
}

fn reread() {
    if let Some(mtm) = MainThreadMarker::new()
        && let Some(delegate) = Delegate::shared(mtm)
    {
        delegate.with(Overlay::reread);
    }
}

fn appointment(event: &EKEvent, zone: &TimeZone) -> Option<Appointment> {
    unsafe {
        if event.isAllDay() || event.status() == EKEventStatus::Canceled || declined(event) {
            return None;
        }
        let start = event.startDate();
        Some(Appointment {
            id: format!(
                "{}@{}",
                event.eventIdentifier()?,
                start.timeIntervalSince1970()
            ),
            title: event.title().to_string(),
            location: event
                .location()
                .map(|location| location.to_string())
                .unwrap_or_default(),
            start: civil(&start, zone)?,
            end: civil(&event.endDate(), zone)?,
        })
    }
}

fn declined(event: &EKEvent) -> bool {
    unsafe { event.attendees() }.is_some_and(|attendees| {
        attendees.iter().any(|attendee| unsafe {
            attendee.isCurrentUser()
                && attendee.participantStatus() == EKParticipantStatus::Declined
        })
    })
}

fn civil(date: &NSDate, zone: &TimeZone) -> Option<DateTime> {
    let since_epoch = SignedDuration::try_from_secs_f64(date.timeIntervalSince1970()).ok()?;
    let timestamp = Timestamp::from_duration(since_epoch).ok()?;
    Some(zone.to_datetime(timestamp))
}

fn date(timestamp: Timestamp) -> Retained<NSDate> {
    NSDate::dateWithTimeIntervalSince1970(timestamp.as_duration().as_secs_f64())
}
