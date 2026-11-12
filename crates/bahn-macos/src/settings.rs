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

use global_hotkey::hotkey::HotKey;
use objc2::rc::Retained;
use objc2_foundation::{NSUserDefaults, ns_string};

const SHORTCUT: &str = "Cmd+Shift+Option+]";

pub(crate) fn is_hidden() -> bool {
    defaults().boolForKey(ns_string!("Hidden"))
}

pub(crate) fn set_hidden(hidden: bool) {
    defaults().setBool_forKey(hidden, ns_string!("Hidden"));
}

pub(crate) fn shows_when_appointments_start() -> bool {
    let key = ns_string!("ShowWhenAppointmentsStart");
    let defaults = defaults();
    defaults
        .objectForKey(key)
        .is_none_or(|_| defaults.boolForKey(key))
}

pub(crate) fn set_showing_when_appointments_start(shows: bool) {
    defaults().setBool_forKey(shows, ns_string!("ShowWhenAppointmentsStart"));
}

pub(crate) fn shortcut() -> HotKey {
    defaults()
        .stringForKey(ns_string!("Shortcut"))
        .and_then(|custom| custom.to_string().parse().ok())
        .unwrap_or_else(|| SHORTCUT.parse().expect("the default shortcut should parse"))
}

fn defaults() -> Retained<NSUserDefaults> {
    NSUserDefaults::standardUserDefaults()
}
