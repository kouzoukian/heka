use gpui::{
    BorderStyle, Bounds, ContentMask, Hsla, PathBuilder, Pixels, Point, Window, point, px, quad,
    size, transparent_black,
};
use gpui_component::Theme;
use heka_core::state::{Buttons, Stick};

use super::WIDTH;
use crate::ui::pad::{stick_offset, trigger_fill};

pub type Xy = (f32, f32);

pub type Curve = (Xy, Xy, Xy);

type Turn = fn(Xy) -> Xy;

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

    pub fn symmetric(start: Xy, right: &[Curve]) -> Self {
        let mut curves = right.to_vec();
        for (index, &(first, second, _)) in right.iter().enumerate().rev() {
            let end = index.checked_sub(1).map_or(start, |before| right[before].2);
            curves.push((mirror(second), mirror(first), mirror(end)));
        }
        Self { start, curves }
    }

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

    pub fn on(&self, pressed: bool) -> Hsla {
        if pressed {
            self.pressed_label
        } else {
            self.label
        }
    }
}

pub struct Label {
    pub at: Xy,
    pub text: &'static str,
    pub color: Hsla,
}

pub struct Painter<'a> {
    origin: Point<Pixels>,
    scale: f32,
    pub palette: Palette,
    window: &'a mut Window,
}

impl<'a> Painter<'a> {
    pub fn new(
        origin: Point<Pixels>,
        scale: f32,
        palette: Palette,
        window: &'a mut Window,
    ) -> Self {
        Self {
            origin,
            scale,
            palette,
            window,
        }
    }

    fn length(&self, length: f32) -> Pixels {
        px(length * self.scale)
    }

    fn at(&self, (x, y): Xy) -> Point<Pixels> {
        self.origin + point(self.length(x), self.length(y))
    }

    fn path(&mut self, mut path: PathBuilder, shape: &Shape, color: Hsla) {
        path.move_to(self.at(shape.start));
        for &(first, second, end) in &shape.curves {
            path.cubic_bezier_to(self.at(end), self.at(first), self.at(second));
        }
        path.close();
        if let Ok(path) = path.build() {
            self.window.paint_path(path, color);
        }
    }

    pub fn fill(&mut self, shape: &Shape, color: Hsla) {
        self.path(PathBuilder::fill(), shape, color);
    }

    pub fn stroke(&mut self, shape: &Shape, width: f32, color: Hsla) {
        self.path(PathBuilder::stroke(self.length(width)), shape, color);
    }

    pub fn line(&mut self, from: Xy, to: Xy, width: f32, color: Hsla) {
        let mut path = PathBuilder::stroke(self.length(width));
        path.move_to(self.at(from));
        path.line_to(self.at(to));
        if let Ok(path) = path.build() {
            self.window.paint_path(path, color);
        }
    }

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

    pub fn rect(
        &mut self,
        (left, top, width, height): (f32, f32, f32, f32),
        radius: f32,
        fill: Hsla,
        border: (f32, Hsla),
    ) {
        let bounds = Bounds::new(
            self.at((left, top)),
            size(self.length(width), self.length(height)),
        );
        self.window.paint_quad(quad(
            bounds,
            self.length(radius),
            fill,
            self.length(border.0),
            border.1,
            BorderStyle::default(),
        ));
    }

    pub fn circle(&mut self, (x, y): Xy, radius: f32, fill: Hsla, border: (f32, Hsla)) {
        let side = 2. * radius;
        self.rect((x - radius, y - radius, side, side), radius, fill, border);
    }

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

    pub fn well(&mut self, centre: Xy, radius: f32) {
        let outline = self.palette.outline.opacity(0.4);
        self.circle(centre, radius, transparent_black(), (1., outline));
    }

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

    pub fn trigger(&mut self, shape: &Shape, travel: u8, clicked: bool) {
        let palette = self.palette;
        self.fill(shape, palette.control);
        if travel > 0 {
            let (top, bottom) = shape.height();
            let from = bottom - (bottom - top) * trigger_fill(travel);
            let mask = ContentMask {
                bounds: Bounds::new(
                    self.at((0., from)),
                    size(self.length(WIDTH), self.length(bottom - from + 1.)),
                ),
            };
            let (origin, scale) = (self.origin, self.scale);
            self.window.with_content_mask(Some(mask), |window| {
                Painter::new(origin, scale, palette, window)
                    .fill(shape, palette.pressed.opacity(0.55));
            });
        }
        let edge = if clicked {
            palette.pressed
        } else {
            palette.outline
        };
        self.stroke(shape, 1., edge);
    }

    pub fn stick(&mut self, (x, y): Xy, at: Stick, clicked: bool) {
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
