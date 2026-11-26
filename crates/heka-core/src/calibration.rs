//! Motion sensor calibration: from a Controller's raw gyro and accelerometer
//! readings to the calibrated units of [`crate::state::Motion`], and back.
//!
//! Source: Linux `drivers/hid/hid-playstation.c` (`dualsense_get_calibration_data`,
//! `dualshock4_get_calibration_data` and their input report handlers). Both
//! DualSense and DualShock 4 normalise to 1/1024 degree per second
//! (`DS_GYRO_RES_PER_DEG_S`) and 1/8192 g (`DS_ACC_RES_PER_G`). As in the kernel, the
//! gyro biases only shape the sensitivity; they are not subtracted from readings.

use crate::link::ReportError;

/// Gyro resolution: calibrated units per degree per second (`DS_GYRO_RES_PER_DEG_S`).
pub const GYRO_RES_PER_DEG_S: i32 = 1024;
/// Accelerometer resolution: calibrated units per g (`DS_ACC_RES_PER_G`).
pub const ACC_RES_PER_G: i32 = 8192;

/// Calibrated value = `numer * (raw - bias) / denom`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AxisCalibration {
    pub bias: i32,
    pub numer: i32,
    pub denom: i32,
}

impl AxisCalibration {
    /// The kernel's fallback when the calibration data is broken: the raw `i16`
    /// range spans `range` calibrated units (`DS_GYRO_RANGE`, `DS_ACC_RANGE`).
    const fn full_scale(range: i32) -> Self {
        Self {
            bias: 0,
            numer: range,
            denom: i16::MAX as i32,
        }
    }

    /// Raw reading to calibrated units, truncating like the kernel's `mult_frac`.
    pub fn apply(self, raw: i16) -> i32 {
        let value =
            i64::from(self.numer) * (i64::from(raw) - i64::from(self.bias)) / i64::from(self.denom);
        value.clamp(i32::MIN.into(), i32::MAX.into()) as i32
    }

    /// Calibrated units back to the raw reading that gives them, rounded to the
    /// nearest and clamped to the `i16` range.
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

/// `a / b` rounded to the nearest, halves away from zero.
fn div_round(a: i64, b: i64) -> i64 {
    let (quotient, remainder) = (a / b, a % b);
    if 2 * remainder.abs() >= b.abs() {
        quotient + a.signum() * b.signum()
    } else {
        quotient
    }
}

/// One Controller's calibration for its three gyro and three accelerometer axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Calibration {
    /// Pitch, yaw, roll.
    pub gyro: [AxisCalibration; 3],
    /// x, y, z.
    pub accel: [AxisCalibration; 3],
}

/// `DS_GYRO_RANGE`: the gyro's full range in calibrated units.
const GYRO_RANGE: i32 = 2048 * GYRO_RES_PER_DEG_S;
/// `DS_ACC_RANGE`: the accelerometer's full range in calibrated units.
const ACC_RANGE: i32 = 4 * ACC_RES_PER_G;

/// The calibration block every layout below shares: 17 little-endian `i16` fields
/// starting one byte after the report ID.
const FIELDS: usize = 17;
/// Length of the calibration block including the report ID byte.
pub const CALIBRATION_BLOCK_LEN: usize = 1 + 2 * FIELDS;

impl Calibration {
    /// What a Controller without usable calibration data gets.
    pub const UNCALIBRATED: Self = Self {
        gyro: [AxisCalibration::full_scale(GYRO_RANGE); 3],
        accel: [AxisCalibration::full_scale(ACC_RANGE); 3],
    };

    /// Reads a calibration block whose gyro limits come in plus/minus pairs per
    /// axis: the DualSense feature report `0x05` and the DualShock 4 USB feature
    /// report `0x02`. `block` starts with the report ID byte; its length is checked
    /// by the caller.
    ///
    /// Fields, as `dualsense_get_calibration_data` reads them: pitch, yaw, roll bias
    /// at 1, 3, 5; pitch plus/minus at 7/9, yaw at 11/13, roll at 15/17; speed
    /// plus/minus at 19/21; accel x plus/minus at 23/25, y at 27/29, z at 31/33.
    pub fn from_paired_block(block: &[u8]) -> Self {
        Self::from_paired_fields(fields(block))
    }

    /// Reads a calibration block as the DualShock 4 orders it over Bluetooth
    /// (feature report `0x05`): the three gyro plus values (pitch, yaw, roll at 7,
    /// 9, 11), then the three minus values (at 13, 15, 17). Everything else is where
    /// [`Self::from_paired_block`] has it (`dualshock4_get_calibration_data`).
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

    /// Raw gyro readings to calibrated units.
    pub fn gyro(&self, raw: [i16; 3]) -> [i32; 3] {
        std::array::from_fn(|i| self.gyro[i].apply(raw[i]))
    }

    /// Raw accelerometer readings to calibrated units.
    pub fn accel(&self, raw: [i16; 3]) -> [i32; 3] {
        std::array::from_fn(|i| self.accel[i].apply(raw[i]))
    }

    /// Calibrated gyro back to the raw readings a Controller with this
    /// calibration would send.
    pub fn raw_gyro(&self, value: [i32; 3]) -> [i16; 3] {
        std::array::from_fn(|i| self.gyro[i].invert(value[i]))
    }

    /// Calibrated acceleration back to raw readings, as [`Self::raw_gyro`].
    pub fn raw_accel(&self, value: [i32; 3]) -> [i16; 3] {
        std::array::from_fn(|i| self.accel[i].invert(value[i]))
    }
}

/// The fields of a calibration block, report ID byte included, in the block's order.
fn fields(block: &[u8]) -> [i32; FIELDS] {
    std::array::from_fn(|i| i32::from(i16::from_le_bytes([block[1 + 2 * i], block[2 + 2 * i]])))
}

/// Checks a calibration feature report's ID and length, report ID byte included.
pub(crate) fn check_report(report: &[u8], id: u8, len: usize) -> Result<(), ReportError> {
    if report.len() < len {
        return Err(ReportError::TooShort { len: report.len() });
    }
    if report[0] != id {
        return Err(ReportError::UnexpectedId { id: report[0] });
    }
    Ok(())
}
