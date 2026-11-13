# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **The README names [spiritlevel](https://github.com/ashlar-display/spiritlevel)**, the
  TMDS bring-up crate, in "Stack position" and its diagram, where it said "TMDS bring-up
  (planned)"; the roadmap's `read_tmds_config` item names it too.
- **The README's Ashlar block links the stack's crate list** instead of naming every crate,
  so it no longer goes stale on crates.io when a crate is added.

### Internal

- **The IDE's project files are no longer tracked**: five `.idea/` files were committed
  before `.gitignore` listed the directory, and shipped in the published `.crate`.
- **`.gitignore` covers the fuzzer's crash artifacts** (`fuzz/artifacts`), as it already did
  its build and coverage output.

## [0.3.1] - 2026-11-03

### Changed

- **`display-types` requirement widened to `>=0.4.3, <0.6`**, so both 0.4.3 and 0.5 are
  accepted, as in hdmi-hal. The display-types items culvert uses are the same in both, so this
  is not a breaking change: an existing dependency on display-types 0.4 keeps resolving, and
  one on 0.5 now lines up with culvert's.

## [0.3.0] - 2026-10-25

### Breaking changes

- **`read_ced` takes the link's lane count and returns a `CedReading`**:
  `read_ced(lanes: HdmiLanes) -> Result<CedReading, ScdcError<_>>`, with display-types'
  `HdmiLanes` (re-exported). It reads the CED block
  (0x50–0x58) with one `read_block` and checks the checksum at 0x56, over 0x50–0x56 for
  three lanes (TMDS and 3-lane FRL) and 0x50–0x58 for four. Reading byte by byte, a counter
  the sink updated between its two bytes could read back as a value it never held (0x00FF
  ticking to 0x0100 as 0x01FF), and nothing said so. A mismatch is a `CedWarning` next to
  the counters, which are decoded anyway, not an error; so is a count for lane 3 at three
  lanes, which stays `None`. Migration: `read_ced(rate.lanes())` (TMDS: `HdmiLanes::Three`)
  and `reading.counters` where the counters were; decide what a warning means for you.
- **`codec::CED_REGISTERS` and `codec::decode_ced` are removed**, the unchecked path; use
  `codec::CED_BLOCK_START`, `CED_BLOCK_LEN` and `decode_ced_block`.
- **With the `plumbob` feature, `Scdc<T>` no longer implements a `read_ced`**: plumbob's
  `ScdcClient` no longer has one (training never read the counters). The `From`
  conversions of `CedCount` and `CedCounters` to plumbob's types are gone with it. Read
  CED from `Scdc<T>` directly. The feature now requires plumbob 0.4, whose `ScdcClient` it
  implements; move plumbob alongside.
- **Every `ScdcTransport` must implement `read_block`**: culvert requires the hdmi-hal
  release where it is required (0.6). A transport that cannot read a block in one
  transaction adds `use hdmi_hal::scdc::read_block_bytewise;` and implements it with that.
- **`read_ltp_requests` and `read_rs_correction` read their two registers with one
  `read_block`** instead of two `read`s, so they now depend on the transport's
  `read_block`: one that fails it fails these reads, and with the `plumbob` feature that
  fails link training (`read_ltp_requests` is polled in LTS:3). One transaction keeps the
  transport from splitting the two registers; whether the sink holds them still is up to
  the sink, and neither has a checksum, so a torn read of them cannot be detected.

### Added

- **`HdmiLanes`**, re-exported from display-types (`Three`, `Four`; `FrlRate::lanes()`
  gives it for every rate), **`CedReading`** (`counters`, and its
  warnings through `warnings()` and `iter_warnings()`, stored privately) and
  **`CedWarning`** (`ChecksumMismatch { expected, found }`, `UnusedLaneReported { count }`).
- **`codec::decode_ced_block`**, `codec::CED_BLOCK_START` and `codec::CED_BLOCK_LEN`.

### Changed

- **The CED and RS counters' documentation says how they behave**, as far as public
  sources show: they saturate at 0x7FFF; a sink may clear them when read, so a lower
  reading is a reset; count afresh after retraining, a rate change or a hot plug;
  `CED_Update` / `RSED_Update` are set by the sink and kept until the source clears them,
  a hint at most (`CED_Update` was seen set with every count zero), and may never be set,
  so read the counters regularly; a lane without
  a valid count is unknown, not zero. `clear_update_flags` says to clear only the flags
  read, and the CED and RS flags before reading the counters.

- **Part of Ashlar** — the README now opens with a block naming the
  [Ashlar](https://ashlar-display.org) stack and linking the other crates in it. Keywords
  are aligned across the stack, with `ashlar` and `hdmi` on every crate; `ashlar` is added,
  `embedded` is dropped in favour of more specific keywords. Categories gain
  `no-std::no-alloc`. The crate's `homepage` is now the Ashlar website.

- **The poll interval and a deadline belong in the polled methods, not the transport.**
  The `plumbob` adapter's documentation, the README and `doc/architecture.md` used to say
  to enforce plumbob's 2 ms poll interval in the transport. plumbob's `ScdcClient`
  contract puts it inside the two polled methods, `read_flt_ready` and
  `read_update_flags`: a wait in the transport applies to every register access, and a
  deadline there cannot tell the polled reads from LTS:L's, so it can fail LTS:L and leave
  the sink in FRL. `doc/architecture.md` now shows an `ScdcClient` that wraps `Scdc<T>`,
  waits in those two methods and forwards the rest, and maps plumbob's "Stopping early"
  rule onto it.
- The README and `doc/architecture.md` use `cargo add` instead of the stale `"0.1"`
  version pins.
- **Moved to the `ashlar-display` organization** — the repository is now
  `ashlar-display/culvert` (the old URLs redirect), and `repository` points there.
  Releases are attested and published from the new repository; 0.2.0, attested before the
  move, verifies with `--owner DracoWhitefire --signer-repo DracoWhitefire/culvert` (see
  "Verifying releases" in the README). The code of conduct is now the organization's,
  shared by every Ashlar repository; the repository's own copy is removed.

### Internal

- **The client tests check more of what they claim**, mirroring culvert-async's:
  transport errors are matched as `ScdcError::Transport`, not only as an error; a CED
  test gives every lane a distinct, asymmetric count, so a swapped byte or lane shows; the
  version tests use 0xA5 and 0x5A rather than 0x01; and `TestTransport` records the
  operations, so tests pin the registers `read_status_flags`, `read_ltp_requests`,
  `read_ced`, `read_rs_correction` and `clear_update_flags` touch, in order and count.
- The `FfeLevels` tests moved from the client module to the register module, where the
  type is, and the end-to-end FFE test is named for what it checks (plumbob limits the
  levels; culvert encodes them).

## [0.2.0] - 2026-10-17

### Breaking changes

- **`ScdcTransport::read` now takes `&self` instead of `&mut self`** (inherited from
  `hdmi-hal`). Any type you pass to `Scdc<T>` that implements `ScdcTransport` must
  change its `read` method receiver from `&mut self` to `&self`. If the implementation
  mutates state during reads (e.g. an operation counter), wrap those fields in `Cell`
  or `Mutex`.
- **SCDC register map corrected** (see *Fixed*); the typed API follows the corrected map:
  - `write_frl_config` writes `Config_1` (0x31). `FrlConfig::dsc_frl_max` is removed
    (`DSC_FRL_Max` is a sink-written flag, now read with `read_source_test_config`).
    `FfeLevels` is the highest TxFFE level index (0–7, `FfeLevels::new`,
    `FfeLevels::value`) instead of the `Ffe0`–`Ffe7` enum, and `write_frl_config`
    returns `ProtocolError::FfeLevelsOutOfRange` when it exceeds the maximum for the
    rate (3 up to 12 Gbps, 7 above).
  - `StatusFlags` decodes `Status_Flags_0` only: `ch0_locked`–`ch2_locked` and
    `ln3_locked` (bits 1–4) and `dsc_decode_fail` (bit 7). `cable_connected`,
    `ch3_locked`, `frl_start` and `ltp_req` are removed; `StatusFlags::new` takes the
    seven flags in bit order.
  - Link training pattern requests are read per lane with `read_ltp_requests`.
    `LtpReq` follows HDMI 2.1's values: LFSR 0–3 are 0x5–0x8, and `AllOnes`, `AllZeros`,
    `NyquistClock`, `DdeCompliance`, `FfeChange` and `RateChange` are added. Every
    value decodes: the undefined ones (0x9–0xD) are `LtpReq::Reserved(value)`, so a
    stray value on one lane (such as the unused lane 3 at a 3-lane rate) no longer fails
    the whole read; whether it matters is for the link training layer to decide.
    `ProtocolError::UnknownLtpReq` is removed, and `codec::decode_ltp_requests` returns
    `LtpRequests` rather than a `Result`. `LtpReq::value()` returns the 4-bit value.
  - `UpdateFlags` decodes all `Update_0` flags (`rr_test`, `source_test_update`,
    `frl_start`, `flt_update`, `rsed_update`); `frl_update` and `dsc_update` are
    removed, `Update_1` is no longer accessed, and `UpdateFlags::new` takes the seven
    flags in bit order. `clear_update_flags` takes the new `ClearableUpdateFlags` —
    every flag but `rr_test` — instead of `UpdateFlags`, and `codec::encode_clear_update_flags`
    is infallible: culvert treats Read Request Test as a flag the source must not clear,
    and used to leave it out of the write silently. `UpdateFlags::clearable()` turns
    flags just read into a clear of everything else, so a read-then-clear works on a sink
    that keeps `RR_Test` set. The rule comes from the Intel `xe` series alone, and
    Amlogic's driver clears the bit, so it is provisional; `doc/architecture.md` records
    the evidence, the reasoning and what would change it. `UpdateFlags` derives `Default`
    (no flag set), and both constructors and `clearable()` are `const fn`.
- **The `plumbob` feature implements plumbob's per-lane `ScdcClient`.** plumbob 0.1's
  interface (`read_training_status`, a single link training pattern request) could not
  work with the corrected register map. `Scdc<T>` now implements the one-method-per-register
  operation trait of plumbob's link training states (LTS:2 → LTS:3 → LTS:P):
  `read_flt_ready` (`Status_Flags_0` bit 6), `read_update_flags` and `clear_update_flags`
  (all `Update_0` flags; plumbob's `UpdateFlags` and `ClearableUpdateFlags` mirror
  culvert's, converted field for field), `read_ltp_requests` (per lane),
  `read_source_test_config` (all six bits, which plumbob records in its trace),
  `write_config_0_defaults`,
  `write_frl_config` (`Config_1`) and `read_ced`. culvert's types convert to plumbob's
  through `From` impls in the feature-gated module; culvert's own types are unchanged.
  `Scdc` does not wait between polls: the poll interval plumbob's limits assume (2 ms by
  default) belongs in the transport.
- **Minimum `hdmi-hal` version raised to 0.5**: `Scdc<T>` is bound by hdmi-hal 0.5's
  `ScdcTransport`, the version plumbob's training uses. It has the `read(&self)` receiver
  from hdmi-hal 0.4.0 (see above) and adds the `read_block` default method, so a
  transport written for hdmi-hal 0.4 only needs its dependency raised.
- **`display-types` updated to 0.4** — aligns with display-types 0.4 across the stack.
  `FrlRate` (the re-exported `HdmiForumFrl`) is now display-types 0.4's type, the one
  `hdmi-hal` and `plumbob` use.
- **`FrlConfig` is `#[non_exhaustive]`**, built with `FrlConfig::new(frl_rate, ffe_levels)`,
  as plumbob's mirror of it is; code outside culvert replaces `FrlConfig { .. }` with
  `new`. `LtpRequests::new` and `CedCounters::new` become `const fn`, and
  `LtpRequests::all_trained`'s documentation says that in 3-lane FRL only lanes 0–2 decide
  whether training passed (lane 3 may hold anything).

### Fixed

- **Wrong SCDC register map** — the map culvert implemented did not match the HDMI 2.1
  register layout. FRL rate and FFE levels were written to `Config_0` (0x30), whose bit 0
  enables sink read requests; lane-lock bits were shifted by one; `FRL_Start` and the
  link training pattern requests were read from the wrong registers, for one lane only,
  with the wrong pattern values; and lane 3's CED counter was read from 0x56/0x57, where
  0x56 is the CED checksum. The map was rebuilt from open-source HDMI 2.1
  implementations (Xilinx, AMD, Amlogic, Realtek and two Linux DRM patch series), with
  every field confirmed by at least two of them; see `doc/architecture.md`.

### Added

- **`LtpReq::from_value`** — the request for a raw 4-bit value (`None` above 0xF); every
  value has exactly one request, `Reserved` only for 0x9–0xD. The decoder uses it.
- `ScdcTransport` re-exported at the crate root, from `hdmi-hal`: the bound on `Scdc<T>`.
- **`culvert::codec`** — the register map without I/O: every register address culvert
  accesses (and the CED checksum), `CED_REGISTERS`, and one encode or decode function per
  register (`encode_tmds_config`, `decode_scrambler_status`, `decode_update_flags`,
  `encode_clear_update_flags`, `encode_config_0`, `encode_config_1`,
  `decode_source_test_config`, `decode_status_flags`, `decode_ltp_requests`, `decode_ced`,
  `decode_rs_correction`). `Scdc<T>` now only performs the reads and writes and uses these;
  `culvert-async` uses the same functions, so the sync and async clients share one
  register map.
- `write_config_0` and `Config0` (`RR_Enable`, `FLT_No_Retrain`).
- `read_source_test_config` and `SourceTestConfig` (`TxFFE_Pre_Shoot_Only`,
  `TxFFE_De_Emphasis_Only`, `TxFFE_No_FFE`, `FLT_No_Timeout`, `DSC_FRL_Max`, `FRL_Max`),
  with a `const fn` constructor in bit order and a default with no bit set. The
  `frl_max` and `dsc_frl_max` docs say what the reference drivers do with both set
  (Amlogic and NVIDIA act on neither, AMD trains at the maximum rate).
- `read_ltp_requests`, `LtpRequests` and `LtpRequests::all_trained`.
- `read_rs_correction` and `RsCorrectionCount` (Reed-Solomon correction count, 0x59/0x5A).
- `FfeLevels::max_for` and `ProtocolError::FfeLevelsOutOfRange`.

- **SLSA Build Level 2 provenance** — release artifacts are attested via
  `actions/attest-build-provenance` and verified with
  `gh attestation verify <file> --repo DracoWhitefire/culvert`.
- **Fuzz targets** — `cargo-fuzz` harnesses for `read_status_flags`,
  `read_ltp_requests` and `read_ced`; 60 s smoke runs on every PR/push, 1 h deep runs on a weekly
  schedule, with automatic corpus minimisation and corpus-update PRs.

### Internal

- **CI builds for a `no_std` target** — new `Build (no_std)` steps build the crate for
  `thumbv7em-none-eabi`, with and without the `plumbob` feature. Nothing checked a
  target without `std` before, so a dependency that enables `std` (as hdmi-hal did
  through `display-types`) went unnoticed.
  The publish workflow runs the same build steps.
- **Automated publish can be triggered by `release-tag`** — `publish.yml` gains a
  `workflow_dispatch` trigger. Tags pushed with `GITHUB_TOKEN` do not start push-triggered
  workflows, so `release-tag`'s "Trigger publish workflow" step
  (`gh workflow run publish.yml`) could not start a publish run. Dispatches against a
  non-tag ref (e.g. `main`) are skipped, so they cannot publish or create a release.
- **Publishing uses crates.io trusted publishing** — `publish.yml` obtains a short-lived
  token through OIDC (`rust-lang/crates-io-auth-action`, revoked at the end of the job, and
  short-lived regardless) instead of the long-lived `CARGO_REGISTRY_TOKEN` secret. The
  upload job runs in the `release` environment, which only `v*` tags can deploy to and
  which requires a maintainer's approval. Quality gates moved to a separate `verify` job,
  so no dependency build scripts run and no cache is restored where publishing credentials
  exist. The attested `.crate` is kept as a separate copy; after upload the job fails if
  crates.io's checksum differs from it, and that same copy is attached to the GitHub
  release. Re-running a partially failed publish reuses an existing GitHub release.
  `publish.yml` refuses tags that do not match the `Cargo.toml` version.
- **Release summary before approval** — `publish.yml` gains a `summary` job that shows the
  crate, version and commit, changes since the previous published release (ignoring
  pre-release or malformed tags, and flagging a baseline that is not on crates.io), changes
  to workflow, toolchain and build configuration, manifest changes, the changelog section,
  and the `.crate` sha256 with a command to reproduce it, on the run page while the publish
  job waits for approval. The publish job refuses to attest a package whose sha256 differs
  from the reviewed one.
- **`release-tag` no longer tags after a failed promotion** — `release-tag` now promotes
  `develop` to `main` through a `release-promote/v*` branch and PR. A failed merge of the
  promote PR used to go unnoticed, after which the previous `main` commit was tagged with
  the new version. The step now fails, and tagging requires `main` to hold exactly the
  promoted tree. It dispatches `publish.yml` with `actions: write` (it had only `read`),
  and it also accepts a squash-merged release commit (`Release vX.Y.Z (#N)`).
- **`release-tag` reports an unfinished fuzz run** — it still requires both CI and the fuzz
  smoke run to have passed on `develop`'s head, but a fuzz run that has not finished yet
  is now reported as such. It used to be reported as `not-found`, which is how the first
  0.2.0 attempt failed: release-tag was started right after the release PR merged.
- **Release workflows only run from `develop`** — `release-prep` and `release-tag` fail when
  started from another branch, so an outdated copy of the workflow is not run against
  `develop` by mistake.
- **`release-prep` no longer edits the README** — its "Update README.md dependency pin"
  step updated the `culvert` pin only, not the pins on other crates in the README and
  docs; the pins are now updated by hand with the rest.
- **`release-prep` leaves the fuzz dependencies alone** — refreshing `fuzz/Cargo.lock` ran
  a plain `cargo update`, which also upgraded every fuzz dependency inside the release
  commit. It now updates only `culvert`'s own entry; Dependabot proposes the rest.
- **No CI or fuzz run on promotion PRs** — CI and the fuzz workflow no longer run on pull
  requests into `main`. The only ones are `release-tag`'s `release-promote/*` PRs, whose
  content already passed both on `develop`; opened by `github-actions[bot]`, their runs
  would be held for approval and listed as `action_required` or failed.
- **Supply-chain hardening for CI** — every third-party action is pinned to a commit SHA
  of a release tag, with `dependabot.yml` keeping the pins (and both `Cargo.lock`s, the
  crate's and the fuzz crate's) current. The Rust toolchains themselves still follow
  `stable` and `nightly`. `cargo-semver-checks` is installed as a checksum-verified
  prebuilt binary (the version listed by the pinned `install-action`) instead of being
  restored from a cached `~/.cargo/bin`; `cargo-fuzz` is installed at a fixed version
  instead of the latest. The security audit also runs weekly.
- **README documents release provenance** — a badge shows whether crates.io requires trusted
  publishing for this crate, and "Verifying releases" now states which versions carry SLSA
  provenance (from 0.2.0, not every release as it previously claimed), which are uploaded
  through trusted publishing (from 0.2.0) or were uploaded with an API token (up to 0.1.2),
  and how to look up a version's publishing details on crates.io.

## [0.1.2] - 2026-04-05

### Changed

- **Minimum `hdmi-hal` version raised to 0.3.0**: culvert now requires
  `hdmi-hal >= 0.3.0`. Users who also depend on `plumbob` or `hdmi-hal-async`
  (both of which require 0.3.0) will no longer see a duplicate `hdmi-hal` copy
  in their dependency graph, and `ScdcTransport` is once again a single coherent
  type across the stack.

## [0.1.1] - 2026-04-04

### Added

- `ScramblerStatus::new(scrambling_active: bool)` — constructor required by `culvert-async`,
  which decodes the register and constructs this type outside the crate.
- `StatusFlags::new(...)` — constructor required by `culvert-async` for the same reason.
- `CedCounters::new(lane0, lane1, lane2, lane3)` — constructor required by `culvert-async`.
- `CedCount::new` is now `pub` (was `pub(crate)`); `culvert-async` needs to construct
  `CedCount` values when decoding `ERR_DET` registers.

## [0.1.0] - 2026-04-03

### Added

**Core SCDC client**

- `Scdc<T>` — stateless typed client that wraps an [`hdmi_hal::scdc::ScdcTransport`] and exposes
  one method per register group. Holds no protocol state; all sequencing and retry logic belongs
  in the caller.
- `Scdc::new(transport)` and `Scdc::into_transport()` for construction and unwrapping.

**Version registers**

- `read_sink_version()` — reads `Sink_Version` (0x01).
- `write_source_version(u8)` — writes `Source_Version` (0x02).

**TMDS scrambling**

- `write_tmds_config(TmdsConfig)` — writes `TMDS_Config` (0x20): `Scrambling_Enable` (bit 0) and
  `TMDS_Bit_Clock_Ratio` (bit 1).
- `read_scrambler_status()` — reads `Scrambler_Status` (0x21); returns `ScramblerStatus` with the
  sink's `scrambling_active` confirmation flag.

**FRL link training**

- `write_frl_config(FrlConfig)` — writes `Config_0` (0x30): `FRL_Rate` (bits 3:0), `DSC_FRL_Max`
  (bit 4), and `FFE_Levels` (bits 7:5).
- `read_status_flags()` — reads `Status_Flags_0` (0x40) and `Status_Flags_1` (0x41); returns
  `StatusFlags` covering clock detection, cable presence, per-lane symbol lock (lanes 0–3),
  `FLT_Ready`, `FRL_Start`, and the current `LtpReq`.

**Update flags**

- `read_update_flags()` — reads `Update_0` (0x10) and `Update_1` (0x11); returns `UpdateFlags`
  with `status_update`, `ced_update`, `frl_update`, and `dsc_update`.
- `clear_update_flags(UpdateFlags)` — write-1-to-clear: each flag set to `true` is cleared in the
  corresponding register; flags set to `false` are left unchanged.

**Character Error Detection**

- `read_ced()` — reads `ERR_DET` registers (0x50–0x57); returns `CedCounters` with per-lane
  `Option<CedCount>` values. A lane's counter is `None` when the high-byte validity bit is not
  set. Lane 3 is always `None` in TMDS or 3-lane FRL mode.

**Register types**

- `TmdsConfig` — `scrambling_enable: bool`, `high_tmds_clock_ratio: bool`.
- `ScramblerStatus` — `scrambling_active: bool`.
- `FrlConfig` — `frl_rate: FrlRate`, `dsc_frl_max: bool`, `ffe_levels: FfeLevels`.
- `FrlRate` — re-export of [`display_types::HdmiForumFrl`]; use `HdmiForumFrl::NotSupported` to
  clear FRL mode.
- `FfeLevels` — exhaustive enum `Ffe0`–`Ffe7` covering all 3-bit FFE level values.
- `LtpReq` — `None`, `Lfsr0`–`Lfsr3`; marked `#[non_exhaustive]` for forward compatibility.
- `StatusFlags` — decoded view of `Status_Flags_0/1`; marked `#[non_exhaustive]`.
- `UpdateFlags` — decoded view of `Update_0/1`; constructed via `UpdateFlags::new`; marked
  `#[non_exhaustive]`.
- `CedCount` — 15-bit newtype (`value() -> u16`); validity bit is consumed during decode and
  never exposed.
- `CedCounters` — `lane0`–`lane3: Option<CedCount>`; marked `#[non_exhaustive]`.

**Error types**

- `ScdcError<E>` — two-variant enum distinguishing `Transport(E)` (I²C/DDC bus error) from
  `Protocol(ProtocolError)` (register content outside the defined values); marked `#[non_exhaustive]`.
- `ProtocolError` — `UnknownFrlRate(u8)` and `UnknownLtpReq(u8)`; the inner `u8` is the raw
  register field for diagnostics only — any value outside the defined set is unconditionally a violation.
  Marked `#[non_exhaustive]`.

**`plumbob` feature**

- Implements [`plumbob::ScdcClient`] for `Scdc<T>`, bridging `write_frl_config`,
  `read_training_status`, and `read_ced` to the `plumbob` trait vocabulary. Enabled by
  `features = ["plumbob"]`.

**Safety and portability**

- `#![no_std]` — the crate is fully `no_std` compatible with no `alloc` requirement.
- `#![forbid(unsafe_code)]` — no unsafe code anywhere in the crate.
