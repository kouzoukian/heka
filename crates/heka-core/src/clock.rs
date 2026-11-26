use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Timestamp {
    pub year: u16,
    pub month: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
    pub millisecond: u16,
}

impl Timestamp {
    pub fn time_of_day(&self) -> String {
        let Self {
            hour,
            minute,
            second,
            ..
        } = self;
        format!("{hour:02}:{minute:02}:{second:02}")
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            year,
            month,
            day,
            millisecond,
            ..
        } = self;
        let time = self.time_of_day();
        write!(f, "{year:04}-{month:02}-{day:02} {time}.{millisecond:03}")
    }
}
