# Testing Strategy

culvert's test suite is built around deterministic register-access tests. All tests run
against in-memory transport implementations; no real hardware is required at any point.

## Test structure

Tests are split between inline unit tests in each `src/client/` module and integration
tests in `tests/scdc.rs`.

### Unit tests (`src/client/`)

Each client module contains tests immediately below the code it covers. They use
`TestTransport`, a 256-byte register array backed transport defined in
`src/client/test_transport.rs`.

`TestTransport` has two constructors:

- `TestTransport::new()` — succeeds on all operations; used for happy-path tests.
- `TestTransport::failing_after(n)` — succeeds for the first `n` operations then returns
  `Err(())`; used to exercise every `?` error branch.
- `TestTransport::log()` — the first 16 operations, in order, so a test can pin which
  registers a method reads and writes, in what order and how often.

The single generic instantiation (`Scdc<TestTransport>`) is intentional: using one
concrete type avoids LLVM counting per-monomorphisation `?` branches as uncovered, which
would inflate the coverage denominator without corresponding tests.

Each register group has tests covering:

- **Encoding correctness** — every field of a write config maps to the correct bit
  position in the output register(s). One assertion per field, not per struct.
- **Decoding correctness** — every field of a read result is extracted from the correct
  bit position. Register values are crafted to isolate individual bits where necessary.
- **Protocol errors** — what culvert refuses produces the correct `ProtocolError`
  variant before anything is written: FFE levels above the rate's maximum. A clear of
  `Update_0` cannot ask for `RR_Test` (`ClearableUpdateFlags` has no field for it); a test
  reads `RR_Test` and `FLT_update` set and clears what it read, which writes `FLT_update`
  only. Undefined
  values the sink reports are not errors: each undefined `LtpReq` nibble (0x9–0xD) decodes
  to `LtpReq::Reserved` with its raw value.
- **Transport error propagation** — every read and write call site has a
  `TestTransport::failing_after(n)` test that triggers failure at that exact operation
  and asserts the error bubbles through as `ScdcError::Transport`.

The `register` module also contains unit tests for the `CedCount` newtype (validity bit
masking, 15-bit value preservation) and `UpdateFlags::new` field ordering.

`codec` tests `decode_ced_block` without a transport: the checksum valid and invalid for
three and four lanes, which bytes each lane count covers, `expected` being the checksum
that makes a block valid, the 0x00FF → 0x0100 torn read, lane 3 reported at three lanes,
both warnings at once and in order, readings comparing and printing only their warnings,
and, exhaustively over every checksum byte, both lane counts and lane 3 valid or not, that
a mismatch comes exactly when the covered bytes do not sum to zero with `expected` the
byte that makes them, and lane 3 is reported exactly when valid and not in use (the lane
count comes from display-types' `lanes()`, tested there). `register` checks that a
reading holds one warning of each kind, and that one more is a bug (`debug_assert`).
`TestTransport` records a `read_block` as one
`Op::ReadBlock(reg, len)`, counted once by `failing_after` (an empty one not at all), so a
test pins each multi-register method to one transaction.

### Torn reads (`tests/torn_reads.rs`)

The tearing, as regression tests: a sink whose lane 0 counter ticks while it is read,
with a checksum kept correct for its current value. Read one register at a time, 0x00FF
ticking to 0x0100 reads back as 0x01FF with a `ChecksumMismatch`. Read as a block from a
snapshotting sink, one that holds its registers still during the transaction (`Block`;
the `read_block` contract leaves that to the sink), it is 0x00FF with no warning. A sweep
over starting counts shows a counter ticking by one per byte never passes the checksum
(a larger jump can), and a block read from that sink always returns the starting count.

### Fuzzing (`fuzz/`)

Two `cargo fuzz` targets run the decoding against arbitrary register bytes: `status_flags`
(0x40–0x42) and `ced` (0x50–0x58, a tenth byte picking three or four lanes). `ced` checks
`read_ced` against the bytes, not against the codec it calls: each lane decoded from its
own two bytes (lane 3 only with four lanes), a checksum warning exactly when the covered
bytes do not sum to zero, with `found` the byte read and `expected` the byte that zeroes
the sum, `UnusedLaneReported` exactly at three lanes with lane 3 valid, carrying its
count, and no other warning, checksum first. The corpus seeds name what they cover
(valid blocks for three and four lanes, a torn lane 0, an unused lane 3, a 3-lane
checksum read as four lanes).

### Integration tests (`tests/scdc.rs`)

The integration tests use `SimulatedScdc`, a separate infallible transport
(`Error = Infallible`) that exercises the public API through the crate boundary. These
tests confirm that the full round-trip — pre-load registers, call an `Scdc` method,
assert on decoded output or written register state — works correctly when going through
`pub use` re-exports.

Coverage is complementary: unit tests cover all branches and error paths; integration
tests confirm end-to-end bit patterns for each register group with realistic multi-field
values.

### plumbob feature tests (`src/client/plumbob_client.rs`, `tests/plumbob.rs`)

When compiled with `--features plumbob`, two more sets of tests run:

- **Unit tests** call each method through the `plumbob::ScdcClient` trait against
  `TestTransport` and assert on register bits: every request value (and 0x9 passed on
  as `LtpReq::Reserved`), each flag clear writing only its own bit, `Config_1` encoding
  (including FRL off), culvert's FFE-levels check, and transport errors from every
  method.
- **End-to-end tests** run plumbob's `FrlTrainer` through `Scdc<T>` against a sink
  simulated at the register level: `FLT_ready` after N reads, request nibbles posted in
  `Status_Flags_1/2` with `FLT_update`, write-1-to-clear `Update_0`, and `Config_1`
  starting and ending training. They cover a full training run, a rate drop, a timeout and
  exhausted rates ending in TMDS, `FLT_no_timeout`, an undefined request ignored and
  returned as a `TrainingWarning`, a 3-lane link whose unused lane 3 reports an undefined
  value, `FRL_start` and `FLT_update` set together (the retrain wins, and
  `FrlStartWithRetrain` is recorded), a DDC NACK mid-LTS:3 still ending in LTS:L
  (`Config_1` back to 0, the PHY in TMDS, `FLT_update` cleared), a retrain resuming from
  no pattern, the `Update_0` flags plumbob does not handle left set, and the FFE maximum
  limited per rate in the `Config_1` byte. They check the whole chain — plumbob's
  sequencing, culvert's encoding, and the register bits in between.

CI's coverage run measures the default features and `--features plumbob` together, so
the feature's module is part of the coverage ratchet.

## Builds

CI runs the tests with and without the `plumbob` feature, on stable and on the MSRV, and
builds both for `thumbv7em-none-eabi`, which has no `std`. The README's example runs as a
doctest against a hidden sink that is ready for FRL training, has `FLT_update` pending and
reports errors on lane 0, so every branch of it runs and its writes are asserted; the `scdc` example
runs against its simulated transport.

## Coverage

CI measures line coverage with `cargo-llvm-cov`, over the default features and the
`plumbob` feature merged into one report. The baseline is stored in
`.coverage-baseline` (currently 100%); CI fails if coverage drops more than 0.1% below
it. New register coverage without tests will trip this.

## Philosophy

`Scdc<T>` runs identically against simulated and real `ScdcTransport` implementations.
A test that cannot run with an in-memory transport does not belong in this repository.
Hardware is never a test dependency.
