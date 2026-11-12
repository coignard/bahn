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
use std::ptr;

use bahn::{Frame, Grip, Monitor, Pixmap};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSCursor, NSCursorFrameResizeDirections, NSCursorFrameResizePosition, NSEvent, NSMenu,
    NSResponder, NSTrackingArea, NSTrackingAreaOptions, NSView,
};
use objc2_core_foundation::{CFData, CFRetained, CFString, CFType, CGSize, kCFBooleanTrue};
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo,
    kCGColorSpaceSRGB,
};
use objc2_foundation::{NSObject, NSPoint, NSRect};
use objc2_quartz_core::{CALayer, CATransaction, kCAFilterNearest};

use crate::delegate::Delegate;
use crate::panel;

const BYTES_PER_PIXEL: usize = size_of::<[u8; 4]>();
const BITS_PER_COMPONENT: usize = u8::BITS as usize;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    #[link_name = "CGSMainConnectionID"]
    fn main_connection() -> i32;
    #[link_name = "CGSSetConnectionProperty"]
    fn set_connection_property(owner: i32, connection: i32, key: &CFString, value: &CFType) -> i32;
}

pub(crate) struct Ivars {
    board: Retained<CALayer>,
    color_space: Option<CFRetained<CGColorSpace>>,
    pointing: Cell<Option<Grip>>,
    stretch: Cell<Option<Stretch>>,
}

#[derive(Clone, Copy)]
struct Stretch {
    grip: Grip,
    from: Frame,
    pointer: NSPoint,
}

define_class!(
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    pub(crate) struct BoardView;

    impl BoardView {
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(mouseDownCanMoveWindow))]
        fn mouse_down_can_move_window(&self) -> bool {
            false
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let Some(window) = self.window() else {
                return;
            };
            match self.grip(event) {
                Some(grip) => self.ivars().stretch.set(Some(Stretch {
                    grip,
                    from: panel::frame(window.frame()),
                    pointer: NSEvent::mouseLocation(),
                })),
                None => window.performWindowDragWithEvent(event),
            }
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, _event: &NSEvent) {
            let Some(stretch) = self.ivars().stretch.get() else {
                return;
            };
            let pointer = NSEvent::mouseLocation();
            let by = (pointer.x - stretch.pointer.x, pointer.y - stretch.pointer.y);
            if let Some(delegate) = Delegate::shared(self.mtm()) {
                delegate.with(|overlay| overlay.stretch(stretch.grip, stretch.from, by));
            }
            self.point(Some(stretch.grip));
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            self.ivars().stretch.set(None);
            self.point(self.grip(event));
        }

        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, event: &NSEvent) {
            self.hover(event);
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            self.hover(event);
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            if self.ivars().stretch.get().is_none() {
                self.let_go();
            }
        }

        #[unsafe(method_id(menuForEvent:))]
        fn menu_for_event(&self, _event: &NSEvent) -> Option<Retained<NSMenu>> {
            Delegate::shared(self.mtm()).map(|delegate| delegate.menu())
        }
    }
);

impl BoardView {
    pub(crate) fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let board = CALayer::new();
        board.setMagnificationFilter(unsafe { kCAFilterNearest });
        board.setMinificationFilter(unsafe { kCAFilterNearest });
        let this = Self::alloc(mtm).set_ivars(Ivars {
            board,
            color_space: CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB })),
            pointing: Cell::new(None),
            stretch: Cell::new(None),
        });
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: NSRect::ZERO] };
        let stage = CALayer::new();
        stage.addSublayer(&view.ivars().board);
        view.setLayer(Some(&stage));
        view.setWantsLayer(true);
        let owner: &AnyObject = &view;
        let area = unsafe {
            NSTrackingArea::initWithRect_options_owner_userInfo(
                NSTrackingArea::alloc(),
                NSRect::ZERO,
                NSTrackingAreaOptions::MouseEnteredAndExited
                    | NSTrackingAreaOptions::MouseMoved
                    | NSTrackingAreaOptions::ActiveAlways
                    | NSTrackingAreaOptions::InVisibleRect,
                Some(owner),
                None,
            )
        };
        view.addTrackingArea(&area);
        let_cursors_change_in_background();
        view
    }

    pub(crate) fn let_go(&self) {
        self.ivars().stretch.set(None);
        if self.ivars().pointing.take().is_some() {
            NSCursor::arrowCursor().set();
        }
    }

    pub(crate) fn place(&self, offset: f64) {
        let board = &self.ivars().board;
        let mut frame = board.frame();
        frame.origin.x = offset;
        still(|| board.setFrame(frame));
    }

    pub(crate) fn present(&self, pixmap: &Pixmap, monitor: Monitor) {
        let Some(image) = self.image(pixmap) else {
            return;
        };
        let board = &self.ivars().board;
        let mut frame = board.frame();
        frame.size = CGSize::new(f64::from(monitor.width()), f64::from(monitor.height()));
        let contents: &CFType = &image;
        still(|| {
            board.setFrame(frame);
            unsafe { board.setContents(Some(contents.as_ref())) };
        });
    }

    fn monitor(&self) -> Monitor {
        Monitor::wide(self.bounds().size.width)
    }

    fn grip(&self, event: &NSEvent) -> Option<Grip> {
        let point = self.convertPoint_fromView(event.locationInWindow(), None);
        Grip::at(point.x, point.y, self.monitor())
    }

    fn hover(&self, event: &NSEvent) {
        if self.ivars().stretch.get().is_none() {
            self.point(self.grip(event));
        }
    }

    fn point(&self, grip: Option<Grip>) {
        self.ivars().pointing.set(grip);
        grip.map_or_else(NSCursor::arrowCursor, |grip| self.resize_cursor(grip))
            .set();
    }

    fn resize_cursor(&self, grip: Grip) -> Retained<NSCursor> {
        let monitor = self.monitor();
        let largest = self
            .window()
            .and_then(|window| window.screen())
            .map(|screen| Monitor::fitting(panel::frame(screen.frame())));
        let directions = if monitor <= Monitor::smallest() {
            NSCursorFrameResizeDirections::Outward
        } else if largest.is_some_and(|largest| monitor >= largest) {
            NSCursorFrameResizeDirections::Inward
        } else {
            NSCursorFrameResizeDirections::All
        };
        NSCursor::frameResizeCursorFromPosition_inDirections(position(grip), directions)
    }

    fn image(&self, pixmap: &Pixmap) -> Option<CFRetained<CGImage>> {
        let width = usize::try_from(pixmap.width()).ok()?;
        let height = usize::try_from(pixmap.height()).ok()?;
        let data = CFData::from_bytes(pixmap.data());
        let provider = CGDataProvider::with_cf_data(Some(&data))?;
        unsafe {
            CGImage::new(
                width,
                height,
                BITS_PER_COMPONENT,
                BITS_PER_COMPONENT * BYTES_PER_PIXEL,
                width * BYTES_PER_PIXEL,
                self.ivars().color_space.as_deref(),
                CGBitmapInfo(CGImageAlphaInfo::PremultipliedLast.0),
                Some(&provider),
                ptr::null(),
                false,
                CGColorRenderingIntent::RenderingIntentDefault,
            )
        }
    }
}

fn position(grip: Grip) -> NSCursorFrameResizePosition {
    match grip {
        Grip::Left => NSCursorFrameResizePosition::Left,
        Grip::Right => NSCursorFrameResizePosition::Right,
        Grip::Bottom => NSCursorFrameResizePosition::Bottom,
        Grip::Top => NSCursorFrameResizePosition::Top,
        Grip::BottomLeft => NSCursorFrameResizePosition::BottomLeft,
        Grip::BottomRight => NSCursorFrameResizePosition::BottomRight,
        Grip::TopLeft => NSCursorFrameResizePosition::TopLeft,
        Grip::TopRight => NSCursorFrameResizePosition::TopRight,
    }
}

fn still(change: impl FnOnce()) {
    CATransaction::begin();
    CATransaction::setDisableActions(true);
    change();
    CATransaction::commit();
}

fn let_cursors_change_in_background() {
    let Some(yes) = (unsafe { kCFBooleanTrue }) else {
        return;
    };
    let key = CFString::from_static_str("SetsCursorInBackground");
    unsafe {
        let connection = main_connection();
        set_connection_property(connection, connection, &key, yes);
    }
}
