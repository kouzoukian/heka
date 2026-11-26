pub const BT_INPUT_SEED: u8 = 0xA1;
pub const BT_OUTPUT_SEED: u8 = 0xA2;
pub const BT_FEATURE_SEED: u8 = 0xA3;

pub const CRC_LEN: usize = 4;

const POLYNOMIAL: u32 = 0xEDB8_8320;

const TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ POLYNOMIAL
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
};

fn update(mut crc: u32, bytes: &[u8]) -> u32 {
    for &byte in bytes {
        crc = TABLE[((crc ^ u32::from(byte)) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc
}

pub fn crc32(bytes: &[u8]) -> u32 {
    !update(!0, bytes)
}

pub fn report_crc(seed: u8, report: &[u8]) -> u32 {
    !update(update(!0, &[seed]), report)
}

pub fn has_valid_crc(seed: u8, report: &[u8]) -> bool {
    let Some(split) = report.len().checked_sub(CRC_LEN) else {
        return false;
    };
    let (body, crc) = report.split_at(split);
    report_crc(seed, body) == u32::from_le_bytes([crc[0], crc[1], crc[2], crc[3]])
}

pub fn seal(seed: u8, report: &mut [u8]) {
    let split = report.len() - CRC_LEN;
    let crc = report_crc(seed, &report[..split]);
    report[split..].copy_from_slice(&crc.to_le_bytes());
}
