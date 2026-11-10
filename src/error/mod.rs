//! Error types returned by [`crate::Scdc`] operations.
//!
//! Each derives `Clone`, `Copy`, `PartialEq` and `Eq` where the transport's error
//! does, so a caller can keep, compare and pass them by value without `alloc`, and each
//! implements `Display` and `core::error::Error`, with the transport's error as the
//! `source` of a transport failure.

use core::fmt;

/// Errors returned by [`Scdc`](crate::Scdc) register operations.
///
/// Two categories of failure are distinguished so that callers can handle them
/// separately: a transport failure means the I²C/DDC bus returned an error; a protocol
/// error means culvert refused the operation because it would break the SCDC protocol
/// (for example FFE levels the rate prohibits), before anything was written.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScdcError<E> {
    /// The underlying transport returned an error.
    Transport(E),
    /// The operation would violate the SCDC protocol; nothing was written.
    Protocol(ProtocolError),
}

/// The error of a read-modify-write of one register, saying which of its two transfers
/// failed.
///
/// Returned by [`Scdc::update_tmds_config`](crate::Scdc::update_tmds_config).
///
/// Deliberately exhaustive: a read-modify-write is a read and a write, never a third
/// transfer, so a caller can map both variants without a wildcard arm that would have no
/// error to carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadModifyWriteError<E> {
    /// The read failed; nothing was written.
    Read(ScdcError<E>),
    /// The read succeeded and the write failed; whether the sink took the write is unknown.
    Write(ScdcError<E>),
}

/// What culvert refuses because it would violate the SCDC protocol.
///
/// Every value a sink can report decodes (an undefined link training request is
/// [`LtpReq::Reserved`](crate::LtpReq::Reserved)), so these come from requests, not from
/// register content.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolError {
    /// The sink reported an FRL rate value not defined by HDMI 2.1.
    ///
    /// The inner value is the raw 4-bit field read directly from the SCDC register. It is
    /// exposed so callers can log or diagnose misbehaving sinks, not because it carries
    /// semantic meaning — any value outside the defined set is treated as a protocol
    /// violation regardless of its numeric value.
    ///
    /// Not currently returned: no culvert method reads an FRL rate back from the sink.
    UnknownFrlRate(u8),
    /// The requested FFE levels exceed the maximum allowed at the requested FRL rate
    /// (see [`FfeLevels::max_for`](crate::FfeLevels::max_for)). A sink treats a
    /// prohibited value as 0, so it is rejected before anything is written.
    FfeLevelsOutOfRange {
        /// The requested FRL rate.
        rate: crate::FrlRate,
        /// The requested FFE levels.
        levels: u8,
    },
}

impl<E: fmt::Display> fmt::Display for ScdcError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(e) => write!(f, "SCDC transport error: {e}"),
            Self::Protocol(p) => write!(f, "refused by the SCDC protocol: {p}"),
        }
    }
}

impl<E: core::error::Error + 'static> core::error::Error for ScdcError<E> {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Transport(e) => Some(e),
            Self::Protocol(p) => Some(p),
        }
    }
}

impl<E: fmt::Display> fmt::Display for ReadModifyWriteError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(e) => write!(f, "read failed, nothing written: {e}"),
            Self::Write(e) => write!(f, "write failed after the read: {e}"),
        }
    }
}

impl<E: core::error::Error + 'static> core::error::Error for ReadModifyWriteError<E> {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Read(e) | Self::Write(e) => Some(e),
        }
    }
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownFrlRate(raw) => {
                write!(f, "the sink reported an undefined FRL rate value {raw:#x}")
            }
            Self::FfeLevelsOutOfRange { rate, levels } => {
                write!(f, "{levels} FFE levels exceed the maximum for {rate:?}")
            }
        }
    }
}

impl core::error::Error for ProtocolError {}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::string::ToString;

    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Bus;

    impl fmt::Display for Bus {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("bus gone")
        }
    }

    impl core::error::Error for Bus {}

    #[test]
    fn errors_print_compare_and_chain() {
        use core::error::Error;

        let transport = ScdcError::Transport(Bus);
        assert_eq!(transport.to_string(), "SCDC transport error: bus gone");
        assert_eq!(transport.source().unwrap().to_string(), "bus gone");

        let refused = ScdcError::<Bus>::Protocol(ProtocolError::UnknownFrlRate(0xF));
        assert_eq!(
            refused.to_string(),
            "refused by the SCDC protocol: the sink reported an undefined FRL rate value 0xf"
        );
        assert!(refused.source().is_some());

        let levels = ProtocolError::FfeLevelsOutOfRange {
            rate: crate::FrlRate::Rate12Gbps4Lanes,
            levels: 4,
        };
        assert!(
            levels
                .to_string()
                .starts_with("4 FFE levels exceed the maximum for ")
        );
        assert!(levels.source().is_none());

        let read = ReadModifyWriteError::Read(transport);
        let write = ReadModifyWriteError::Write(transport);
        assert_eq!(
            read.to_string(),
            "read failed, nothing written: SCDC transport error: bus gone"
        );
        assert_eq!(
            write.to_string(),
            "write failed after the read: SCDC transport error: bus gone"
        );
        assert!(write.source().is_some());
        let copied = read;
        assert_eq!(copied, read);
        assert_ne!(read, write);
    }
}
