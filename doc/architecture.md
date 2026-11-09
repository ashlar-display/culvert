# Architecture

## Role

Culvert implements the HDMI 2.1 SCDC (Status and Control Data Channel) protocol. It sits
on top of `hdmi-hal`'s `ScdcTransport` trait and provides typed, structured access to the
SCDC register map: named fields, bitfield structs, and typed operations for scrambling
control, FRL training primitives, and CED (Character Error Detection) reporting.

The relationship to `hdmi-hal` mirrors the relationship of piaf to its input bytes.
`ScdcTransport` moves raw bytes; culvert gives those bytes meaning. The transport is
injected — culvert implements the protocol logic, the caller provides the hardware.

Culvert is a protocol primitive library, not a policy layer. It provides the typed
operations that a link training state machine needs to call. The sequencing of those
operations — when to set the FRL rate, how long to wait for `FLT_Ready`, how to handle
timeout and retry — belongs in the link training crate above.

---

## Scope

Culvert covers:

- a typed SCDC register map: named constants, bitfield structs, and typed values for all
  SCDC-defined registers,
- the `Scdc<T>` client: wraps a `ScdcTransport` and exposes typed read/write
  methods for each register group,
- scrambling control: writing `TMDS_Config`, polling `Scrambler_Status`,
- FRL training primitives: writing `Config_1` (FRL rate, FFE levels) and `Config_0`,
  reading `Status_Flags_0` (`FLT_Ready`, lane lock), the per-lane `LTP` requests and
  `Source_Test_Configuration`, reading and clearing `Update_0` (`FLT_Update`, `FRL_Start`),
- CED reporting: reading the per-lane error counters (the `ERR_DET` block, in one
  transaction, with its checksum checked) and the Reed-Solomon correction count,
- version negotiation: reading `Sink_Version`, writing `Source_Version`,
- structured errors: transport errors and protocol-level violations (e.g. an undefined
  link training pattern request from the sink) surfaced as distinct variants.

The following are out of scope:

- **Async API** — the async client lives in the separate `culvert-async` crate, which
  shares culvert's register map through [`culvert::codec`](#the-codec-module). Culvert
  carries no async surface.
- **Link training state machine** — the sequencing of FRL training (rate selection loop,
  timeout handling, retry logic, fallback to TMDS) belongs in the link training crate.
  Culvert provides the register operations; the state machine decides when to call them.
- **InfoFrame encoding** — a separate crate in the signaling layer.
- **PHY configuration** — `HdmiPhy` operations are the link training crate's concern.
- **I²C / DDC transport** — platform backends implement `ScdcTransport` from `hdmi-hal`.
  Culvert never touches I²C directly.

---

## Dependencies

```
display-types  ─┐
hdmi-hal       ─┴─►  culvert  ──►  frl-training
```

- `display-types` — for `HdmiForumFrl`, the FRL rate enum used in `Config_1`.
- `hdmi-hal` — for the `ScdcTransport` trait.

Culvert does not depend on `piaf` or `concordance`. It is consumed by the link training
crate, which sequences culvert's operations according to the FRL training algorithm.
---

## The SCDC Register Map

SCDC is defined by HDMI 2.1. Registers are one byte wide, addressed
by a one-byte offset over DDC/I²C to the sink's SCDC address (0x54).

The register map divides into six functional groups. Bit positions are given per field;
see [Sources](#sources) for how each was established.

**Version** (0x01–0x02)
- `Sink_Version` (0x01, R) — SCDC protocol version supported by the sink.
- `Source_Version` (0x02, W) — SCDC protocol version the source intends to use.

**Update flags** (0x10)
- `Update_0` (0x10, R/W, write-1-to-clear) — change notification flags set by the sink:
  `Status_Update` (bit 0), `CED_Update` (bit 1), `RR_Test` (bit 2),
  `Source_Test_Update` (bit 3), `FRL_Start` (bit 4), `FLT_Update` (bit 5),
  `RSED_Update` (bit 6). The source reads and then clears these to detect sink-side
  state changes without polling every status register on every pass. `CED_Update` and
  `RSED_Update` are set when errors occur and stay set until the source clears them, and
  a sink may never set them (see "Character Error Detection" below). culvert treats
  `RR_Test` as a flag the source must not clear: `clear_update_flags` takes
  `ClearableUpdateFlags`, which has no field for it (see "`RR_Test` is not cleared"
  below — a provisional decision resting on one source).
- `Update_1` (0x11) — no fields are defined by the sources below; culvert does not
  access it.

**TMDS and scrambling** (0x20–0x21)
- `TMDS_Config` (0x20, W) — `Scrambling_Enable` (bit 0) and `TMDS_Bit_Clock_Ratio`
  (bit 1).
- `Scrambler_Status` (0x21, R) — `Scrambling_Status` (bit 0): sink acknowledgement that
  scrambling is active.

**FRL configuration** (0x30–0x35)
- `Config_0` (0x30, W) — `RR_Enable` (bit 0, read request enable) and
  `FLT_No_Retrain` (bit 1).
- `Config_1` (0x31, W) — `FRL_Rate` (bits 3:0, maps to `HdmiForumFrl`) and
  `FFE_Levels` (bits 7:4). Written by the source to request a training rate.
- `Source_Test_Configuration` (0x35, R) — written by the sink (compliance testing) to
  instruct the source: `TxFFE_Pre_Shoot_Only` (bit 1), `TxFFE_De_Emphasis_Only` (bit 2),
  `TxFFE_No_FFE` (bit 3), `FLT_No_Timeout` (bit 5), `DSC_FRL_Max` (bit 6) and `FRL_Max`
  (bit 7). Read by the source when `Source_Test_Update` is set.

**Status** (0x40–0x42)
- `Status_Flags_0` (0x40, R) — `Clock_Detected` (bit 0), `Ch0_Locked` (bit 1),
  `Ch1_Locked` (bit 2), `Ch2_Locked` (bit 3), `Ln3_Locked` (bit 4, FRL 4-lane only),
  `FLT_Ready` (bit 6, sink ready for link training), `DSC_Decode_Fail` (bit 7).
- `Status_Flags_1` (0x41, R) — link training pattern requested for lane 0 (bits 3:0)
  and lane 1 (bits 7:4).
- `Status_Flags_2` (0x42, R) — link training pattern requested for lane 2 (bits 3:0)
  and lane 3 (bits 7:4).

  LTP request values: 0x0 = no pattern (lane trained), 0x1 = all ones, 0x2 = all zeros,
  0x3 = Nyquist clock, 0x4 = DDE compliance pattern, 0x5–0x8 = LFSR 0–3,
  0xE = request FFE change, 0xF = request FRL rate change. Other values are undefined.

**Character Error Detection** (0x50–0x5A)
- `ERR_DET_0_L/H` (0x50/0x51), `ERR_DET_1_L/H` (0x52/0x53), `ERR_DET_2_L/H`
  (0x54/0x55) — per-lane 15-bit error counters with a validity bit (bit 7) in the high
  byte. Counters are read as a pair (low + high byte) to form a single `u16` value.
- `ERR_DET_Checksum` (0x56) — makes the bytes it covers sum to zero (mod 256): 0x50–0x56
  for TMDS and 3-lane FRL, 0x50–0x58 (lane 3 included) for 4-lane FRL.
- `ERR_DET_3_L/H` (0x57/0x58) — lane 3 counter, same format; only populated in FRL
  4-lane mode.
- `RS_Correction_L/H` (0x59/0x5A) — Reed-Solomon correction count (FRL), same format
  as a CED counter: validity bit (bit 7) in the high byte and a 15-bit count. No checksum
  covers it.

`read_ced` reads 0x50–0x58 as one block and checks the checksum for the link's lane count
(display-types' `HdmiLanes`, re-exported: `rate.lanes()`, three for TMDS); a mismatch is a warning, not an error (see "Error Handling"). The coverage
follows three implementations that agree: the Amlogic transmitter drivers, the Linux
kernel's SCDC helper (`drm_scdc_helper.c`, 2026) and edid-decode; a patent of LG's
supports the TMDS range. How the counters behave, from what public sources show (no
source defines all of it):

- They accumulate and saturate at 0x7FFF (a receiver's flag logic; an Intel errata calls
  7FFFh "full").
- They reset on power-off, on a hot plug or when the sink clears its SCDC registers. No
  source ties a reset to retraining or a rate change.
- Whether reading clears them is unknown: sources disagree. culvert's documentation
  therefore tells a caller to treat a reading lower than the previous one as a reset, and
  to count afresh after retraining, a rate change or a hot plug, which is right either
  way.
- `CED_Update` / `RSED_Update` are set by the sink and kept until the source clears them:
  the register is write-1-to-clear, and one receiver sets `CED_Update` when a counter
  rises by 4 or more in a second, or first saturates, and never clears it itself. When a
  sink sets them is not settled: one was seen with `CED_Update` set and every count zero
  (below), so a set flag does not mean there are counts. A receiver has shipped that never
  set `CED_Update` (an Intel errata). So a caller reads the counters regularly, takes the
  flag as a hint at most, and clears it before reading the counters, which is right
  however the sink sets it.
- No public source defines the validity bit beyond "valid"; a lane without it is `None`,
  unknown rather than zero.

Checked on hardware (October 2026; one sink, an LG ULTRAGEAR over amdgpu's DDC, TMDS,
scrambled): the whole map read as 32-byte block reads matched byte reads on every stable
register; the sink's checksum made 0x50–0x56 sum to zero with the three validity bits set
(0x80 × 3 + 0x80); `CED_Update` was set with every count zero, and once cleared was not
set again within five seconds on the error-free link. Why it was set is open: our own
reads cannot explain it, since the first CED read of the first run already read zero. The
counts stayed zero throughout, so clear on read and tearing could not be observed.

### Sources

The register map was originally written from a secondary summary and was wrong in
several places (FRL configuration in `Config_0`, shifted lock bits, `FRL_Start` and the
LTP requests in the wrong registers, lane 3 CED at 0x56/0x57). The map above was rebuilt
from open-source HDMI 2.1 implementations; every field was confirmed by at least two of
them (one behavioural rule, not clearing `RR_Test`, rests on a single source and has its
own section below):

- the AMD/Xilinx HDMI 2.1 receiver and transmitter drivers (`embeddedsw`,
  `v_hdmirx1/src/xv_hdmirx1_frl.c`: SCDC field table with address, mask and shift for
  every field; LTP values in `xv_hdmirx1_frl.h`; source-side training in
  `v_hdmitx1/src/xv_hdmitx1_frl.c`),
- AMD's display driver in mainline Linux (`dc_hdmi_types.h`, `link_hdmi_frl.c`),
- the Amlogic HDMI 2.1 transmitter driver (`hdmitx21/hw/hdmi_tx_ddc.h`) and a Realtek
  receiver driver (`hdmi_scdc.h`),
- the Linux DRM SCDC helper patch series for HDMI 2.1 fields (LKML, 2026-07, v8) and the
  Intel HDMI FRL enablement series for the `xe` driver (2026-08), including its FRL link
  training sequence. Both extend the mainline `drm_scdc.h`; the `xe` series additionally
  defines `Update_0` bit 7 (`LIP_Update`, HDMI 2.2) and FRL rates above 12 Gbps, which
  culvert does not decode yet.

`FFE_Levels` is the highest TxFFE level index the source supports: 0–3 for rates up to
12 Gbps and 0–7 for faster rates, per the Intel `xe` series (`drm_scdc_config_frl`) and
AMD's display driver (`hdmi_frl_get_max_ffe_level`). A sink treats a prohibited value as 0.

### `RR_Test` is not cleared (provisional)

**Rule.** culvert never clears `Update_0` bit 2, `RR_Test` (Read Request Test).
`clear_update_flags` (and `codec::encode_clear_update_flags`) takes `ClearableUpdateFlags`,
which has every update flag but `RR_Test`, so a clear that includes it cannot be written.
`read_update_flags` still reports it in `UpdateFlags`, and `UpdateFlags::clearable()`
turns the flags just read into a clear of everything else.

**Evidence.** Unlike the field layout above, this rule rests on a single source, and
another contradicts it:

| Source | What it does with `RR_Test` |
|---|---|
| Intel `xe` HDMI 2.1 series (2026-08), `drm_scdc_clear_update_flags` | States the rule — "Read Request Test is the only update flag the source cannot clear" — and returns `-EINVAL` if asked to clear it. |
| Amlogic HDMI 2.1 transmitter, `hdmitx21/hdmi_tx_scdc.c` | Clears it with the other HDMI 2.0 update flags it has read (`HDMI20_UPDATE_FLAGS & data`, which includes `READ_REQUEST_TEST`). |
| AMD display driver, `link_hdmi_frl.c` | Only ever clears specific FRL flags; no evidence either way. |
| AMD/Xilinx receiver, `xv_hdmirx1_frl.c` | Its SCDC field table has no `RR_Test`; no evidence either way. |
| Linux mainline `drm_scdc.h` | Defines the bit (`SCDC_READ_REQUEST_TEST`); has no clear helper. |

**Why culvert follows Intel.**
- It is the only source that states a rule; Amlogic's code may clear the bit only because
  it clears every flag it has seen, and a sink that owns the bit may ignore the write.
- The risks are asymmetric. If the rule holds, a source that clears `RR_Test` can break a
  sink's read-request test; if it does not, a source that leaves the flag alone loses
  nothing, as far as any source shows.
- Nothing in the stack needs to clear it: plumbob clears only `FLT_update`, `FRL_start`
  and `Source_Test_Update`.
- Nothing is dropped silently. culvert used to leave `rr_test` out of the write and
  return `Ok(())`, leaving the caller to believe the flag was cleared; with no field for
  it, the clear says exactly what is written.

**Why a separate type (October 2026).** A first version kept one `UpdateFlags` for reads
and clears and refused a clear with `rr_test` set (`ProtocolError::RrTestNotClearable`,
writing nothing). The second review of the FRL rework showed the cost: a caller that
clears what it has just read — the natural loop for an integration layer, a kernel driver
or firmware watching `Status_Update`, `CED_Update` and `FLT_update` after training — works
on every ordinary sink and fails on every clear once a sink sets `RR_Test`, which is to
say on compliance equipment, leaving `FLT_update` set. The bug would surface late, in
front of a tester. With a separate type it cannot be written: the compiler catches it,
`clearable()` names the one bit left out at the call site, and the refusal error is gone.

**Alternatives considered.** Following Amlogic and writing whatever the caller asks
(most literal register access, but lets a caller break an RR test if Intel is right); one
type for reads and clears with a run-time refusal (the first version, above); a helper
that strips `rr_test` from `UpdateFlags` (helps only callers who know to use it); keeping
the silent omission (rejected: no silent failures).

**Revisit when** any of these appears: a public source that states the rule for `RR_Test`; a second
implementation that states or contradicts the rule (for example, a clear helper in
mainline `drm_scdc`); a sink that keeps `RR_Test` set and appears to expect the source to
clear it; or a compliance report either way. Dropping the rule means adding an `rr_test`
field to `ClearableUpdateFlags` (non-breaking for code that reads its fields, as the type
is `#[non_exhaustive]`; `new` gains a parameter, which is breaking), encoding it in
`codec::encode_clear_update_flags`, and updating this section.

---

## The `Scdc<T>` Client

The central type is a thin client struct that owns the transport and exposes typed
methods grouped by register function:

```rust
pub struct Scdc<T> {
    transport: T,
}

impl<T: ScdcTransport> Scdc<T> {
    pub fn new(transport: T) -> Self;
    pub fn into_transport(self) -> T;

    // Version
    pub fn read_sink_version(&mut self) -> Result<u8, ScdcError<T::Error>>;
    pub fn write_source_version(&mut self, version: u8) -> Result<(), ScdcError<T::Error>>;

    // Scrambling
    pub fn write_tmds_config(&mut self, config: TmdsConfig) -> Result<(), ScdcError<T::Error>>;
    pub fn read_tmds_config(&mut self) -> Result<TmdsConfigReading, ScdcError<T::Error>>;
    /// Read-modify-write: the two bits from `config`, every other bit as read.
    pub fn update_tmds_config(&mut self, config: TmdsConfig)
        -> Result<(), ReadModifyWriteError<T::Error>>;
    pub fn read_scrambler_status(&mut self) -> Result<ScramblerStatus, ScdcError<T::Error>>;

    // FRL configuration
    pub fn write_config_0(&mut self, config: Config0) -> Result<(), ScdcError<T::Error>>;
    pub fn write_frl_config(&mut self, config: FrlConfig) -> Result<(), ScdcError<T::Error>>;
    pub fn read_source_test_config(&mut self) -> Result<SourceTestConfig, ScdcError<T::Error>>;

    // Status and update flags
    pub fn read_status_flags(&mut self) -> Result<StatusFlags, ScdcError<T::Error>>;
    pub fn read_ltp_requests(&mut self) -> Result<LtpRequests, ScdcError<T::Error>>;
    pub fn read_update_flags(&mut self) -> Result<UpdateFlags, ScdcError<T::Error>>;
    pub fn clear_update_flags(&mut self, flags: ClearableUpdateFlags) -> Result<(), ScdcError<T::Error>>;

    // CED
    pub fn read_ced(&mut self, lanes: HdmiLanes) -> Result<CedReading, ScdcError<T::Error>>;
    pub fn read_rs_correction(&mut self) -> Result<Option<RsCorrectionCount>, ScdcError<T::Error>>;
}
```

`Scdc<T>` holds no state beyond the transport. Register reads and writes are direct and
stateless from the client's perspective; any sequencing state lives in the caller.

Methods map to register groups, not necessarily individual registers. Three methods span
multiple registers, each read with one `ScdcTransport::read_block`:

- `read_ltp_requests()` reads `Status_Flags_1` and `Status_Flags_2` (0x41–0x42) and
  returns the pattern requested for each of the four lanes as one `LtpRequests` struct.
  The sink updates all lanes' requests together and signals the change once through
  `FLT_Update`.
- `read_ced(lanes)` reads the CED block (0x50–0x58: the four counters and the checksum)
  and returns a `CedReading`: the counters, with a warning if the checksum does not match
  or lane 3 reports a count at three lanes.
- `read_rs_correction()` reads `RS_Correction_L/H` (0x59–0x5A).

`hdmi-hal`'s `read_block` contract makes each one transaction, so the transport does not
split the registers; whether the sink holds them still during it is up to the sink. A
transport that reads one register at a time (`read_block_bytewise`) says so, and some bus
drivers split a block read on the wire anyway (the Synopsys DesignWare HDMI controller's
reads one byte at a time). The CED checksum is the one protection against a torn read
that holds on every transport, adapter and sink: it catches most torn CED reads, not all.
The LTP requests and the RS count have no checksum, so a torn read of them cannot be
detected; the transaction narrows the window, on a transport and adapter that keep it.
The tearing (0x00FF ticking to 0x0100 read back as 0x01FF) is kept as regression tests,
`tests/torn_reads.rs`. None of this is a multi-step sequence (write
rate, poll for ready, handle pattern request): those belong in the link training crate.

---


## The `codec` Module

`culvert::codec` holds the register map without any I/O: the address of every register
culvert accesses, `CED_BLOCK_START` and `CED_BLOCK_LEN` for the CED block, and one
function per register that encodes a typed value into a byte or decodes bytes into a typed
value — `encode_config_1`, `decode_ltp_requests`, `decode_ced_block`, and so on.
Protocol checks live there too: `encode_config_1` rejects FFE levels above the rate's
maximum as a `ProtocolError`. Decoding judges nothing it cannot judge: `decode_ltp_requests`
decodes every value, the undefined ones (0x9–0xD) as `LtpReq::Reserved(value)`, because
whether one matters depends on the lanes in use, which only the link training layer
knows. Nor does it withhold anything: `decode_ced_block` checks the checksum and returns
the decoded counters with a typed warning when it fails, leaving the decision to the
caller.

`Scdc<T>`'s methods only perform the reads and writes and hand the bytes to `codec`.
`culvert-async`'s client does the same with `.await`, so the sync and async clients share
one register map: an address or bit-layout fix in `codec` reaches both.

---

## Key Types

```rust
pub struct TmdsConfig {
    pub scrambling_enable: bool,
    pub high_tmds_clock_ratio: bool,  // false = /10, true = /40
}

pub struct ScramblerStatus {
    pub scrambling_active: bool,
    pub raw: u8,                      // the byte as read
}

/// `Config_0` (0x30).
pub struct Config0 {
    pub rr_enable: bool,        // bit 0: sink may raise read requests
    pub flt_no_retrain: bool,   // bit 1
}

/// FFE (Feed-Forward Equalization) levels written into `Config_1` bits[7:4]: the highest
/// TxFFE level index the source supports. `FfeLevels::new` rejects values above 7, and
/// `FfeLevels::max_for(rate)` gives the per-rate maximum (3 up to 12 Gbps, 7 above), which
/// `write_frl_config` enforces.
pub struct FfeLevels(u8);

/// `Config_1` (0x31).
pub struct FrlConfig {
    pub frl_rate: HdmiForumFrl,   // from display-types; bits[3:0]
    pub ffe_levels: FfeLevels,    // bits[7:4]
}

/// `Source_Test_Configuration` (0x35), written by the sink.
pub struct SourceTestConfig {
    pub txffe_pre_shoot_only: bool,     // bit 1
    pub txffe_de_emphasis_only: bool,   // bit 2
    pub txffe_no_ffe: bool,             // bit 3
    pub flt_no_timeout: bool,           // bit 5
    pub dsc_frl_max: bool,              // bit 6
    pub frl_max: bool,                  // bit 7
}

/// Link Training Pattern requested by the sink for one lane (a 4-bit field in
/// `Status_Flags_1`/`Status_Flags_2`). `LtpReq::value()` returns the 4-bit value.
pub enum LtpReq {
    None,              // 0x0: lane trained, no pattern requested
    AllOnes,           // 0x1
    AllZeros,          // 0x2
    NyquistClock,      // 0x3
    DdeCompliance,     // 0x4
    Lfsr0,             // 0x5
    Lfsr1,             // 0x6
    Lfsr2,             // 0x7
    Lfsr3,             // 0x8
    FfeChange,         // 0xE: sink requests an FFE level change
    RateChange,        // 0xF: sink requests a lower FRL rate
    Reserved(u8),      // 0x9–0xD: undefined by HDMI 2.1
}

/// Per-lane pattern requests from `Status_Flags_1` (0x41) and `Status_Flags_2` (0x42).
pub struct LtpRequests {
    pub lane0: LtpReq,
    pub lane1: LtpReq,
    pub lane2: LtpReq,
    pub lane3: LtpReq,   // LtpReq::None in 3-lane FRL
}

/// `Status_Flags_0` (0x40).
pub struct StatusFlags {
    pub clock_detected: bool,    // bit 0
    pub ch0_locked: bool,        // bit 1
    pub ch1_locked: bool,        // bit 2
    pub ch2_locked: bool,        // bit 3
    pub ln3_locked: bool,        // bit 4, FRL 4-lane only
    pub flt_ready: bool,         // bit 6: sink ready for link training
    pub dsc_decode_fail: bool,   // bit 7
}

/// `Update_0` (0x10), write-1-to-clear.
pub struct UpdateFlags {
    pub status_update: bool,        // bit 0
    pub ced_update: bool,           // bit 1
    pub rr_test: bool,              // bit 2
    pub source_test_update: bool,   // bit 3
    pub frl_start: bool,            // bit 4: training passed, source may start FRL
    pub flt_update: bool,           // bit 5: LTP requests changed
    pub rsed_update: bool,          // bit 6
}

/// The `Update_0` flags a source may clear: every flag but `RR_Test`.
/// `UpdateFlags::clearable()` gives the flags just read in this form.
pub struct ClearableUpdateFlags {
    pub status_update: bool,        // bit 0
    pub ced_update: bool,           // bit 1
    pub source_test_update: bool,   // bit 3
    pub frl_start: bool,            // bit 4
    pub flt_update: bool,           // bit 5
    pub rsed_update: bool,          // bit 6
}

/// A 15-bit character error count decoded from an ERR_DET register pair.
/// The high byte's bit 7 is the validity flag; the counter occupies bits[14:0].
/// `CedCount::value()` returns the raw count as a `u16` (always <= 0x7FFF).
pub struct CedCount(u16);

/// A 15-bit Reed-Solomon correction count from RS_Correction_L/H (0x59/0x5A), same
/// format as a CED counter.
pub struct RsCorrectionCount(u16);

pub struct CedCounters {
    pub lane0: Option<CedCount>,   // None if validity bit not set
    pub lane1: Option<CedCount>,
    pub lane2: Option<CedCount>,
    pub lane3: Option<CedCount>,   // None in TMDS / 3-lane FRL mode
}

/// The link's data lanes, re-exported from display-types (`FrlRate::lanes()`, total:
/// `NotSupported`, TMDS, is `Three`). With `Three` the checksum covers 0x50–0x56, with
/// `Four` 0x50–0x58.
pub use display_types::HdmiLanes; // enum HdmiLanes { Three, Four }

/// One CED read: the counters, decoded whatever the warnings say, and the warnings,
/// stored privately (`warnings()`, `iter_warnings()`).
pub struct CedReading {
    pub counters: CedCounters,
    // private: the warnings
}

pub enum CedWarning {
    /// The covered bytes do not sum to zero: most likely a torn read.
    ChecksumMismatch { expected: u8, found: u8 },
    /// Lane 3 reports a count with `HdmiLanes::Three`; `lane3` stays `None`.
    UnusedLaneReported { count: CedCount },
}
```

All output structs are `#[non_exhaustive]` for forward compatibility, and so are
`CedWarning` and each of its variants. `HdmiLanes` is display-types' and exhaustive: every
HDMI version that defines FRL has three or four lanes.

---

## Error Handling

Culvert surfaces two distinct failure categories:

```rust
#[non_exhaustive]
pub enum ScdcError<E> {
    /// The underlying I²C/DDC transport returned an error.
    Transport(E),
    /// The operation would violate the SCDC protocol (e.g. FFE levels the rate
    /// prohibits); nothing was written.
    Protocol(ProtocolError),
}

#[non_exhaustive]
pub enum ProtocolError {
    UnknownFrlRate(u8),   // reserved; no current method reads an FRL rate back
    FfeLevelsOutOfRange { rate: FrlRate, levels: u8 },
}
```

This mirrors the pattern established in piaf: transport failures and protocol violations
are distinct. A caller that only cares about transport health can match on `Transport(_)`;
`Protocol(_)` means culvert refused a request that would break the protocol. Every value a
sink can report decodes — an undefined link training request is `LtpReq::Reserved` — so
unexpected sink behaviour reaches the caller as data, not as an error.

Both enums are `#[non_exhaustive]` at the type level, consistent with the rest of the
stack. Tuple variants are plain — callers can match `UnknownFrlRate(value)` without `..`.

**Warnings.** One read finds something wrong without refusing anything: the CED checksum.
A mismatch is a `CedWarning` in the `CedReading`, next to counters decoded anyway, never
an error, following the stack's rule for checksums ("a checksum mismatch on decode is a
structured warning, not an error — the decoded struct is still returned and the caller
decides"), which cartouche follows for InfoFrames. An error would leave no counts at all
for a sink whose checksum does not conform, and the decoding layer judges nothing: whether
to distrust the counts, or read again, is the caller's. The warnings are stored privately
(`warnings()`, `iter_warnings()`), as the stack's warning storage rule requires, so a
later warning can be added without breaking anything; at most one of each kind occurs per
read. `ProtocolError` and `ScdcError` keep their meaning: requests culvert refuses, and
transport failures.

`ScdcError` is `#[non_exhaustive]` to allow future variants without a breaking change.

---

## The Culvert / Link Training Boundary

This boundary is worth stating explicitly because HDMI 2.1 describes SCDC protocol
mechanics and training algorithm steps together.

**Culvert's responsibility:** typed register access. Given a desired FRL rate, write it
into `Config_1`. Given a status register, decode it into `StatusFlags`. Culvert does not
know what to do with a `StatusFlags`; it only knows how to read one.

**Link training's responsibility:** the state machine. Receive a ranked list of FRL tiers
from concordance. For each tier: wait for `FLT_Ready`, write `Config_1`, then on each
`FLT_Update` read the per-lane `LTP` requests and act on them (send the pattern, change
FFE, or drop the rate) until every lane reports no pattern, then wait for `FRL_Start`. That sequencing logic, timeout
handling, and retry policy live in the link training crate — not here.

The rule: if it touches time, state across multiple register accesses, or fallback logic,
it belongs in link training. If it reads or writes registers and returns typed results, it
belongs in culvert.

---

## The `plumbob` Feature

culvert implements `plumbob::ScdcClient` for `Scdc<T>` behind a `plumbob` cargo feature.
This follows the same convention as `serde` feature flags in the ecosystem: the producing
crate reaches toward the consuming crate's trait, rather than the consumer depending on
the producer.

```sh
# a crate using both
cargo add culvert --features plumbob
cargo add plumbob
```

Each `ScdcClient` method calls one culvert method:

| `ScdcClient` method | culvert method | Register |
|---|---|---|
| `read_flt_ready` | `read_status_flags().flt_ready` | `Status_Flags_0` (0x40) bit 6 |
| `read_update_flags` | `read_update_flags` | `Update_0` (0x10), all seven flags |
| `clear_update_flags` | `clear_update_flags` | `Update_0` (0x10), write 1 to clear |
| `read_ltp_requests` | `read_ltp_requests` | `Status_Flags_1/2` (0x41/0x42) |
| `read_source_test_config` | `read_source_test_config` | `Source_Test_Configuration` (0x35) |
| `write_config_0_defaults` | `write_config_0(Config0::default())` | `Config_0` (0x30) |
| `write_frl_config` | `write_frl_config` | `Config_1` (0x31) |

plumbob's `ScdcClient` has no CED method: training never reads the counters, so a link
health monitor reads them from `Scdc<T>` directly (`read_ced`, `read_rs_correction`).

`From` impls in the feature-gated module convert culvert's types to plumbob's owned types
(`LtpReq`, `LtpRequests`, `UpdateFlags`, `SourceTestConfig`)
and plumbob's `ClearableUpdateFlags` and `FrlConfig` back for the writes. culvert's own types are
unchanged, so culvert means the same thing with or without the feature. plumbob's
`ScdcClient` types mirror culvert's field for field (the same names, every field of each
register, the same constructors and `#[non_exhaustive]`; each crate adds methods of its
own), so every field crosses the boundary and the `From`
impls are field-for-field copies, tested bit by bit. plumbob clears only the flags it acts
on (`Source_Test_Update`, `FRL_start`, `FLT_update`); culvert's `StatusFlags` has no
plumbob counterpart and stays available through `Scdc<T>` directly.
`write_frl_config` keeps culvert's own FFE-levels check, which plumbob's per-rate limit
already satisfies.

**Polling and stopping early.** `Scdc` holds no state and does not wait between calls.
plumbob's limits assume one poll every 2 ms by default, and plumbob's contract puts the
wait inside the two polled methods, `read_flt_ready` and `read_update_flags`. With
`Scdc<T>` that means an `ScdcClient` of your own that wraps it, waits in those two
methods and forwards the rest:

```rust
use culvert::{Scdc, ScdcError, ScdcTransport};
use plumbob::{ScdcClient, UpdateFlags};
use std::time::Duration;

/// `Scdc<T>` that waits the poll interval in the two methods plumbob polls.
struct Paced<T>(Scdc<T>);

impl<T: ScdcTransport> ScdcClient for Paced<T> {
    type Error = ScdcError<T::Error>;

    fn read_flt_ready(&mut self) -> Result<bool, Self::Error> {
        std::thread::sleep(Duration::from_millis(2)); // or the platform's timer
        ScdcClient::read_flt_ready(&mut self.0)
    }

    fn read_update_flags(&mut self) -> Result<UpdateFlags, Self::Error> {
        std::thread::sleep(Duration::from_millis(2));
        ScdcClient::read_update_flags(&mut self.0)
    }

    // … the other five methods forward without waiting
}
```

A wall-clock deadline or a cancel signal goes in the same two methods, following
plumbob's "Stopping early" rule (`plumbob::ScdcClient`): return one error of your own
(the wrapper's `Error` then wraps `ScdcError`), let every later call through, and never
fail a read made after a `write_frl_config` with `NotSupported` until a `write_frl_config`
with an FRL rate starts training again. Neither belongs in the transport: a wait there
applies to every register access, not only the polls, and a transport cannot tell the
polled reads from LTS:L's, so a deadline there can fail LTS:L itself and leave the sink in
FRL.

---

## `no_std` Compatibility

Culvert requires no allocator. All output types are stack-allocated structs. The
`ScdcError<E>` type requires no heap. `Scdc<T>` holds only the transport, which is
caller-owned.

The full API is available in bare `no_std` environments. CI builds the crate, with and
without the `plumbob` feature, for `thumbv7em-none-eabi`, a target without `std`.

---

## Design Principles

- **Typed access, not raw bytes.** Every register read returns a named struct, not a raw
  `u8`. Every register write takes a typed config, not a bit pattern. Culvert is the
  translation layer between the wire format and the rest of the stack.
- **Interface owned by the consumer.** The `ScdcClient` trait that culvert implements is
  defined in `plumbob`, not here. culvert implements the trait; it does not define it.
  This means the link training layer can swap culvert for any other `ScdcClient`
  implementation without touching culvert. The `plumbob` cargo feature gates the impl
  so culvert remains independently usable without the link training layer as a dependency.
- **Accuracy and completeness.** Every field culvert decodes is checked against the
  sources listed with the register map. No register is omitted because its consumer has
  not been built yet; registers not wrapped yet (`Update_1`, `Test_Config_0`,
  manufacturer identification) are tracked on the roadmap.
- **One register map.** Every address, bit position and protocol check is in `codec`,
  shared by the sync and async clients; the clients add only the I/O.
- **Stateless client, stateful caller.** `Scdc<T>` holds no protocol state. Sequencing,
  retry logic, and training state live in the caller. This keeps culvert fully testable
  in isolation — any sequence of register reads and writes can be exercised without
  simulating a training run.
- **Deterministic and testable.** The simulated transport pattern from `hdmi-hal` applies
  here: pre-load a register array, run culvert operations against it, assert on results.
  No hardware required.
- **Transport errors and protocol errors are distinct.** A caller should be able to tell
  whether a failure came from the I²C bus or from a request culvert refused.
- **Stack-ordered delivery.** The 0.1.0 scope is the register coverage needed by the
  link training crate. Everything else the standard defines is on the roadmap.
- **No unsafe code.** `#![forbid(unsafe_code)]`.
- **Attested releases.** Every release is published through a GitHub Actions workflow
  that signs the `.crate` package with [SLSA Build Level 2](https://slsa.dev) provenance.
  Verify with `gh attestation verify <file> --repo ashlar-display/culvert` (the release from
  before the move to the `ashlar-display` organization: see "Verifying releases" in the
  README).

