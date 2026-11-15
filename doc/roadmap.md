# Roadmap

SCDC registers HDMI 2.1 defines that are not wrapped
by culvert yet, plus the `Update_0` bit 7 (`LIP_Update`) that HDMI 2.2 adds. Methods for
these groups may be added in later releases. Additions to registers culvert already reads
follow at the end; each is additive. Planned releases driven by the rest of the stack come
first.

---

## Following hdmi-hal's PHY trait split

culvert re-exports hdmi-hal's `ScdcTransport`, so hdmi-hal's planned breaking release
(splitting its PHY trait by link mode, with new clock, delay and power traits) is a breaking
release of culvert as well, even though culvert's own API does not change. It carries
`read_tmds_config` (below).

The same hdmi-hal release adds `DdcBus`, one trait for the whole DDC bus, and bounds
`ScdcTransport::Error` by `DdcError`, whose `kind()` classifies a transport failure (no
acknowledgement, timeout, busy, unsupported, detached, other). culvert follows:

- **Re-exports** of `DdcError`, `DdcErrorKind` and `NoAcknowledgeSource` next to
  `ScdcTransport`, since an implementor of `ScdcTransport` now writes them.
- **`ScdcError::transport_kind()`**, additive: `Some(kind)` for a `Transport` error, `None`
  for a protocol violation. Callers generic over the SCDC client can then tell a sink that
  does not answer from a transient fault without reaching for the transport.
- An `ScdcTransport` over a `DdcBus` is `hdmi-hal`'s adapter `ScdcOverDdc`; culvert does
  not touch the DDC bus itself.

---

## Reading `TMDS_Config` (0x20)

culvert writes `TMDS_Config` (`write_tmds_config`) but cannot read it back. The TMDS
bring-up crate, [spiritlevel](https://github.com/ashlar-display/spiritlevel), plans a check,
after a reconnect, whether the sink still holds the configuration the link needs, which
takes a read: an additive `read_tmds_config`, returning the decoded bits and the whole
byte, with a `TMDS_Config` decoder in `codec` that also serves register dumps (see
"Read-back decoders" below). With it, `update_tmds_config`: a read-modify-write that sets
the two bits and keeps the others as read, saying which of its two transfers failed. Both
are on the development branch, built against hdmi-hal's next breaking release, and ship
in the release that follows it.

---

## `LIP_Update` (`Update_0` bit 7, HDMI 2.2)

HDMI 2.2 defines bit 7 of `Update_0` as `LIP_Update`; the Intel `xe` FRL series defines
the bit (`SCDC_LIP_UPDATE`). `decode_update_flags` reads bits 0–6 and ignores bit 7, so
`UpdateFlags` has no field for it, and `clear_update_flags` never clears it. A field (and
whatever registers the flag points at) will be added when culvert covers HDMI 2.2.

---

## `Update_1` (0x11)

None of the sources culvert's register map is based on defines a field in `Update_1`, so
culvert does not access it (`codec::UPDATE_0` notes this). It will be wrapped if a field
is defined.

---

## DSC status

`Status_Flags_0` bit 7 (`DSC_Decode_Fail`) is read by `read_status_flags`, and the
sink-written `DSC_FRL_Max` test flag by `read_source_test_config`. Any further DSC-related
registers will be wrapped once the DSC path in the link training crate requires them.

---

## `Test_Config_0` (0xC0)

The mainline Linux `drm_scdc.h` and the Amlogic transmitter driver define `Test_Config_0`:
`Test_Read_Request` (bit 7) and a delay (bits 6:0). It belongs to the read-request
mechanism, with `Config_0`'s `RR_Enable` and `Update_0`'s `RR_Test`. culvert writes
`RR_Enable` and decodes `RR_Test` (and refuses to clear it), but does not access
`Test_Config_0`; it will be wrapped when culvert supports read requests.

---

## Manufacturer identification (0xD0–0xDD)

HDMI 2.1 defines a range of SCDC registers for sink manufacturer OUI (0xD0–0xD2), device
identification (0xD3–0xDA), hardware and software revisions (0xDB–0xDD), and
manufacturer-specific data (0xDE onwards). These are not required for link
training and are deferred indefinitely. If they are ever needed, they would be
exposed through a separate `read_manufacturer_info()` method returning raw bytes
rather than typed fields, since the content is vendor-defined.

---

## One CED and RS read (0x50–0x5A)

`read_ced` reads 0x50–0x58 and `read_rs_correction` 0x59–0x5A, two transactions. A link
health monitor that polls both would save one with a single 11-byte read returning both,
as a new method and type. Not needed for correctness: the CED counters and the RS count
measure different things, so reading them at the same instant gains little. It will be
designed with the monitor. A wider "link snapshot" of 0x40–0x5A would fit in 32 bytes, but
reads the reserved registers 0x43–0x4F.

---

## Read-back decoders for register dumps

A dump tool reads the register map through `ScdcTransport::read_block` in parts (a
transport may limit the length; `hdmi-hal-i2c-dev`'s is 32 bytes) and decodes slices with
`codec`: `decode_ced_block(&[u8; 9])` already works on a slice of a dump. Decoders for the
registers culvert only writes are missing: `Config_0`, `Config_1` (rate and FFE levels;
`ProtocolError::UnknownFrlRate`, "not currently returned", would be returned there) and
`TMDS_Config`.

---

## Raw CED bytes in `CedReading`

`CedReading` carries the decoded counters and the warnings, not the nine bytes read. A
diagnostics consumer that wants them gets a new field: adding one to the
`#[non_exhaustive]` struct breaks nothing. Added when a consumer needs it.
