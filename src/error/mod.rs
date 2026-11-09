//! Error types returned by [`crate::Scdc`] operations.

/// Errors returned by [`Scdc`](crate::Scdc) register operations.
///
/// Two categories of failure are distinguished so that callers can handle them
/// separately: a transport failure means the I²C/DDC bus returned an error; a protocol
/// error means culvert refused the operation because it would break the SCDC protocol
/// (for example FFE levels the rate prohibits), before anything was written.
#[non_exhaustive]
#[derive(Debug)]
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
#[derive(Debug)]
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
#[derive(Debug)]
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
