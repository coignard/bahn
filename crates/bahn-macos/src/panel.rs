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
use std::time::Instant;

use bahn::{Frame, Glide, Grip, Monitor, Pixmap};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSFloatingWindowLevel, NSPanel, NSScreen,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_core_graphics::CGRectContainsPoint;
use objc2_foundation::{NSPoint, NSRect, NSRunLoop, NSRunLoopCommonModes, NSSize, ns_string};
use objc2_quartz_core::{CADisplayLink, CATransaction};

use crate::delegate::Delegate;
use crate::view::BoardView;

const MARGIN: f64 = 16.0;

pub(crate) struct Panel {
    window: Retained<NSPanel>,
    view: Retained<BoardView>,
    link: RefCell<Retained<CADisplayLink>>,
    home: Cell<Frame>,
    shown: Cell<bool>,
    slide: Cell<Option<Slide>>,
}

#[derive(Clone, Copy)]
struct Slide {
    stage: Frame,
    glide: Glide,
}

impl Panel {
    pub(crate) fn new(delegate: &Delegate) -> Self {
        let mtm = delegate.mtm();
        let window = NSPanel::initWithContentRect_styleMask_backing_defer(
            NSPanel::alloc(mtm),
            NSRect::ZERO,
            NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
            NSBackingStoreType::Buffered,
            false,
        );
        window.setOpaque(false);
        window.setBackgroundColor(Some(&NSColor::clearColor()));
        window.setHasShadow(false);
        window.setLevel(NSFloatingWindowLevel);
        window.setHidesOnDeactivate(false);
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        unsafe { window.setReleasedWhenClosed(false) };
        let view = BoardView::new(mtm);
        window.setContentView(Some(&view));
        window.setDelegate(Some(ProtocolObject::from_ref(delegate)));
        let home = if window.setFrameUsingName_force(ns_string!("Bahn"), true) {
            placed(frame(window.frame()), mtm)
        } else {
            corner(Monitor::default(), mtm)
        };
        window.setFrame_display(rect(home), false);
        let link = link(screen_of(home, mtm).as_deref(), &view, delegate);
        Self {
            window,
            view,
            link: RefCell::new(link),
            home: Cell::new(home),
            shown: Cell::new(false),
            slide: Cell::new(None),
        }
    }

    pub(crate) fn is_shown(&self) -> bool {
        self.shown.get()
    }

    pub(crate) fn is_away(&self) -> bool {
        !self.window.isVisible()
    }

    pub(crate) fn is_sliding(&self) -> bool {
        self.slide.get().is_some()
    }

    pub(crate) fn monitor(&self) -> Monitor {
        Monitor::wide(self.home.get().width)
    }

    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub(crate) fn scale(&self) -> u32 {
        let factor = screen_of(self.home.get(), self.window.mtm()).map_or_else(
            || self.window.backingScaleFactor(),
            |screen| screen.backingScaleFactor(),
        );
        factor.round().max(1.0) as u32
    }

    pub(crate) fn present(&self, pixmap: &Pixmap) {
        self.view.present(pixmap, self.monitor());
    }

    pub(crate) fn animate(&self, moving: bool) {
        self.link.borrow().setPaused(!moving);
    }

    pub(crate) fn toggle(&self, at: Instant) {
        let slide = self.slide.get();
        let stage = slide.map_or_else(|| self.stage(), |slide| slide.stage);
        let from = match slide {
            Some(slide) => slide.glide.position(at),
            None if self.is_away() => stage.width,
            None => 0.0,
        };
        let showing = !self.shown.get();
        let to = if showing { 0.0 } else { stage.width };
        self.shown.set(showing);
        self.slide.set(Some(Slide {
            stage,
            glide: Glide::new(from, to, at),
        }));
        self.view.let_go();
        self.window.setIgnoresMouseEvents(true);
        self.window.setFrame_display(rect(stage), false);
        self.view.place(from);
        if self.is_away() {
            CATransaction::flush();
            self.window.orderFrontRegardless();
        }
    }

    pub(crate) fn glide(&self, at: Instant) -> bool {
        let Some(slide) = self.slide.get() else {
            return false;
        };
        self.view.place(slide.glide.position(at));
        if !slide.glide.is_over(at) {
            return true;
        }
        if self.shown.get() {
            self.window.setFrame_display(rect(self.home.get()), false);
            self.window.setIgnoresMouseEvents(false);
        } else {
            self.window.orderOut(None);
        }
        self.slide.set(None);
        false
    }

    pub(crate) fn moved(&self) {
        if self.is_resting() {
            self.home.set(frame(self.window.frame()));
            self.remember();
        }
    }

    pub(crate) fn stretch(&self, grip: Grip, from: Frame, by: (f64, f64)) -> bool {
        let Some(screen) = self.window.screen().filter(|_| self.is_resting()) else {
            return false;
        };
        let stretched = grip.stretch(from, by, frame(screen.frame()));
        if stretched == self.home.get() {
            return false;
        }
        self.home.set(stretched);
        self.window.setFrame_display(rect(stretched), false);
        self.remember();
        true
    }

    pub(crate) fn rescreen(&self) {
        self.follow(self.window.screen().as_deref());
    }

    pub(crate) fn rescue(&self) {
        if self.is_sliding() {
            return;
        }
        let home = placed(self.home.get(), self.window.mtm());
        self.home.set(home);
        if self.shown.get() {
            self.window.setFrame_display(rect(home), false);
            self.remember();
        }
        self.follow(screen_of(home, self.window.mtm()).as_deref());
    }

    fn is_resting(&self) -> bool {
        self.shown.get() && !self.is_sliding()
    }

    fn stage(&self) -> Frame {
        let mtm = self.window.mtm();
        if self.is_away() {
            self.home.set(placed(self.home.get(), mtm));
        }
        let home = self.home.get();
        let screen = screen_of(home, mtm);
        self.follow(screen.as_deref());
        let edge = screen.map_or(home.x + home.width, |screen| screen.frame().max().x);
        Frame {
            width: (edge - home.x).max(home.width),
            ..home
        }
    }

    fn follow(&self, screen: Option<&NSScreen>) {
        let Some(target) = Delegate::shared(self.window.mtm()) else {
            return;
        };
        let link = link(screen, &self.view, &target);
        let old = self.link.replace(link);
        self.link.borrow().setPaused(old.isPaused());
        old.invalidate();
    }

    fn remember(&self) {
        self.window.saveFrameUsingName(ns_string!("Bahn"));
    }
}

pub(crate) fn frame(rect: NSRect) -> Frame {
    Frame {
        x: rect.origin.x,
        y: rect.origin.y,
        width: rect.size.width,
        height: rect.size.height,
    }
}

fn rect(frame: Frame) -> NSRect {
    NSRect::new(
        NSPoint::new(frame.x, frame.y),
        NSSize::new(frame.width, frame.height),
    )
}

fn placed(home: Frame, mtm: MainThreadMarker) -> Frame {
    screen_of(home, mtm).map_or_else(
        || corner(Monitor::wide(home.width), mtm),
        |screen| home.settle(frame(screen.frame())),
    )
}

fn corner(monitor: Monitor, mtm: MainThreadMarker) -> Frame {
    let (width, height) = (f64::from(monitor.width()), f64::from(monitor.height()));
    let Some(screen) = NSScreen::mainScreen(mtm) else {
        return Frame {
            x: 0.0,
            y: 0.0,
            width,
            height,
        };
    };
    let top_right = screen.visibleFrame().max();
    Frame {
        x: top_right.x - width - MARGIN,
        y: top_right.y - height - MARGIN,
        width,
        height,
    }
    .settle(frame(screen.frame()))
}

fn screen_of(frame: Frame, mtm: MainThreadMarker) -> Option<Retained<NSScreen>> {
    let middle = NSPoint::new(frame.x + frame.width / 2.0, frame.y + frame.height / 2.0);
    NSScreen::screens(mtm)
        .iter()
        .find(|screen| CGRectContainsPoint(screen.frame(), middle))
}

fn link(screen: Option<&NSScreen>, view: &BoardView, target: &Delegate) -> Retained<CADisplayLink> {
    let link = match screen {
        Some(screen) => unsafe { screen.displayLinkWithTarget_selector(target, sel!(tick:)) },
        None => unsafe { view.displayLinkWithTarget_selector(target, sel!(tick:)) },
    };
    link.setPaused(true);
    unsafe { link.addToRunLoop_forMode(&NSRunLoop::mainRunLoop(), NSRunLoopCommonModes) };
    link
}
