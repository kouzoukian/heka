use serde::{Deserialize, Serialize};

const TITLE_BAR_MIDDLE: i64 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    fn centre(&self) -> (i64, i64) {
        (
            i64::from(self.x) + i64::from(self.width / 2),
            i64::from(self.y) + i64::from(self.height / 2),
        )
    }

    fn holds(&self, (x, y): (i64, i64)) -> bool {
        let within = |at: i64, start: i32, length: u32| {
            (i64::from(start)..i64::from(start) + i64::from(length)).contains(&at)
        };
        within(x, self.x, self.width) && within(y, self.y, self.height)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowPlacement {
    #[serde(flatten)]
    pub bounds: Rect,
    #[serde(default)]
    pub maximized: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monitor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Monitor {
    pub id: Option<String>,
    pub area: Rect,
}

impl WindowPlacement {
    pub fn monitor_in<'a>(&self, monitors: &'a [Monitor]) -> Option<&'a Monitor> {
        let centre = self.bounds.centre();
        let title_bar = (centre.0, i64::from(self.bounds.y) + TITLE_BAR_MIDDLE);
        if !monitors.iter().any(|monitor| monitor.area.holds(title_bar)) {
            return None;
        }
        let holding = || monitors.iter().filter(|monitor| monitor.area.holds(centre));
        holding()
            .find(|monitor| monitor.id == self.monitor)
            .or_else(|| holding().next())
    }
}
