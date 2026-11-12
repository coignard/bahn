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

use std::cell::OnceCell;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationDelegate, NSControlStateValueOff, NSControlStateValueOn, NSMenu,
    NSMenuItem, NSWindowDelegate, NSWorkspace, NSWorkspaceDidWakeNotification,
};
use objc2_foundation::{
    NSNotification, NSNotificationCenter, NSNotificationName, NSObject, NSObjectProtocol, NSString,
    NSSystemClockDidChangeNotification, NSSystemTimeZoneDidChangeNotification, ns_string,
};

use crate::clock;
use crate::overlay::Overlay;
use crate::settings;

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = OnceCell<Overlay>]
    pub(crate) struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            self.watch_the_clock();
            self.ivars().get_or_init(|| Overlay::new(self)).launch();
        }

        #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]
        fn should_handle_reopen(&self, _application: &NSApplication, _visible: bool) -> bool {
            self.with(Overlay::show);
            false
        }

        #[unsafe(method(applicationDidChangeScreenParameters:))]
        fn did_change_screen_parameters(&self, _notification: &NSNotification) {
            self.with(Overlay::rescue);
        }
    }

    unsafe impl NSWindowDelegate for Delegate {
        #[unsafe(method(windowDidMove:))]
        fn window_did_move(&self, _notification: &NSNotification) {
            self.with(Overlay::moved);
        }

        #[unsafe(method(windowDidChangeScreen:))]
        fn window_did_change_screen(&self, _notification: &NSNotification) {
            self.with(Overlay::rescreen);
        }

        #[unsafe(method(windowDidChangeBackingProperties:))]
        fn window_did_change_backing_properties(&self, _notification: &NSNotification) {
            self.with(Overlay::poke);
        }
    }

    impl Delegate {
        #[unsafe(method(tick:))]
        fn tick(&self, _sender: &AnyObject) {
            self.with(Overlay::tick);
        }

        #[unsafe(method(clockDidChange:))]
        fn clock_did_change(&self, _notification: &NSNotification) {
            clock::forget_time_zone();
            self.with(Overlay::reread);
        }

        #[unsafe(method(hide:))]
        fn hide(&self, _sender: &AnyObject) {
            self.with(Overlay::hide);
        }

        #[unsafe(method(switchShowingWhenAppointmentsStart:))]
        fn switch_showing_when_appointments_start(&self, _sender: &AnyObject) {
            self.with(Overlay::switch_showing_when_appointments_start);
        }
    }
);

impl Delegate {
    pub(crate) fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(OnceCell::new());
        unsafe { msg_send![super(this), init] }
    }

    pub(crate) fn shared(mtm: MainThreadMarker) -> Option<Retained<Self>> {
        let delegate = NSApplication::sharedApplication(mtm).delegate()?;
        let object: &AnyObject = delegate.as_ref();
        object.downcast_ref::<Self>().map(Message::retain)
    }

    pub(crate) fn with(&self, act: impl FnOnce(&Overlay)) {
        if let Some(overlay) = self.ivars().get() {
            act(overlay);
        }
    }

    pub(crate) fn menu(&self) -> Retained<NSMenu> {
        let mtm = self.mtm();
        let menu = NSMenu::new(mtm);
        let arrivals = item(
            mtm,
            ns_string!("Show When Appointments Start"),
            sel!(switchShowingWhenAppointmentsStart:),
            self,
        );
        arrivals.setState(if settings::shows_when_appointments_start() {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        menu.addItem(&arrivals);
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        menu.addItem(&item(mtm, ns_string!("Hide"), sel!(hide:), self));
        menu.addItem(&item(
            mtm,
            ns_string!("Quit Bahn"),
            sel!(terminate:),
            &NSApplication::sharedApplication(mtm),
        ));
        menu
    }

    fn watch_the_clock(&self) {
        let workspace = NSWorkspace::sharedWorkspace().notificationCenter();
        let system = NSNotificationCenter::defaultCenter();
        for (center, name) in [
            (&workspace, unsafe { NSWorkspaceDidWakeNotification }),
            (&system, unsafe { NSSystemClockDidChangeNotification }),
            (&system, unsafe { NSSystemTimeZoneDidChangeNotification }),
        ] {
            self.observe(center, name, sel!(clockDidChange:));
        }
    }

    fn observe(&self, center: &NSNotificationCenter, name: &NSNotificationName, selector: Sel) {
        unsafe { center.addObserver_selector_name_object(self, selector, Some(name), None) };
    }
}

fn item(
    mtm: MainThreadMarker,
    title: &NSString,
    action: Sel,
    target: &AnyObject,
) -> Retained<NSMenuItem> {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            title,
            Some(action),
            ns_string!(""),
        )
    };
    unsafe { item.setTarget(Some(target)) };
    item
}
