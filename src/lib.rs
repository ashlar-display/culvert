//! Typed access to the HDMI 2.1 SCDC (Status and Control Data Channel) register map.
//!
//! Culvert sits on top of [`hdmi_hal::scdc::ScdcTransport`] and provides named structs,
//! bitfield types, and typed operations for scrambling control, FRL training primitives,
//! and CED (Character Error Detection) reporting.
//!
//! The central type is [`Scdc`], a thin stateless client that wraps a transport
//! and exposes one typed method per register group. Sequencing of register operations —
//! rate selection, timeout handling, retry logic — belongs in the link training crate
//! above.
//!
//! The register map itself — addresses and per-register encoding and decoding — is the
//! I/O-free [`codec`] module, which `Scdc` and `culvert-async`'s client share.
#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod client;
pub mod codec;
mod error;
mod register;

pub use client::Scdc;
pub use error::{ProtocolError, ReadModifyWriteError, ScdcError};
/// The transport `Scdc<T>` is generic over, re-exported from `hdmi-hal`.
pub use hdmi_hal::scdc::ScdcTransport;
pub use register::{
    CedCount, CedCounters, CedReading, CedWarning, ClearableUpdateFlags, Config0, FfeLevels,
    FrlConfig, FrlRate, HdmiLanes, LtpReq, LtpRequests, RsCorrectionCount, ScramblerStatus,
    SourceTestConfig, StatusFlags, TmdsConfig, TmdsConfigReading, UpdateFlags,
};
