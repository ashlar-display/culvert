//! End-to-end tests for the `plumbob` feature: plumbob's link training through `Scdc<T>`,
//! against maquette's simulated FRL sink and PHY.
//!
//! The sink trains as the public receivers do, and the PHY's patterns and equalization reach
//! it on the line, so these tests check the whole chain: plumbob's sequencing, culvert's
//! encoding and decoding, and the register bits in between. Only what a sink does not do by
//! itself (undefined requests, a retrain it starts) is scripted, with scheduled presets or
//! the SCDC hook.
#![cfg(feature = "plumbob")]

use core::time::Duration;

use culvert::ScdcError;
use hdmi_hal::scdc::ScdcOverDdc;
use maquette::{
    Action, Actor, Bench, Ddc, DdcErrorKind, Effect, Env, Event, Fault, Filter, HdmiForumFrl,
    LaneLimits, LanePatterns, LinkLanes, LtpPattern, NoAcknowledgeSource, OpKind, Phy, PhyCall,
    ScdcHook, ScdcRegisters, SinkConfig, TxFfeLevel, When,
};
use plumbob::{
    FallbackReason, FfeLevels, FrlTrainer, TmdsExit, TrainingConfig, TrainingError, TrainingEvent,
    TrainingOutcome, TrainingWarning,
};

const UPDATE_0: u8 = 0x10;
const CONFIG_0: u8 = 0x30;
const CONFIG_1: u8 = 0x31;
const SOURCE_TEST_CONFIG: u8 = 0x35;
const STATUS_FLAGS_0: u8 = 0x40;
const LTP_REQ_01: u8 = 0x41;
const LTP_REQ_23: u8 = 0x42;

const SOURCE_TEST_UPDATE: u8 = 0x08;
const FRL_START: u8 = 0x10;
const FLT_UPDATE: u8 = 0x20;
const FLT_READY: u8 = 0x40;

const R12: HdmiForumFrl = HdmiForumFrl::Rate12Gbps4Lanes;
const R10: HdmiForumFrl = HdmiForumFrl::Rate10Gbps4Lanes;

/// An FRL sink taking up to 12 Gbps at 4 lanes.
const FRL_SINK: SinkConfig = SinkConfig::new().with_max_frl_rate(Some(R12));

type Trainer<'b> = FrlTrainer<culvert::Scdc<ScdcOverDdc<Ddc<'b>>>, Phy<'b>>;

/// plumbob's trainer over culvert's client over the bench's bus, and the bench's PHY.
fn trainer<'b>(bench: &'b Bench<'_>) -> Trainer<'b> {
    let scdc =
        culvert::Scdc::new(ScdcOverDdc::new(bench.ddc()).expect("the bench's bus serves SCDC"));
    FrlTrainer::new(scdc, bench.phy())
}

/// FFE levels 3, as culvert's tests trained. plumbob counts polls, not time: at the bench's
/// 90 µs a byte, its default 100 LTS:3 polls last about 36 ms, less than the sink's first
/// TxFFE attempt (45 ms at three levels), so tests that wait for the sink's own timeouts
/// poll longer.
fn ffe_3() -> TrainingConfig {
    let mut config = TrainingConfig::default();
    config.ffe_levels = FfeLevels::new(3).unwrap();
    config
}

/// Trains `rates` on `bench`, and returns the outcome.
fn train(bench: &Bench<'_>, rates: &[HdmiForumFrl], config: &TrainingConfig) -> TrainingOutcome {
    trainer(bench).train(rates, config).unwrap().outcome
}

/// Every value the source wrote to `Config_1`, in order.
fn config_1_writes(bench: &Bench<'_>) -> Vec<u8> {
    bench
        .transcript()
        .filter_map(|record| match record.event {
            Event::Register {
                reg: CONFIG_1,
                to,
                by: Actor::Source,
                ..
            } => Some(to),
            _ => None,
        })
        .collect()
}

/// The PHY calls that succeeded, in order.
fn phy_calls(bench: &Bench<'_>) -> Vec<PhyCall> {
    bench
        .transcript()
        .filter_map(|record| match record.event {
            Event::Phy {
                call,
                result: Ok(()),
                ..
            } => Some(call),
            _ => None,
        })
        .collect()
}

fn rates(bench: &Bench<'_>) -> Vec<HdmiForumFrl> {
    phy_calls(bench)
        .into_iter()
        .filter_map(|call| match call {
            PhyCall::SetFrlRate(rate) => Some(rate),
            _ => None,
        })
        .collect()
}

fn patterns(bench: &Bench<'_>) -> Vec<LanePatterns> {
    phy_calls(bench)
        .into_iter()
        .filter_map(|call| match call {
            PhyCall::SendLtp(patterns) => Some(patterns),
            _ => None,
        })
        .collect()
}

fn reg(bench: &Bench<'_>, reg: u8) -> u8 {
    bench.sink().scdc(reg).unwrap()
}

fn same(pattern: LtpPattern) -> LanePatterns {
    LanePatterns {
        lane0: Some(pattern),
        lane1: Some(pattern),
        lane2: Some(pattern),
        lane3: Some(pattern),
    }
}

/// The source's writes to `Update_0`, which clear the flags it saw. plumbob writes it once
/// before training, after taking the requests (it sends the patterns first), and after the
/// pass: a retrain the sink starts comes after the third write, and its pass after the
/// fourth, once plumbob has taken the retrain's requests.
fn update_0_writes() -> Filter {
    Filter::ANY
        .with_addr(0x54)
        .with_op(OpKind::Write)
        .with_first(UPDATE_0)
}

/// Schedules the sink to post `requests` (lanes 0–1, lanes 2–3) and `update_0` when `when`
/// comes: a sink's step it does not take by itself.
fn post_at(bench: &Bench<'_>, when: When, requests: [u8; 2], update_0: u8) {
    for (reg, value) in [
        (LTP_REQ_01, requests[0]),
        (LTP_REQ_23, requests[1]),
        (UPDATE_0, update_0),
    ] {
        bench
            .schedule_action(Action::Preset { reg, value }, when)
            .unwrap();
    }
}

#[test]
fn trains_through_scdc() {
    let bench = Bench::new(FRL_SINK, &[]).unwrap();
    bench.preset_scdc(CONFIG_0, 0xFF);
    // Lane 1 needs TxFFE level 1: the sink asks for it once its first attempt runs out.
    bench.set_lane_limits(LinkLanes {
        lane1: LaneLimits::NONE.with_min_tx_ffe(TxFfeLevel::new(1).unwrap()),
        ..LinkLanes::NONE
    });
    let mut config = ffe_3();
    config.ltp_polls = 1_000;

    let outcome = train(&bench, &[R12], &config);

    assert!(matches!(
        outcome,
        TrainingOutcome::Success {
            achieved_rate: R12,
            ..
        }
    ));
    // Config_0 cleared; Config_1 = 12 Gbps (6) with FFE levels 3 in bits 7:4.
    assert_eq!(reg(&bench, CONFIG_0), 0x00);
    assert_eq!(config_1_writes(&bench), [0x36]);
    // Every flag plumbob serviced was cleared.
    assert_eq!(
        reg(&bench, UPDATE_0) & (FRL_START | FLT_UPDATE | SOURCE_TEST_UPDATE),
        0
    );
    assert_eq!(rates(&bench), [R12]);
    assert!(patterns(&bench).contains(&LanePatterns {
        lane0: Some(LtpPattern::Lfsr0),
        lane1: Some(LtpPattern::Lfsr1),
        lane2: Some(LtpPattern::Lfsr2),
        lane3: Some(LtpPattern::Lfsr3),
    }));
    let raised = phy_calls(&bench)
        .into_iter()
        .rev()
        .find_map(|call| match call {
            PhyCall::AdjustEqualization(eq) => Some(eq),
            _ => None,
        })
        .unwrap();
    assert_eq!(raised.lane1.tx_ffe_level.value(), 1);
    assert_eq!(raised.lane0.tx_ffe_level.value(), 0);
    // The sink trained: its lanes are locked, FRL_start came from it.
    assert_eq!(reg(&bench, STATUS_FLAGS_0) & 0x1E, 0x1E);
}

#[test]
fn steps_down_when_the_sink_asks_for_a_lower_rate() {
    // A sink taking 10 Gbps at most: every lane asks for a lower rate at 12.
    let bench = Bench::new(SinkConfig::new().with_max_frl_rate(Some(R10)), &[]).unwrap();

    let outcome = train(&bench, &[R12, R10], &ffe_3());

    assert!(matches!(
        outcome,
        TrainingOutcome::Success {
            achieved_rate: R10,
            ..
        }
    ));
    // Config_1: 12 Gbps (6), then 10 Gbps (5), both with FFE levels 3.
    assert_eq!(config_1_writes(&bench), [0x36, 0x35]);
    assert_eq!(rates(&bench), [R12, R10]);
}

#[test]
fn a_timeout_returns_the_sink_to_tmds() {
    // Lanes that never pass, on a sink that never gives up: plumbob's LTS:3 times out.
    let bench = Bench::new(FRL_SINK.with_lts3_timeout(None), &[]).unwrap();
    bench.set_lane_limits(LinkLanes::all(
        LaneLimits::NONE.with_min_tx_ffe(TxFfeLevel::MAX),
    ));
    let mut config = ffe_3();
    config.ltp_polls = 5;

    let outcome = train(&bench, &[R12], &config);

    assert!(matches!(
        outcome,
        TrainingOutcome::FallbackRequired {
            reason: FallbackReason::TrainingTimeout,
            ..
        }
    ));
    // Config_1 written with 12 Gbps, then with FRL off.
    assert_eq!(config_1_writes(&bench), [0x36, 0x00]);
    assert_eq!(rates(&bench), [R12, HdmiForumFrl::NotSupported]);
}

#[test]
fn rates_exhausted_clears_the_pending_flt_update() {
    let bench = Bench::new(SinkConfig::new().with_max_frl_rate(Some(R10)), &[]).unwrap();

    let outcome = train(&bench, &[R12], &ffe_3());

    assert!(matches!(
        outcome,
        TrainingOutcome::FallbackRequired {
            reason: FallbackReason::RatesExhausted,
            ..
        }
    ));
    assert_eq!(config_1_writes(&bench), [0x36, 0x00]);
    assert_eq!(reg(&bench, UPDATE_0) & FLT_UPDATE, 0);
}

#[test]
fn flt_no_timeout_from_source_test_configuration() {
    let bench = Bench::new(FRL_SINK, &[]).unwrap();
    // The sink sets FLT_no_timeout (bit 5) and Source_Test_Update before training.
    bench.preset_scdc(SOURCE_TEST_CONFIG, 0x20);
    bench.preset_scdc(UPDATE_0, SOURCE_TEST_UPDATE);
    // FLT_ready arrives late: on the tenth read of Status_Flags_0.
    bench.preset_scdc(STATUS_FLAGS_0, 0);
    bench
        .schedule_action(
            Action::Preset {
                reg: STATUS_FLAGS_0,
                value: FLT_READY,
            },
            When::BeforeOp {
                filter: Filter::ANY.with_addr(0x54).with_first(STATUS_FLAGS_0),
                nth: 10,
            },
        )
        .unwrap();
    let mut config = ffe_3();
    config.flt_ready_polls = 2;

    let outcome = train(&bench, &[R12], &config);

    // FLT_ready arrives after the normal limit, which FLT_no_timeout suspends.
    assert!(matches!(
        outcome,
        TrainingOutcome::Success {
            achieved_rate: R12,
            ..
        }
    ));
    assert_eq!(reg(&bench, UPDATE_0) & SOURCE_TEST_UPDATE, 0);
}

/// Shows a request the sink did not make on the wire: `nibble` in `lane`'s place while the
/// register holds `when_held` there. The sink still trains on what it holds.
struct Undefined {
    lane: usize,
    nibble: u8,
}

impl ScdcHook for Undefined {
    fn read(&mut self, reg: u8, value: u8, _: &ScdcRegisters<'_>, _: &mut Env<'_>) -> u8 {
        let (at, shift) = (LTP_REQ_01 + (self.lane / 2) as u8, 4 * (self.lane % 2));
        if reg == at {
            (value & !(0x0F << shift)) | (self.nibble << shift)
        } else {
            value
        }
    }
}

/// Trains on `bench`, returning the warnings with the outcome.
fn train_with_warnings(
    bench: &Bench<'_>,
    rates: &[HdmiForumFrl],
) -> (TrainingOutcome, Vec<TrainingWarning>) {
    let trained = trainer(bench).train(rates, &ffe_3()).unwrap();
    let warnings = trained.iter_warnings().copied().collect();
    (trained.outcome, warnings)
}

#[test]
fn an_undefined_request_is_ignored_and_reported() {
    // The sink asks nothing of lane 1, and the wire shows 0x9 there while it asks for
    // patterns: plumbob ignores it, and training passes on the other lanes.
    let config = FRL_SINK.with_ltp_requests(LanePatterns {
        lane1: None,
        ..same(LtpPattern::Lfsr0)
    });
    let mut hook = Undefined { lane: 1, nibble: 0 };
    struct WhileRequesting<'h>(&'h mut Undefined);
    impl ScdcHook for WhileRequesting<'_> {
        fn read(&mut self, reg: u8, value: u8, regs: &ScdcRegisters<'_>, env: &mut Env<'_>) -> u8 {
            // Lane 0 asking for a pattern: the requesting round, not the pass.
            self.0.nibble = if regs.get(LTP_REQ_01) & 0x0F != 0 {
                0x9
            } else {
                0
            };
            self.0.read(reg, value, regs, env)
        }
    }
    let mut hook = WhileRequesting(&mut hook);
    let bench = Bench::builder(&[])
        .sink(config)
        .scdc_hook(&mut hook)
        .build()
        .unwrap();

    let (outcome, warnings) = train_with_warnings(&bench, &[R12]);

    assert!(matches!(
        outcome,
        TrainingOutcome::Success {
            achieved_rate: R12,
            ..
        }
    ));
    assert!(
        matches!(
            warnings.as_slice(),
            [TrainingWarning::UndefinedLtpRequest {
                lane: 1,
                value: 0x9,
                count: 1,
                in_use: true,
                ..
            }]
        ),
        "{warnings:?}"
    );
}

#[test]
fn a_three_lane_link_ignores_an_undefined_lane_3_nibble() {
    // Lane 3 is not in use at a 3-lane rate; the wire shows 0x9 there in every round.
    let rate = HdmiForumFrl::Rate6Gbps3Lanes;
    let mut hook = Undefined {
        lane: 3,
        nibble: 0x9,
    };
    let bench = Bench::builder(&[])
        .sink(FRL_SINK)
        .scdc_hook(&mut hook)
        .build()
        .unwrap();

    let (outcome, warnings) = train_with_warnings(&bench, &[rate]);

    assert!(matches!(
        outcome,
        TrainingOutcome::Success { achieved_rate, .. } if achieved_rate == rate
    ));
    assert!(
        matches!(
            warnings.as_slice(),
            [TrainingWarning::UndefinedLtpRequest {
                lane: 3,
                value: 0x9,
                count: 2,
                in_use: false,
                ..
            }]
        ),
        "{warnings:?}"
    );
}

#[test]
fn frl_start_together_with_flt_update_retrains() {
    // After the pass, the sink sets FRL_start and FLT_update at once, asking for LFSR 1
    // on every lane (scripted: a sink-started retrain); then it passes again.
    let bench = Bench::new(FRL_SINK, &[]).unwrap();
    post_at(
        &bench,
        When::AfterOp {
            filter: update_0_writes(),
            nth: 3,
        },
        [0x66, 0x66],
        FRL_START | FLT_UPDATE,
    );
    post_at(
        &bench,
        // plumbob clears FRL_start first (the fourth write), takes the requests, then clears
        // FLT_update (the fifth).
        When::AfterOp {
            filter: update_0_writes(),
            nth: 5,
        },
        [0x00, 0x00],
        FLT_UPDATE,
    );
    let mut trainer = trainer(&bench);
    let mut events = Vec::new();
    let outcome = trainer
        .train_with_events(&[R12], &ffe_3(), &mut |event| events.push(event))
        .unwrap()
        .outcome;

    assert!(matches!(
        outcome,
        TrainingOutcome::Success {
            achieved_rate: R12,
            ..
        }
    ));
    assert!(
        events.contains(&TrainingEvent::FrlStartWithRetrain),
        "{events:?}"
    );
    assert_eq!(
        reg(&bench, UPDATE_0) & (FRL_START | FLT_UPDATE),
        0,
        "every flag plumbob saw is cleared"
    );
    assert!(patterns(&bench).contains(&same(LtpPattern::Lfsr1)));
}

#[test]
fn an_io_error_mid_training_exits_to_tmds() {
    // A DDC NACK reading the lane requests in LTS:3: plumbob still runs LTS:L, which turns
    // FRL off in Config_1, returns the PHY to TMDS and clears the pending FLT_update.
    let bench = Bench::new(FRL_SINK, &[]).unwrap();
    bench
        .schedule(
            Fault::new(Effect::Fail(DdcErrorKind::NoAcknowledge(
                NoAcknowledgeSource::Data,
            )))
            .on(Filter::ANY.with_addr(0x54).with_first(LTP_REQ_01)),
        )
        .unwrap();
    let result = trainer(&bench).train(&[R12], &ffe_3());
    assert!(
        matches!(
            result,
            Err(TrainingError::Scdc {
                error: ScdcError::Transport(_),
                exit: TmdsExit::Exited,
                ..
            })
        ),
        "{result:?}"
    );
    assert_eq!(config_1_writes(&bench), [0x36, 0x00]);
    assert_eq!(rates(&bench), [R12, HdmiForumFrl::NotSupported]);
    assert_eq!(reg(&bench, UPDATE_0) & FLT_UPDATE, 0);
}

#[test]
fn a_retrain_resumes_from_no_pattern_at_register_level() {
    // After the pass, the sink asks lane 0 alone for LFSR 1 (scripted), then passes again.
    let bench = Bench::new(FRL_SINK, &[]).unwrap();
    post_at(
        &bench,
        When::AfterOp {
            filter: update_0_writes(),
            nth: 3,
        },
        [0x06, 0x00],
        FLT_UPDATE,
    );
    post_at(
        &bench,
        When::AfterOp {
            filter: update_0_writes(),
            nth: 4,
        },
        [0x00, 0x00],
        FLT_UPDATE,
    );

    let outcome = train(&bench, &[R12], &ffe_3());

    assert!(matches!(
        outcome,
        TrainingOutcome::Success {
            achieved_rate: R12,
            ..
        }
    ));
    assert_eq!(reg(&bench, UPDATE_0) & (FRL_START | FLT_UPDATE), 0);
    assert!(patterns(&bench).contains(&LanePatterns {
        lane0: Some(LtpPattern::Lfsr1),
        lane1: None,
        lane2: None,
        lane3: None,
    }));
}

#[test]
fn update_0_flags_plumbob_does_not_handle_are_left_set() {
    let bench = Bench::new(FRL_SINK, &[]).unwrap();
    // Status_Update, CED_Update, RR_Test and RSED_Update.
    bench.preset_scdc(UPDATE_0, 0x01 | 0x02 | 0x04 | 0x40);

    let outcome = train(&bench, &[R12], &ffe_3());

    assert!(matches!(
        outcome,
        TrainingOutcome::Success {
            achieved_rate: R12,
            ..
        }
    ));
    assert_eq!(reg(&bench, UPDATE_0), 0x47);
}

#[test]
fn the_ffe_maximum_is_limited_per_rate() {
    // FfeLevels::MAX is 7; up to 12 Gbps a sink allows 3. plumbob limits the levels before
    // culvert encodes them (culvert would reject 7 at these rates).
    for (rate, config_1) in [(R12, 0x36u8), (HdmiForumFrl::Rate3Gbps3Lanes, 0x31)] {
        let bench = Bench::new(FRL_SINK, &[]).unwrap();
        let mut config = TrainingConfig::default();
        config.ffe_levels = FfeLevels::MAX;
        let outcome = train(&bench, &[rate], &config);
        assert!(matches!(
            outcome,
            TrainingOutcome::Success { achieved_rate, .. } if achieved_rate == rate
        ));
        assert_eq!(config_1_writes(&bench), [config_1]);
    }
}

#[test]
fn a_whole_training_takes_bench_time() {
    // The sink matches patterns over its lock trigger: training is not instantaneous.
    let bench = Bench::new(FRL_SINK, &[]).unwrap();
    let start = bench.now();
    train(&bench, &[R12], &ffe_3());
    assert!(bench.now().saturating_duration_since(start) >= Duration::from_millis(2));
}
