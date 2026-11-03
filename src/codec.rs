//! Register addresses and per-register encoding and decoding for the SCDC register map
//! (HDMI 2.1), without I/O.
//!
//! [`Scdc`](crate::Scdc) uses these to turn register bytes into culvert's types and back;
//! `culvert-async`'s client uses the same functions, so the register map exists once. A
//! client only performs the reads and writes: which registers each operation touches is
//! given by the address constants here.
//!
//! The constants cover every register culvert accesses. `Update_1` (0x11),
//! `Test_Config_0` (0xC0) and the manufacturer identification registers (0xD0 onwards)
//! are described in `doc/roadmap.md`.
//!
//! Decoding judges nothing it cannot judge, and withholds nothing: where a decode finds
//! something wrong, as [`decode_ced_block`] does with the CED checksum, it returns the
//! decoded value with a typed warning, and the caller decides.

use crate::error::ProtocolError;
use crate::register::{
    CedCount, CedCounters, CedReading, CedWarning, ClearableUpdateFlags, Config0, FfeLevels,
    FrlConfig, HdmiLanes, LtpReq, LtpRequests, RsCorrectionCount, ScramblerStatus,
    SourceTestConfig, StatusFlags, TmdsConfig, UpdateFlags,
};

// --- Addresses

/// `Sink_Version`.
pub const SINK_VERSION: u8 = 0x01;
/// `Source_Version`.
pub const SOURCE_VERSION: u8 = 0x02;
/// `Update_0`. `Update_1` (0x11) defines no fields in the HDMI 2.1
/// implementations this map is based on, so it is not accessed.
pub const UPDATE_0: u8 = 0x10;
/// `TMDS_Config`.
pub const TMDS_CONFIG: u8 = 0x20;
/// `Scrambler_Status`.
pub const SCRAMBLER_STATUS: u8 = 0x21;
/// `Config_0`.
pub const CONFIG_0: u8 = 0x30;
/// `Config_1`.
pub const CONFIG_1: u8 = 0x31;
/// `Source_Test_Configuration`.
pub const SOURCE_TEST_CONFIG: u8 = 0x35;
/// `Status_Flags_0`.
pub const STATUS_FLAGS_0: u8 = 0x40;
/// `Status_Flags_1`: link training requests for lanes 0–1.
pub const STATUS_FLAGS_1: u8 = 0x41;
/// `Status_Flags_2`: link training requests for lanes 2–3.
pub const STATUS_FLAGS_2: u8 = 0x42;
/// `ERR_DET_0_L`.
pub const ERR_DET_0_L: u8 = 0x50;
/// `ERR_DET_0_H`.
pub const ERR_DET_0_H: u8 = 0x51;
/// `ERR_DET_1_L`.
pub const ERR_DET_1_L: u8 = 0x52;
/// `ERR_DET_1_H`.
pub const ERR_DET_1_H: u8 = 0x53;
/// `ERR_DET_2_L`.
pub const ERR_DET_2_L: u8 = 0x54;
/// `ERR_DET_2_H`.
pub const ERR_DET_2_H: u8 = 0x55;
/// `ERR_DET_Checksum`: makes the CED registers it covers sum to zero (mod 256);
/// checked by [`decode_ced_block`].
pub const ERR_DET_CHECKSUM: u8 = 0x56;
/// `ERR_DET_3_L`.
pub const ERR_DET_3_L: u8 = 0x57;
/// `ERR_DET_3_H`.
pub const ERR_DET_3_H: u8 = 0x58;
/// `RS_Correction_L`: the Reed-Solomon correction count, low byte (FRL).
pub const RS_CORRECTION_L: u8 = 0x59;
/// `RS_Correction_H`: the Reed-Solomon correction count, high byte (FRL).
pub const RS_CORRECTION_H: u8 = 0x5A;

/// The first register of the CED block, `ERR_DET_0_L`.
pub const CED_BLOCK_START: u8 = ERR_DET_0_L;
/// The length of the CED block, 0x50–0x58: the four lanes' counters and the checksum,
/// read whole whatever the lane count, in one transaction.
pub const CED_BLOCK_LEN: usize = 9;

// --- Encoding and decoding

/// Encodes `TMDS_Config`: `Scrambling_Enable` (bit 0) and `TMDS_Bit_Clock_Ratio` (bit 1).
pub fn encode_tmds_config(config: TmdsConfig) -> u8 {
    (config.scrambling_enable as u8) | ((config.high_tmds_clock_ratio as u8) << 1)
}

/// Decodes `Scrambler_Status`: the sink confirms TMDS scrambling is active (bit 0).
pub fn decode_scrambler_status(byte: u8) -> ScramblerStatus {
    ScramblerStatus {
        scrambling_active: byte & 0x01 != 0,
    }
}

/// Decodes `Update_0`.
pub fn decode_update_flags(update_0: u8) -> UpdateFlags {
    UpdateFlags {
        status_update: update_0 & 0x01 != 0,
        ced_update: update_0 & 0x02 != 0,
        rr_test: update_0 & 0x04 != 0,
        source_test_update: update_0 & 0x08 != 0,
        frl_start: update_0 & 0x10 != 0,
        flt_update: update_0 & 0x20 != 0,
        rsed_update: update_0 & 0x40 != 0,
    }
}

/// Encodes the `Update_0` write that clears the flags set in `flags` (write 1 to clear).
/// `RR_Test` (bit 2) is never written: [`ClearableUpdateFlags`] has no field for it.
pub fn encode_clear_update_flags(flags: ClearableUpdateFlags) -> u8 {
    (flags.status_update as u8)
        | ((flags.ced_update as u8) << 1)
        | ((flags.source_test_update as u8) << 3)
        | ((flags.frl_start as u8) << 4)
        | ((flags.flt_update as u8) << 5)
        | ((flags.rsed_update as u8) << 6)
}

/// Encodes `Config_0`: `RR_Enable` (bit 0) and `FLT_No_Retrain` (bit 1).
pub fn encode_config_0(config: Config0) -> u8 {
    (config.rr_enable as u8) | ((config.flt_no_retrain as u8) << 1)
}

/// Encodes `Config_1`: `FRL_Rate` in bits\[3:0\] and `FFE_Levels` in bits\[7:4\].
///
/// Returns [`ProtocolError::FfeLevelsOutOfRange`] if the FFE levels exceed
/// [`FfeLevels::max_for`] the rate; a sink treats a prohibited value as 0.
pub fn encode_config_1(config: FrlConfig) -> Result<u8, ProtocolError> {
    if config.ffe_levels.value() > FfeLevels::max_for(config.frl_rate).value() {
        return Err(ProtocolError::FfeLevelsOutOfRange {
            rate: config.frl_rate,
            levels: config.ffe_levels.value(),
        });
    }
    Ok((config.frl_rate as u8) | (config.ffe_levels.value() << 4))
}

/// Decodes `Source_Test_Configuration`. Bits 0 and 4 are reserved.
pub fn decode_source_test_config(byte: u8) -> SourceTestConfig {
    SourceTestConfig {
        txffe_pre_shoot_only: byte & 0x02 != 0,
        txffe_de_emphasis_only: byte & 0x04 != 0,
        txffe_no_ffe: byte & 0x08 != 0,
        flt_no_timeout: byte & 0x20 != 0,
        dsc_frl_max: byte & 0x40 != 0,
        frl_max: byte & 0x80 != 0,
    }
}

/// Decodes `Status_Flags_0`: clock detection, lane lock, `FLT_Ready` and DSC decode
/// failure.
pub fn decode_status_flags(status_flags_0: u8) -> StatusFlags {
    StatusFlags {
        clock_detected: status_flags_0 & 0x01 != 0,
        ch0_locked: status_flags_0 & 0x02 != 0,
        ch1_locked: status_flags_0 & 0x04 != 0,
        ch2_locked: status_flags_0 & 0x08 != 0,
        ln3_locked: status_flags_0 & 0x10 != 0,
        flt_ready: status_flags_0 & 0x40 != 0,
        dsc_decode_fail: status_flags_0 & 0x80 != 0,
    }
}

/// Decodes the per-lane link training requests from `Status_Flags_1` (lanes 0–1) and
/// `Status_Flags_2` (lanes 2–3), one nibble per lane, low nibble first.
///
/// Every value decodes: one HDMI 2.1 leaves undefined (0x9–0xD) is
/// [`LtpReq::Reserved`].
pub fn decode_ltp_requests(status_flags_1: u8, status_flags_2: u8) -> LtpRequests {
    LtpRequests {
        lane0: LtpReq::from_nibble(status_flags_1),
        lane1: LtpReq::from_nibble(status_flags_1 >> 4),
        lane2: LtpReq::from_nibble(status_flags_2),
        lane3: LtpReq::from_nibble(status_flags_2 >> 4),
    }
}

/// Decodes the CED block, the bytes of registers 0x50–0x58 from [`CED_BLOCK_START`], and
/// checks its checksum. Never fails: what it finds wrong is a [`CedWarning`] in the
/// returned [`CedReading`], next to the counters, which are decoded either way.
///
/// - Each lane's counter is its low and high byte; the high byte's bit 7 is a validity
///   flag, and a lane whose flag is clear is `None`.
/// - The checksum (0x56) makes the bytes it covers sum to zero (mod 256): 0x50–0x56 with
///   [`HdmiLanes::Three`] (TMDS and 3-lane FRL), 0x50–0x58 with [`HdmiLanes::Four`]. If they
///   do not, [`CedWarning::ChecksumMismatch`].
/// - With [`HdmiLanes::Three`], lane 3 is `None`: the checksum does not cover it. If its
///   validity bit is set, [`CedWarning::UnusedLaneReported`] carries the count.
pub fn decode_ced_block(block: &[u8; CED_BLOCK_LEN], lanes: HdmiLanes) -> CedReading {
    let [l0, h0, l1, h1, l2, h2, checksum, l3, h3] = *block;
    let decode = |lo: u8, hi: u8| -> Option<CedCount> {
        (hi & 0x80 != 0).then(|| CedCount::new(((hi as u16) << 8) | lo as u16))
    };
    let covered = match lanes {
        HdmiLanes::Three => &block[..7],
        HdmiLanes::Four => &block[..],
    };
    let sum = covered
        .iter()
        .fold(0u8, |sum, &byte| sum.wrapping_add(byte));
    let mut reading = CedReading::new(CedCounters {
        lane0: decode(l0, h0),
        lane1: decode(l1, h1),
        lane2: decode(l2, h2),
        lane3: match lanes {
            HdmiLanes::Three => None,
            HdmiLanes::Four => decode(l3, h3),
        },
    });
    if sum != 0 {
        // The other covered bytes sum to `sum - checksum`; the checksum that zeroes it is
        // that sum's negation.
        reading.push(CedWarning::ChecksumMismatch {
            expected: sum.wrapping_sub(checksum).wrapping_neg(),
            found: checksum,
        });
    }
    if lanes == HdmiLanes::Three {
        if let Some(count) = decode(l3, h3) {
            reading.push(CedWarning::UnusedLaneReported { count });
        }
    }
    reading
}

/// Decodes the Reed-Solomon correction count from `RS_Correction_L/H`; `None` if the high
/// byte's validity bit (bit 7) is not set.
pub fn decode_rs_correction(low: u8, high: u8) -> Option<RsCorrectionCount> {
    (high & 0x80 != 0).then(|| RsCorrectionCount::new(((high as u16) << 8) | low as u16))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A CED block with `counts` as the four lanes' (low, high) bytes and the checksum
    /// that is correct for `lanes`.
    fn block(counts: [(u8, u8); 4], lanes: HdmiLanes) -> [u8; CED_BLOCK_LEN] {
        let [(l0, h0), (l1, h1), (l2, h2), (l3, h3)] = counts;
        let mut block = [l0, h0, l1, h1, l2, h2, 0, l3, h3];
        let covered = match lanes {
            HdmiLanes::Three => &block[..7],
            HdmiLanes::Four => &block[..],
        };
        let sum = covered
            .iter()
            .fold(0u8, |sum, &byte| sum.wrapping_add(byte));
        block[6] = sum.wrapping_neg();
        block
    }

    fn count(value: u16) -> Option<CedCount> {
        Some(CedCount::new(value))
    }

    const VALID_LANES: [(u8, u8); 4] = [(0x02, 0x83), (0x04, 0x85), (0x06, 0x87), (0x08, 0x89)];

    #[test]
    fn three_lanes_with_a_valid_checksum() {
        let lanes = [(0x02, 0x83), (0x04, 0x85), (0x06, 0x87), (0x00, 0x00)];
        let reading = decode_ced_block(&block(lanes, HdmiLanes::Three), HdmiLanes::Three);
        assert_eq!(
            reading.counters,
            CedCounters::new(count(0x0302), count(0x0504), count(0x0706), None)
        );
        assert_eq!(reading.warnings(), []);
    }

    #[test]
    fn four_lanes_with_a_valid_checksum() {
        let reading = decode_ced_block(&block(VALID_LANES, HdmiLanes::Four), HdmiLanes::Four);
        assert_eq!(
            reading.counters,
            CedCounters::new(count(0x0302), count(0x0504), count(0x0706), count(0x0908))
        );
        assert_eq!(reading.warnings(), []);
    }

    #[test]
    fn a_clear_validity_bit_is_none() {
        let lanes = [(0xFF, 0x7F), (0x04, 0x85), (0x00, 0x00), (0x01, 0x00)];
        let reading = decode_ced_block(&block(lanes, HdmiLanes::Four), HdmiLanes::Four);
        assert_eq!(
            reading.counters,
            CedCounters::new(None, count(0x0504), None, None)
        );
        assert_eq!(reading.warnings(), []);
    }

    #[test]
    fn a_torn_read_is_a_checksum_mismatch_with_the_counts_still_decoded() {
        // The sink's counter is 0x00FF with its checksum; it ticks to 0x0100 between the
        // low and the high byte, which read back as 0x01FF.
        let lanes = [(0xFF, 0x80), (0x00, 0x00), (0x00, 0x00), (0x00, 0x00)];
        let mut torn = block(lanes, HdmiLanes::Three);
        torn[1] = 0x81;
        let reading = decode_ced_block(&torn, HdmiLanes::Three);
        assert_eq!(reading.counters.lane0, count(0x01FF));
        assert_eq!(
            reading.warnings(),
            [CedWarning::ChecksumMismatch {
                expected: torn[6].wrapping_sub(1),
                found: torn[6],
            }]
        );
    }

    #[test]
    fn expected_is_the_checksum_that_makes_the_block_valid() {
        let valid = block(VALID_LANES, HdmiLanes::Four);
        let mut wrong = valid;
        wrong[6] = valid[6].wrapping_add(0x10);
        assert_eq!(
            decode_ced_block(&wrong, HdmiLanes::Four).warnings(),
            [CedWarning::ChecksumMismatch {
                expected: valid[6],
                found: wrong[6],
            }]
        );
    }

    #[test]
    fn the_lane_count_decides_what_the_checksum_covers() {
        // Valid for three lanes, with a lane 3 count the 3-lane checksum leaves out.
        let lanes = [(0x02, 0x83), (0x04, 0x85), (0x06, 0x87), (0x08, 0x09)];
        let three = block(lanes, HdmiLanes::Three);
        assert_eq!(decode_ced_block(&three, HdmiLanes::Three).warnings(), []);
        assert!(matches!(
            decode_ced_block(&three, HdmiLanes::Four).warnings(),
            [CedWarning::ChecksumMismatch { .. }]
        ));
        let four = block(lanes, HdmiLanes::Four);
        assert!(matches!(
            decode_ced_block(&four, HdmiLanes::Three).warnings(),
            [CedWarning::ChecksumMismatch { .. }]
        ));
    }

    #[test]
    fn lane_3_reported_in_3_lane_mode_is_a_warning_not_a_count() {
        let reading = decode_ced_block(&block(VALID_LANES, HdmiLanes::Three), HdmiLanes::Three);
        assert_eq!(reading.counters.lane3, None);
        assert_eq!(
            reading.warnings(),
            [CedWarning::UnusedLaneReported {
                count: CedCount::new(0x0908),
            }]
        );
    }

    #[test]
    fn the_warnings_follow_the_checksum_byte_and_lane_3_exhaustively() {
        // Every checksum byte, both lane counts, lane 3 with and without its validity bit:
        // the inputs that decide the warnings. A mismatch exactly when the covered bytes do
        // not sum to zero, `expected` the byte that makes them, `found` the byte read; lane
        // 3 reported exactly when it is valid and not in use.
        for lanes in [HdmiLanes::Three, HdmiLanes::Four] {
            for h3 in [0x00, 0x89] {
                for checksum in 0..=u8::MAX {
                    let mut bytes = block(
                        [(0x02, 0x83), (0x04, 0x85), (0x06, 0x87), (0x08, h3)],
                        lanes,
                    );
                    bytes[6] = checksum;
                    let covered = match lanes {
                        HdmiLanes::Three => &bytes[..7],
                        HdmiLanes::Four => &bytes[..],
                    };
                    let sum = covered.iter().fold(0u8, |sum, &b| sum.wrapping_add(b));
                    // The other covered bytes sum to `sum - checksum`: `expected` cancels it.
                    let mismatch = (sum != 0).then_some(CedWarning::ChecksumMismatch {
                        expected: checksum.wrapping_sub(sum),
                        found: checksum,
                    });
                    let unused = (lanes == HdmiLanes::Three && h3 & 0x80 != 0).then_some(
                        CedWarning::UnusedLaneReported {
                            count: CedCount::new(u16::from(h3) << 8 | 0x08),
                        },
                    );
                    let reading = decode_ced_block(&bytes, lanes);
                    assert!(
                        reading
                            .iter_warnings()
                            .copied()
                            .eq([mismatch, unused].into_iter().flatten()),
                        "{checksum:#04x}, {lanes:?}, {h3:#04x}"
                    );
                }
            }
        }
    }

    #[test]
    fn both_warnings_at_once_in_the_order_found() {
        let mut wrong = block(VALID_LANES, HdmiLanes::Three);
        wrong[6] = wrong[6].wrapping_add(1);
        let reading = decode_ced_block(&wrong, HdmiLanes::Three);
        assert!(matches!(
            reading.warnings(),
            [
                CedWarning::ChecksumMismatch { .. },
                CedWarning::UnusedLaneReported { .. },
            ]
        ));
        assert!(reading.iter_warnings().eq(reading.warnings()));
    }

    #[test]
    fn readings_compare_and_print_only_their_warnings() {
        let clean = decode_ced_block(&block(VALID_LANES, HdmiLanes::Four), HdmiLanes::Four);
        let again = decode_ced_block(&block(VALID_LANES, HdmiLanes::Four), HdmiLanes::Four);
        let warned = decode_ced_block(&block(VALID_LANES, HdmiLanes::Three), HdmiLanes::Three);
        assert_eq!(clean, again);
        assert_ne!(clean.clone(), warned);
        let mut printed = DebugBuf::default();
        core::fmt::write(&mut printed, format_args!("{clean:?}")).unwrap();
        assert!(printed.as_str().ends_with("warnings: [] }"));
    }

    /// Collects `Debug` output without `alloc`.
    struct DebugBuf {
        bytes: [u8; 256],
        len: usize,
    }

    impl Default for DebugBuf {
        fn default() -> Self {
            Self {
                bytes: [0; 256],
                len: 0,
            }
        }
    }

    impl DebugBuf {
        fn as_str(&self) -> &str {
            core::str::from_utf8(&self.bytes[..self.len]).unwrap()
        }
    }

    impl core::fmt::Write for DebugBuf {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let end = self.len + s.len();
            let slot = self.bytes.get_mut(self.len..end).ok_or(core::fmt::Error)?;
            slot.copy_from_slice(s.as_bytes());
            self.len = end;
            Ok(())
        }
    }
}
