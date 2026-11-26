//! What both pads are drawn with: curves, circles and the controls they share.

use gpui::{
    BorderStyle, Bounds, ContentMask, Hsla, PathBuilder, Pixels, Point, Window, point, px, quad,
    size, transparent_black,
};
use gpui_component::Theme;
use heka_core::state::{Buttons, Stick};

use super::WIDTH;
use crate::ui::pad::{stick_offset, trigger_fill};

pub type Xy = (f32, f32);

/// A cubic curve from where the last one ended: two control points and its end.
pub type Curve = (Xy, Xy, Xy);

/// Moves a point around the middle by a quarter or half turn.
type Turn = fn(Xy) -> Xy;

/// An outline made of curves, closed back to its start.
#[derive(Clone)]
pub struct Shape {
    start: Xy,
    curves: Vec<Curve>,
}

impl Shape {
    pub fn new(start: Xy, curves: &[Curve]) -> Self {
        Self {
            start,
            curves: curves.to_vec(),
        }
    }

    /// Straight edges between `points`.
    pub fn polygon(points: &[Xy]) -> Self {
        let curves = points
            .windows(2)
            .map(|edge| (edge[0], edge[1], edge[1]))
            .collect();
        Self {
            start: points[0],
            curves,
        }
    }

    /// A shape the same on both sides, from its right half: from a point on the
    /// drawing's middle line, clockwise, back to that line.
    pub fn symmetric(start: Xy, right: &[Curve]) -> Self {
        let mut curves = right.to_vec();
        // The left half is the right one walked backwards, so each curve ends
        // where the one before it ends and takes its control points swapped.
        for (index, &(first, second, _)) in right.iter().enumerate().rev() {
            let end = index.checked_sub(1).map_or(start, |before| right[before].2);
            curves.push((mirror(second), mirror(first), mirror(end)));
        }
        Self { start, curves }
    }

    /// The same shape on the other side of the drawing.
    pub fn mirrored(&self) -> Self {
        Self {
            start: mirror(self.start),
            curves: self
                .curves
                .iter()
                .map(|&(first, second, end)| (mirror(first), mirror(second), mirror(end)))
                .collect(),
        }
    }

    /// The same shape with every point moved by `place`.
    pub fn map(&self, place: impl Fn(Xy) -> Xy) -> Self {
        Self {
            start: place(self.start),
            curves: self
                .curves
                .iter()
                .map(|&(first, second, end)| (place(first), place(second), place(end)))
                .collect(),
        }
    }

    /// The top and bottom of the shape's points.
    fn height(&self) -> (f32, f32) {
        self.curves
            .iter()
            .flat_map(|&(first, second, end)| [first.1, second.1, end.1])
            .fold((self.start.1, self.start.1), |(top, bottom), y| {
                (top.min(y), bottom.max(y))
            })
    }
}

fn mirror((x, y): Xy) -> Xy {
    (WIDTH - x, y)
}

/// The colours of one drawing.
#[derive(Clone, Copy)]
pub struct Palette {
    pub body: Hsla,
    pub outline: Hsla,
    pub control: Hsla,
    pub label: Hsla,
    pub bright: Hsla,
    pub pressed: Hsla,
    pub pressed_label: Hsla,
    pub red: Hsla,
    pub green: Hsla,
    pub blue: Hsla,
    pub yellow: Hsla,
    pub magenta: Hsla,
}

impl Palette {
    pub fn new(theme: &Theme) -> Self {
        Self {
            body: theme.secondary,
            outline: theme.muted_foreground.opacity(0.45),
            control: theme.group_box,
            label: theme.muted_foreground,
            bright: theme.foreground,
            pressed: theme.primary,
            pressed_label: theme.primary_foreground,
            red: theme.red,
            green: theme.green,
            blue: theme.blue,
            yellow: theme.yellow,
            magenta: theme.magenta,
        }
    }

    /// A label's colour on a control that is or is not pressed.
    pub fn on(&self, pressed: bool) -> Hsla {
        if pressed {
            self.pressed_label
        } else {
            self.label
        }
    }
}

/// A word on the drawing, centred on `at`.
pub struct Label {
    pub at: Xy,
    pub text: &'static str,
    pub color: Hsla,
}

/// Paints onto the drawing's canvas.
pub struct Painter<'a> {
    origin: Point<Pixels>,
    pub palette: Palette,
    window: &'a mut Window,
}

impl<'a> Painter<'a> {
    pub fn new(origin: Point<Pixels>, palette: Palette, window: &'a mut Window) -> Self {
        Self {
            origin,
            palette,
            window,
        }
    }

    fn at(&self, (x, y): Xy) -> Point<Pixels> {
        self.origin + point(px(x), px(y))
    }

    fn path(&mut self, mut path: PathBuilder, shape: &Shape, color: Hsla) {
        path.move_to(self.at(shape.start));
        for &(first, second, end) in &shape.curves {
            path.cubic_bezier_to(self.at(end), self.at(first), self.at(second));
        }
        path.close();
        // A shape that cannot be built is only missing from the picture.
        if let Ok(path) = path.build() {
            self.window.paint_path(path, color);
        }
    }

    pub fn fill(&mut self, shape: &Shape, color: Hsla) {
        self.path(PathBuilder::fill(), shape, color);
    }

    pub fn stroke(&mut self, shape: &Shape, width: f32, color: Hsla) {
        self.path(PathBuilder::stroke(px(width)), shape, color);
    }

    /// A straight line.
    pub fn line(&mut self, from: Xy, to: Xy, width: f32, color: Hsla) {
        let mut path = PathBuilder::stroke(px(width));
        path.move_to(self.at(from));
        path.line_to(self.at(to));
        if let Ok(path) = path.build() {
            self.window.paint_path(path, color);
        }
    }

    /// A filled and outlined control, in the pressed colour when `pressed`.
    pub fn control(&mut self, shape: &Shape, pressed: bool) {
        let Palette {
            control,
            outline,
            pressed: lit,
            ..
        } = self.palette;
        let (fill, edge) = if pressed {
            (lit, lit)
        } else {
            (control, outline)
        };
        self.fill(shape, fill);
        self.stroke(shape, 1., edge);
    }

    /// A rectangle with rounded corners, from its left, top, width and height.
    pub fn rect(
        &mut self,
        (left, top, width, height): (f32, f32, f32, f32),
        radius: f32,
        fill: Hsla,
        border: (f32, Hsla),
    ) {
        let bounds = Bounds::new(self.at((left, top)), size(px(width), px(height)));
        self.window.paint_quad(quad(
            bounds,
            px(radius),
            fill,
            px(border.0),
            border.1,
            BorderStyle::default(),
        ));
    }

    pub fn circle(&mut self, (x, y): Xy, radius: f32, fill: Hsla, border: (f32, Hsla)) {
        let side = 2. * radius;
        self.rect((x - radius, y - radius, side, side), radius, fill, border);
    }

    /// A round button, filled with `lit` when pressed.
    pub fn button(&mut self, centre: Xy, radius: f32, pressed: bool, lit: Hsla) {
        let Palette {
            control, outline, ..
        } = self.palette;
        let (fill, edge) = if pressed {
            (lit, lit)
        } else {
            (control, outline)
        };
        self.circle(centre, radius, fill, (1., edge));
    }

    /// The faint circle a group of controls sits in.
    pub fn well(&mut self, centre: Xy, radius: f32) {
        let outline = self.palette.outline.opacity(0.4);
        self.circle(centre, radius, transparent_black(), (1., outline));
    }

    /// The d-pad's four keys: `up`, drawn around the middle, turned each way.
    pub fn dpad(&mut self, (x, y): Xy, up: &Shape, held: Buttons) {
        let turns: [(Buttons, Turn); 4] = [
            (Buttons::DPAD_UP, |(x, y)| (x, y)),
            (Buttons::DPAD_DOWN, |(x, y)| (-x, -y)),
            (Buttons::DPAD_LEFT, |(x, y)| (y, -x)),
            (Buttons::DPAD_RIGHT, |(x, y)| (-y, x)),
        ];
        for (button, turn) in turns {
            let key = up.map(|point| {
                let (dx, dy) = turn(point);
                (x + dx, y + dy)
            });
            self.control(&key, held.contains(button));
        }
    }

    /// A trigger that fills from the bottom with its travel, outlined once it
    /// clicks.
    pub fn trigger(&mut self, shape: &Shape, travel: u8, clicked: bool) {
        let palette = self.palette;
        self.fill(shape, palette.control);
        if travel > 0 {
            let (top, bottom) = shape.height();
            let from = bottom - (bottom - top) * trigger_fill(travel);
            let mask = ContentMask {
                bounds: Bounds::new(self.at((0., from)), size(px(WIDTH), px(bottom - from + 1.))),
            };
            let origin = self.origin;
            self.window.with_content_mask(Some(mask), |window| {
                Painter::new(origin, palette, window).fill(shape, palette.pressed.opacity(0.55));
            });
        }
        let edge = if clicked {
            palette.pressed
        } else {
            palette.outline
        };
        self.stroke(shape, 1., edge);
    }

    /// A stick's ring and its cap, which fills when the stick is clicked.
    pub fn stick(&mut self, (x, y): Xy, at: Stick, clicked: bool) {
        /// How far the cap moves from the centre at full travel.
        const TRAVEL: f32 = 8.;
        let palette = self.palette;
        self.circle((x, y), 25., palette.control, (1., palette.outline));
        let (dx, dy) = stick_offset(at);
        let centre = (x + dx * TRAVEL, y + dy * TRAVEL);
        let (fill, edge) = if clicked {
            (palette.pressed, palette.pressed)
        } else if at != Stick::CENTER {
            (palette.body, palette.label)
        } else {
            (palette.body, palette.outline)
        };
        self.circle(centre, 17., fill, (2., edge));
        let dimple = if clicked {
            palette.pressed_label.opacity(0.4)
        } else {
            palette.outline
        };
        self.circle(centre, 10., transparent_black(), (1., dimple));
    }
}
