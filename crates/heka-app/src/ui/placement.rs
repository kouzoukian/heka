use gpui::{
    App, Bounds, DisplayId, Pixels, PlatformDisplay, Size, Window, WindowBounds, point, px, size,
};
use heka_core::window_placement::{Monitor, Rect, WindowPlacement};
use heka_windows::window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

const SIZE: (f32, f32) = (840., 680.);
pub const MIN_SIZE: (f32, f32) = (640., 560.);

pub fn opening(saved: Option<&WindowPlacement>, cx: &App) -> (WindowBounds, Option<DisplayId>) {
    let centred = |size| Bounds::centered(None, size, cx);
    let Some(saved) = saved else {
        let default = centred(size(px(SIZE.0), px(SIZE.1)));
        return (WindowBounds::Windowed(default), None);
    };
    let displays = cx.displays();
    let monitors: Vec<Monitor> = displays
        .iter()
        .map(|display| Monitor {
            id: id(display.as_ref()),
            area: rect(display.bounds()),
        })
        .collect();
    let found = saved.monitor_in(&monitors).and_then(|found| {
        let place = monitors.iter().position(|monitor| monitor == found)?;
        displays.get(place)
    });
    let Rect {
        x,
        y,
        width,
        height,
    } = saved.bounds;
    let kept: Size<Pixels> = size(px(width as f32), px(height as f32));
    let at_least = |kept: Size<Pixels>| {
        size(
            kept.width.max(px(MIN_SIZE.0)),
            kept.height.max(px(MIN_SIZE.1)),
        )
    };
    let bounds = match found {
        Some(_) => Bounds::new(point(px(x as f32), px(y as f32)), at_least(kept)),
        None => {
            let fitting = cx.primary_display().map_or(kept, |display| {
                let room = display.bounds().size;
                size(kept.width.min(room.width), kept.height.min(room.height))
            });
            centred(at_least(fitting))
        }
    };
    let bounds = if saved.maximized {
        WindowBounds::Maximized(bounds)
    } else {
        WindowBounds::Windowed(bounds)
    };
    (bounds, found.map(|display| display.id()))
}

pub fn keep_size(window: &mut Window, asked: WindowBounds) {
    if let WindowBounds::Windowed(asked) = asked
        && window.viewport_size() != asked.size
    {
        window.resize(asked.size);
    }
}

pub fn current(window: &Window, cx: &App) -> Option<WindowPlacement> {
    let RawWindowHandle::Win32(handle) = HasWindowHandle::window_handle(window).ok()?.as_raw()
    else {
        return None;
    };
    let on = DisplayId::new(window::monitor_unless_minimized(handle.hwnd.get())?);
    let (bounds, maximized) = match window.window_bounds() {
        WindowBounds::Maximized(bounds) => (bounds, true),
        WindowBounds::Windowed(bounds) | WindowBounds::Fullscreen(bounds) => (bounds, false),
    };
    Some(WindowPlacement {
        bounds: rect(bounds),
        maximized,
        monitor: cx
            .displays()
            .iter()
            .find(|display| display.id() == on)
            .and_then(|display| id(display.as_ref())),
    })
}

fn id(display: &dyn PlatformDisplay) -> Option<String> {
    display.uuid().ok().map(|uuid| uuid.to_string())
}

fn rect(bounds: Bounds<Pixels>) -> Rect {
    let whole = |length: Pixels| length.as_f32().round();
    Rect {
        x: whole(bounds.origin.x) as i32,
        y: whole(bounds.origin.y) as i32,
        width: whole(bounds.size.width) as u32,
        height: whole(bounds.size.height) as u32,
    }
}
