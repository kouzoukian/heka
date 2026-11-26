use crate::link::ReportError;

pub const GYRO_RES_PER_DEG_S: i32 = 1024;
pub const ACC_RES_PER_G: i32 = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AxisCalibration {
    pub bias: i32,
    pub numer: i32,
    pub denom: i32,
}

impl AxisCalibration {
    const fn full_scale(range: i32) -> Self {
        Self {
            bias: 0,
            numer: range,
            denom: i16::MAX as i32,
        }
    }

    pub fn apply(self, raw: i16) -> i32 {
        let value =
            i64::from(self.numer) * (i64::from(raw) - i64::from(self.bias)) / i64::from(self.denom);
        value.clamp(i32::MIN.into(), i32::MAX.into()) as i32
    }

    pub fn invert(self, value: i32) -> i16 {
        if self.numer == 0 {
            return 0;
        }
        let raw = div_round(
            i64::from(value) * i64::from(self.denom),
            i64::from(self.numer),
        );
        (raw + i64::from(self.bias)).clamp(i16::MIN.into(), i16::MAX.into()) as i16
    }
}

fn div_round(a: i64, b: i64) -> i64 {
    let (quotient, remainder) = (a / b, a % b);
    if 2 * remainder.abs() >= b.abs() {
        quotient + a.signum() * b.signum()
    } else {
        quotient
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Calibration {
    pub gyro: [AxisCalibration; 3],
    pub accel: [AxisCalibration; 3],
}

const GYRO_RANGE: i32 = 2048 * GYRO_RES_PER_DEG_S;
const ACC_RANGE: i32 = 4 * ACC_RES_PER_G;

const FIELDS: usize = 17;
pub const CALIBRATION_BLOCK_LEN: usize = 1 + 2 * FIELDS;

impl Calibration {
    pub const UNCALIBRATED: Self = Self {
        gyro: [AxisCalibration::full_scale(GYRO_RANGE); 3],
        accel: [AxisCalibration::full_scale(ACC_RANGE); 3],
    };

    pub fn from_paired_block(block: &[u8]) -> Self {
        Self::from_paired_fields(fields(block))
    }

    pub fn from_grouped_block(block: &[u8]) -> Self {
        let mut f = fields(block);
        let [pitch_plus, yaw_plus, roll_plus] = [f[3], f[4], f[5]];
        let [pitch_minus, yaw_minus, roll_minus] = [f[6], f[7], f[8]];
        f[3..9].copy_from_slice(&[
            pitch_plus,
            pitch_minus,
            yaw_plus,
            yaw_minus,
            roll_plus,
            roll_minus,
        ]);
        Self::from_paired_fields(f)
    }

    fn from_paired_fields(fields: [i32; FIELDS]) -> Self {
        let f = |i: usize| fields[i];
        let speed_2x = f(9) + f(10);
        let gyro = |bias: i32, plus: i32, minus: i32| {
            let denom = (plus - bias).abs() + (minus - bias).abs();
            if denom == 0 {
                AxisCalibration::full_scale(GYRO_RANGE)
            } else {
                AxisCalibration {
                    bias: 0,
                    numer: speed_2x * GYRO_RES_PER_DEG_S,
                    denom,
                }
            }
        };
        let accel = |plus: i32, minus: i32| {
            let range_2g = plus - minus;
            if range_2g == 0 {
                AxisCalibration::full_scale(ACC_RANGE)
            } else {
                AxisCalibration {
                    bias: plus - range_2g / 2,
                    numer: 2 * ACC_RES_PER_G,
                    denom: range_2g,
                }
            }
        };
        Self {
            gyro: [
                gyro(f(0), f(3), f(4)),
                gyro(f(1), f(5), f(6)),
                gyro(f(2), f(7), f(8)),
            ],
            accel: [
                accel(f(11), f(12)),
                accel(f(13), f(14)),
                accel(f(15), f(16)),
            ],
        }
    }

    pub fn gyro(&self, raw: [i16; 3]) -> [i32; 3] {
        std::array::from_fn(|i| self.gyro[i].apply(raw[i]))
    }

    pub fn accel(&self, raw: [i16; 3]) -> [i32; 3] {
        std::array::from_fn(|i| self.accel[i].apply(raw[i]))
    }

    pub fn raw_gyro(&self, value: [i32; 3]) -> [i16; 3] {
        std::array::from_fn(|i| self.gyro[i].invert(value[i]))
    }

    pub fn raw_accel(&self, value: [i32; 3]) -> [i16; 3] {
        std::array::from_fn(|i| self.accel[i].invert(value[i]))
    }
}

fn fields(block: &[u8]) -> [i32; FIELDS] {
    std::array::from_fn(|i| i32::from(i16::from_le_bytes([block[1 + 2 * i], block[2 + 2 * i]])))
}

pub(crate) fn check_report(report: &[u8], id: u8, len: usize) -> Result<(), ReportError> {
    if report.len() < len {
        return Err(ReportError::TooShort { len: report.len() });
    }
    if report[0] != id {
        return Err(ReportError::UnexpectedId { id: report[0] });
    }
    Ok(())
}
