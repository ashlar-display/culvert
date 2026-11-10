use hdmi_hal::scdc::ScdcTransport;

use crate::codec;
use crate::error::{ReadModifyWriteError, ScdcError};
use crate::register::{ScramblerStatus, TmdsConfig, TmdsConfigReading};

use super::Scdc;

impl<T: ScdcTransport> Scdc<T> {
    /// Writes scrambling configuration to `TMDS_Config` (0x20).
    ///
    /// Sets `Scrambling_Enable` (bit 0) and `TMDS_Bit_Clock_Ratio` (bit 1).
    pub fn write_tmds_config(&mut self, config: TmdsConfig) -> Result<(), ScdcError<T::Error>> {
        self.transport
            .write(codec::TMDS_CONFIG, codec::encode_tmds_config(config))
            .map_err(ScdcError::Transport)
    }

    /// Reads `TMDS_Config` (0x20): the two bits [`write_tmds_config`](Self::write_tmds_config)
    /// sets, and the whole byte.
    pub fn read_tmds_config(&mut self) -> Result<TmdsConfigReading, ScdcError<T::Error>> {
        let raw = self
            .transport
            .read(codec::TMDS_CONFIG)
            .map_err(ScdcError::Transport)?;
        Ok(TmdsConfigReading {
            config: codec::decode_tmds_config(raw),
            raw,
        })
    }

    /// Sets `Scrambling_Enable` (bit 0) and `TMDS_Bit_Clock_Ratio` (bit 1) of `TMDS_Config`
    /// (0x20) and keeps its other bits: reads the byte, then writes it back with the two
    /// bits from `config` and every other bit as read.
    ///
    /// [`write_tmds_config`](Self::write_tmds_config) writes the whole byte instead, the
    /// other bits 0. Two transfers, not one transaction: a failed read writes nothing
    /// ([`ReadModifyWriteError::Read`]), and a failed write says so
    /// ([`ReadModifyWriteError::Write`]).
    pub fn update_tmds_config(
        &mut self,
        config: TmdsConfig,
    ) -> Result<(), ReadModifyWriteError<T::Error>> {
        let byte = self
            .transport
            .read(codec::TMDS_CONFIG)
            .map_err(|e| ReadModifyWriteError::Read(ScdcError::Transport(e)))?;
        self.transport
            .write(
                codec::TMDS_CONFIG,
                codec::update_tmds_config_byte(byte, config),
            )
            .map_err(|e| ReadModifyWriteError::Write(ScdcError::Transport(e)))
    }

    /// Reads scrambler acknowledgement from `Scrambler_Status` (0x21).
    ///
    /// Returns [`ScramblerStatus::scrambling_active`] set when the sink confirms
    /// that TMDS scrambling is active (bit 0), and the byte read
    /// ([`ScramblerStatus::raw`]).
    pub fn read_scrambler_status(&mut self) -> Result<ScramblerStatus, ScdcError<T::Error>> {
        let byte = self
            .transport
            .read(codec::SCRAMBLER_STATUS)
            .map_err(ScdcError::Transport)?;
        Ok(codec::decode_scrambler_status(byte))
    }
}

#[cfg(test)]
mod tests {
    use super::super::Scdc;
    use super::super::test_transport::{Op, TestTransport};
    use crate::error::{ReadModifyWriteError, ScdcError};
    use crate::register::{ScramblerStatus, TmdsConfig, TmdsConfigReading};

    const SCRAMBLED_40: TmdsConfig = TmdsConfig {
        scrambling_enable: true,
        high_tmds_clock_ratio: true,
    };

    #[test]
    fn read_tmds_config_decodes_the_two_bits_and_keeps_the_byte() {
        let mut sim = TestTransport::new();
        sim.set(0x20, 0xA6); // ratio set, scrambling clear, reserved bits set
        assert_eq!(
            Scdc::new(sim).read_tmds_config().unwrap(),
            TmdsConfigReading {
                config: TmdsConfig {
                    scrambling_enable: false,
                    high_tmds_clock_ratio: true,
                },
                raw: 0xA6,
            }
        );
    }

    #[test]
    fn update_tmds_config_keeps_the_other_bits() {
        let mut sim = TestTransport::new();
        sim.set(0x20, 0xF0);
        let mut scdc = Scdc::new(sim);
        scdc.update_tmds_config(SCRAMBLED_40).unwrap();
        let sim = scdc.into_transport();
        assert_eq!(sim.get(0x20), 0xF3);
        assert_eq!(
            sim.log()[..3],
            [Some(Op::Read(0x20)), Some(Op::Write(0x20, 0xF3)), None]
        );
    }

    #[test]
    fn update_tmds_config_clears_only_the_two_bits() {
        let mut sim = TestTransport::new();
        sim.set(0x20, 0xFF);
        let mut scdc = Scdc::new(sim);
        scdc.update_tmds_config(TmdsConfig {
            scrambling_enable: false,
            high_tmds_clock_ratio: false,
        })
        .unwrap();
        assert_eq!(scdc.into_transport().get(0x20), 0xFC);
    }

    #[test]
    fn update_tmds_config_writes_nothing_after_a_failed_read() {
        let mut scdc = Scdc::new(TestTransport::failing_after(0));
        assert!(
            scdc.update_tmds_config(SCRAMBLED_40)
                .is_err_and(|error| matches!(
                    error,
                    ReadModifyWriteError::Read(ScdcError::Transport(()))
                ))
        );
        assert_eq!(scdc.into_transport().log()[0], None);
    }

    #[test]
    fn update_tmds_config_reports_a_failed_write() {
        let mut sim = TestTransport::failing_after(1);
        sim.set(0x20, 0x80);
        let mut scdc = Scdc::new(sim);
        assert!(
            scdc.update_tmds_config(SCRAMBLED_40)
                .is_err_and(|error| matches!(
                    error,
                    ReadModifyWriteError::Write(ScdcError::Transport(()))
                ))
        );
        let sim = scdc.into_transport();
        assert_eq!(sim.log()[..2], [Some(Op::Read(0x20)), None]);
        assert_eq!(sim.get(0x20), 0x80);
    }

    #[test]
    fn tmds_config_scrambling_only() {
        let mut scdc = Scdc::new(TestTransport::new());
        scdc.write_tmds_config(TmdsConfig {
            scrambling_enable: true,
            high_tmds_clock_ratio: false,
        })
        .unwrap();
        assert_eq!(scdc.into_transport().get(0x20), 0x01);
    }

    #[test]
    fn tmds_config_clock_ratio_only() {
        let mut scdc = Scdc::new(TestTransport::new());
        scdc.write_tmds_config(TmdsConfig {
            scrambling_enable: false,
            high_tmds_clock_ratio: true,
        })
        .unwrap();
        assert_eq!(scdc.into_transport().get(0x20), 0x02);
    }

    #[test]
    fn tmds_config_both_clear() {
        let mut scdc = Scdc::new(TestTransport::new());
        scdc.write_tmds_config(TmdsConfig {
            scrambling_enable: false,
            high_tmds_clock_ratio: false,
        })
        .unwrap();
        assert_eq!(scdc.into_transport().get(0x20), 0x00);
    }

    #[test]
    fn scrambler_status_bit0_set() {
        let mut sim = TestTransport::new();
        sim.set(0x21, 0xFF); // all bits set; only bit 0 is defined
        assert_eq!(
            Scdc::new(sim).read_scrambler_status().unwrap(),
            ScramblerStatus {
                scrambling_active: true,
                raw: 0xFF,
            }
        );
    }

    #[test]
    fn scrambler_status_bit0_clear() {
        let mut sim = TestTransport::new();
        sim.set(0x21, 0xFE); // bit 0 clear, other bits set (reserved)
        assert_eq!(
            Scdc::new(sim).read_scrambler_status().unwrap(),
            ScramblerStatus {
                scrambling_active: false,
                raw: 0xFE,
            }
        );
    }

    #[test]
    fn transport_error_propagates() {
        assert!(
            Scdc::new(TestTransport::failing_after(0))
                .write_tmds_config(TmdsConfig {
                    scrambling_enable: false,
                    high_tmds_clock_ratio: false
                })
                .is_err_and(|error| matches!(error, ScdcError::Transport(())))
        );
        assert!(
            Scdc::new(TestTransport::failing_after(0))
                .read_scrambler_status()
                .is_err_and(|error| matches!(error, ScdcError::Transport(())))
        );
        assert!(
            Scdc::new(TestTransport::failing_after(0))
                .read_tmds_config()
                .is_err_and(|error| matches!(error, ScdcError::Transport(())))
        );
    }
}
