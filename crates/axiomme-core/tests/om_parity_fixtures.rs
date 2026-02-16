use std::{fs, path::PathBuf};

use axiomme_core::om::{
    BufferTokensInput, DEFAULT_OBSERVER_MAX_TOKENS_PER_BATCH, ObservationConfigInput,
    OmConfigInput, OmObservationChunk, OmRecord, OmScope, ProcessInputStepOptions,
    ProcessInputStepPlan, ReflectionAction, ReflectionConfigInput, ResolvedObservationConfig,
    ResolvedReflectionConfig, decide_observer_write_action, plan_process_input_step,
    plan_process_output_result, resolve_om_config, select_activation_boundary,
    select_reflection_action, should_trigger_reflector,
};
use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ParityFixture {
    #[serde(rename = "config_cases")]
    config: Vec<ConfigCaseFixture>,
    #[serde(rename = "reflector_trigger_cases")]
    reflector_trigger: Vec<ReflectorTriggerCaseFixture>,
    #[serde(rename = "reflection_action_cases")]
    reflection_action: Vec<ReflectionActionCaseFixture>,
    #[serde(rename = "observer_write_decision_cases")]
    observer_write_decision: Vec<ObserverWriteDecisionCaseFixture>,
    #[serde(rename = "activation_cases")]
    activation: Vec<ActivationCaseFixture>,
    #[serde(rename = "process_input_step_cases")]
    process_input_step: Vec<ProcessInputStepCaseFixture>,
    #[serde(rename = "process_output_result_cases")]
    process_output_result: Vec<ProcessOutputResultCaseFixture>,
}

#[derive(Debug, Deserialize)]
struct ConfigCaseFixture {
    name: String,
    input: ConfigInputFixture,
    expect: Option<ResolvedConfigExpectation>,
    expect_error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReflectorTriggerCaseFixture {
    name: String,
    observation_tokens: u32,
    threshold: u32,
    expected: bool,
}

#[derive(Debug, Deserialize)]
struct ReflectionActionCaseFixture {
    name: String,
    observation_tokens: u32,
    reflection_threshold: u32,
    buffer_activation: Option<f32>,
    block_after: Option<u32>,
    has_buffered_reflection: bool,
    is_buffering_reflection: bool,
    is_reflecting: bool,
    expected_action: ReflectionActionFixture,
}

#[derive(Debug, Deserialize)]
struct ObserverWriteDecisionCaseFixture {
    name: String,
    record_pending_tokens: u32,
    record_observation_tokens: u32,
    record_last_buffered_at_tokens: u32,
    observation_config: ObserverDecisionObservationConfigFixture,
    expected: ObserverWriteDecisionExpectation,
}

#[derive(Debug, Deserialize)]
struct ObserverDecisionObservationConfigFixture {
    message_tokens_base: u32,
    total_budget: Option<u32>,
    max_tokens_per_batch: Option<u32>,
    buffer_tokens: Option<u32>,
    buffer_activation: Option<f32>,
    block_after: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ObserverWriteDecisionExpectation {
    threshold: u32,
    #[serde(flatten)]
    trigger: ObserverWriteDecisionTriggerExpectation,
    #[serde(flatten)]
    action: ObserverWriteDecisionActionExpectation,
}

#[derive(Debug, Deserialize)]
struct ObserverWriteDecisionTriggerExpectation {
    threshold_reached: bool,
    interval_triggered: bool,
    block_after_exceeded: bool,
}

#[derive(Debug, Deserialize)]
struct ObserverWriteDecisionActionExpectation {
    should_run_observer: bool,
    should_activate_after_observer: bool,
}

#[derive(Debug, Deserialize)]
struct ActivationCaseFixture {
    name: String,
    activation_ratio: f32,
    message_threshold: u32,
    current_pending_tokens: u32,
    chunks: Vec<ActivationChunkFixture>,
    expected: ActivationBoundaryExpectation,
}

#[derive(Debug, Deserialize)]
struct ActivationChunkFixture {
    message_tokens: u32,
    observation_tokens: u32,
    message_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ActivationBoundaryExpectation {
    chunks_activated: usize,
    message_tokens_activated: u32,
    observation_tokens_activated: u32,
    activated_message_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ProcessInputStepCaseFixture {
    name: String,
    record: ProcessInputStepRecordFixture,
    observation_config: ObserverDecisionObservationConfigFixture,
    reflection_config: ProcessInputStepReflectionConfigFixture,
    options: ProcessInputStepOptionsFixture,
    expected: ProcessInputStepExpectationFixture,
}

#[derive(Debug, Deserialize)]
struct ProcessInputStepRecordFixture {
    pending_message_tokens: u32,
    observation_token_count: u32,
    last_buffered_at_tokens: u32,
    has_buffered_reflection: bool,
    is_reflecting: bool,
    is_buffering_reflection: bool,
}

#[derive(Debug, Deserialize)]
struct ProcessInputStepReflectionConfigFixture {
    observation_tokens: u32,
    buffer_activation: Option<f32>,
    block_after: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ProcessInputStepOptionsFixture {
    is_initial_step: bool,
    read_only: bool,
    has_buffered_observation_chunks: bool,
}

#[derive(Debug, Deserialize)]
struct ProcessInputStepExpectationFixture {
    #[serde(flatten)]
    observer: ProcessInputStepObserverExpectationFixture,
    #[serde(flatten)]
    reflection: ProcessInputStepReflectionExpectationFixture,
    #[serde(flatten)]
    transition: ProcessInputStepTransitionExpectationFixture,
}

#[derive(Debug, Deserialize)]
struct ProcessInputStepObserverExpectationFixture {
    should_activate_buffered_before_observer: bool,
    should_run_observer: bool,
    should_activate_buffered_after_observer: bool,
}

#[derive(Debug, Deserialize)]
struct ProcessInputStepReflectionExpectationFixture {
    #[serde(rename = "reflection_action")]
    action: Option<ReflectionActionFixture>,
    #[serde(rename = "reflection_command_present")]
    command_present: bool,
    #[serde(rename = "reflection_should_increment_trigger_count")]
    should_increment_trigger_count: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ProcessInputStepTransitionExpectationFixture {
    #[serde(rename = "reflection_next_is_reflecting")]
    next_is_reflecting: Option<bool>,
    #[serde(rename = "reflection_next_is_buffering_reflection")]
    next_is_buffering_reflection: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ProcessOutputResultCaseFixture {
    name: String,
    read_only: bool,
    unsaved_message_count: usize,
    expected_should_save_unsaved_messages: bool,
}

#[derive(Debug, Deserialize)]
struct ResolvedConfigExpectation {
    message_tokens_base: u32,
    observation_tokens: u32,
    total_budget: Option<u32>,
    buffer_tokens: Option<u32>,
    observation_buffer_activation: Option<f32>,
    observation_block_after: Option<u32>,
    reflection_buffer_activation: Option<f32>,
    reflection_block_after: Option<u32>,
    async_buffering_disabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ScopeFixture {
    Session,
    Thread,
    Resource,
}

#[derive(Debug, Deserialize, Default)]
struct ConfigInputFixture {
    scope: ScopeFixture,
    #[serde(default)]
    share_token_budget: bool,
    #[serde(default)]
    observation: ObservationConfigFixture,
    #[serde(default)]
    reflection: ReflectionConfigFixture,
}

#[derive(Debug, Deserialize, Default)]
struct ObservationConfigFixture {
    message_tokens: Option<u32>,
    max_tokens_per_batch: Option<u32>,
    buffer_tokens: Option<BufferTokensFixture>,
    buffer_activation: Option<f32>,
    block_after: Option<f32>,
}

#[derive(Debug, Deserialize, Default)]
struct ReflectionConfigFixture {
    observation_tokens: Option<u32>,
    buffer_activation: Option<f32>,
    block_after: Option<f32>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum BufferTokensFixture {
    Disabled,
    Absolute { value: u32 },
    Ratio { value: f64 },
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum ReflectionActionFixture {
    None,
    Buffer,
    Reflect,
}

impl Default for ScopeFixture {
    fn default() -> Self {
        Self::Thread
    }
}

impl From<ScopeFixture> for OmScope {
    fn from(value: ScopeFixture) -> Self {
        match value {
            ScopeFixture::Session => Self::Session,
            ScopeFixture::Thread => Self::Thread,
            ScopeFixture::Resource => Self::Resource,
        }
    }
}

impl From<BufferTokensFixture> for BufferTokensInput {
    fn from(value: BufferTokensFixture) -> Self {
        match value {
            BufferTokensFixture::Disabled => Self::Disabled,
            BufferTokensFixture::Absolute { value } => Self::Absolute(value),
            BufferTokensFixture::Ratio { value } => Self::Ratio(value),
        }
    }
}

impl From<ReflectionActionFixture> for ReflectionAction {
    fn from(value: ReflectionActionFixture) -> Self {
        match value {
            ReflectionActionFixture::None => Self::None,
            ReflectionActionFixture::Buffer => Self::Buffer,
            ReflectionActionFixture::Reflect => Self::Reflect,
        }
    }
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/parity_cases.json")
}

fn load_fixture() -> ParityFixture {
    let path = fixture_path();
    let raw = fs::read_to_string(&path).expect("fixture file must be readable");
    serde_json::from_str::<ParityFixture>(&raw).expect("fixture json must be valid")
}

fn approx_eq_opt(actual: Option<f32>, expected: Option<f32>) -> bool {
    match (actual, expected) {
        (None, None) => true,
        (Some(a), Some(b)) => (a - b).abs() <= 1e-6_f32,
        _ => false,
    }
}

fn build_observation_config(
    config: &ObserverDecisionObservationConfigFixture,
) -> ResolvedObservationConfig {
    ResolvedObservationConfig {
        message_tokens_base: config.message_tokens_base,
        total_budget: config.total_budget,
        max_tokens_per_batch: config
            .max_tokens_per_batch
            .unwrap_or(DEFAULT_OBSERVER_MAX_TOKENS_PER_BATCH),
        buffer_tokens: config.buffer_tokens,
        buffer_activation: config.buffer_activation,
        block_after: config.block_after,
    }
}

fn build_om_record(now: DateTime<Utc>, pending_tokens: u32, observation_tokens: u32) -> OmRecord {
    OmRecord {
        id: "record-1".to_string(),
        scope: OmScope::Session,
        scope_key: "session:s-1".to_string(),
        session_id: Some("s-1".to_string()),
        thread_id: None,
        resource_id: None,
        generation_count: 0,
        last_applied_outbox_event_id: None,
        origin_type: axiomme_core::om::OmOriginType::Initial,
        active_observations: String::new(),
        observation_token_count: observation_tokens,
        pending_message_tokens: pending_tokens,
        last_observed_at: None,
        current_task: None,
        suggested_response: None,
        last_activated_message_ids: Vec::new(),
        observer_trigger_count_total: 0,
        reflector_trigger_count_total: 0,
        is_observing: false,
        is_reflecting: false,
        is_buffering_observation: false,
        is_buffering_reflection: false,
        last_buffered_at_tokens: 0,
        last_buffered_at_time: None,
        buffered_reflection: None,
        buffered_reflection_tokens: None,
        buffered_reflection_input_tokens: None,
        reflected_observation_line_count: None,
        created_at: now,
        updated_at: now,
    }
}

fn build_process_input_step_record(
    case: &ProcessInputStepCaseFixture,
    now: DateTime<Utc>,
) -> OmRecord {
    let mut record = build_om_record(
        now,
        case.record.pending_message_tokens,
        case.record.observation_token_count,
    );
    record.last_buffered_at_tokens = case.record.last_buffered_at_tokens;
    record.is_reflecting = case.record.is_reflecting;
    record.is_buffering_reflection = case.record.is_buffering_reflection;
    record.buffered_reflection = case
        .record
        .has_buffered_reflection
        .then(|| "buffered".to_string());
    record
}

fn assert_process_input_step_expectation(
    case: &ProcessInputStepCaseFixture,
    actual: &ProcessInputStepPlan,
) {
    assert_eq!(
        actual.should_activate_buffered_before_observer,
        case.expected
            .observer
            .should_activate_buffered_before_observer,
        "{} should_activate_buffered_before_observer",
        case.name
    );
    assert_eq!(
        actual.should_run_observer, case.expected.observer.should_run_observer,
        "{} should_run_observer",
        case.name
    );
    assert_eq!(
        actual.should_activate_buffered_after_observer,
        case.expected
            .observer
            .should_activate_buffered_after_observer,
        "{} should_activate_buffered_after_observer",
        case.name
    );
    assert_eq!(
        actual.reflection_decision.is_some(),
        case.expected.reflection.action.is_some(),
        "{} reflection_decision presence",
        case.name
    );
    assert_eq!(
        actual
            .reflection_decision
            .as_ref()
            .and_then(|decision| decision.command.as_ref())
            .is_some(),
        case.expected.reflection.command_present,
        "{} reflection_command_present",
        case.name
    );

    if let (Some(expected_action), Some(decision)) = (
        case.expected.reflection.action,
        actual.reflection_decision.as_ref(),
    ) {
        assert_eq!(
            decision.action,
            ReflectionAction::from(expected_action),
            "{} reflection_action",
            case.name
        );
        if let Some(expected) = case.expected.reflection.should_increment_trigger_count {
            assert_eq!(
                decision.should_increment_trigger_count, expected,
                "{} reflection_should_increment_trigger_count",
                case.name
            );
        }
        if let Some(expected) = case.expected.transition.next_is_reflecting {
            assert_eq!(
                decision.next_is_reflecting, expected,
                "{} reflection_next_is_reflecting",
                case.name
            );
        }
        if let Some(expected) = case.expected.transition.next_is_buffering_reflection {
            assert_eq!(
                decision.next_is_buffering_reflection, expected,
                "{} reflection_next_is_buffering_reflection",
                case.name
            );
        }
    }
}

fn saturating_usize_to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[test]
fn config_parity_cases_match_expected_results() {
    let fixture = load_fixture();
    for case in fixture.config {
        let input = OmConfigInput {
            scope: case.input.scope.into(),
            share_token_budget: case.input.share_token_budget,
            observation: ObservationConfigInput {
                message_tokens: case.input.observation.message_tokens,
                max_tokens_per_batch: case.input.observation.max_tokens_per_batch,
                buffer_tokens: case.input.observation.buffer_tokens.map(Into::into),
                buffer_activation: case.input.observation.buffer_activation,
                block_after: case.input.observation.block_after,
            },
            reflection: ReflectionConfigInput {
                observation_tokens: case.input.reflection.observation_tokens,
                buffer_activation: case.input.reflection.buffer_activation,
                block_after: case.input.reflection.block_after,
            },
        };

        match (case.expect, case.expect_error) {
            (Some(expect), None) => {
                let resolved = resolve_om_config(input).unwrap_or_else(|err| {
                    panic!("{} expected success, got error: {err:?}", case.name)
                });
                assert_eq!(
                    resolved.observation.message_tokens_base, expect.message_tokens_base,
                    "{} message_tokens_base",
                    case.name
                );
                assert_eq!(
                    resolved.observation.max_tokens_per_batch,
                    DEFAULT_OBSERVER_MAX_TOKENS_PER_BATCH,
                    "{} max_tokens_per_batch",
                    case.name
                );
                assert_eq!(
                    resolved.reflection.observation_tokens, expect.observation_tokens,
                    "{} observation_tokens",
                    case.name
                );
                assert_eq!(
                    resolved.observation.total_budget, expect.total_budget,
                    "{} total_budget",
                    case.name
                );
                assert_eq!(
                    resolved.observation.buffer_tokens, expect.buffer_tokens,
                    "{} buffer_tokens",
                    case.name
                );
                assert!(
                    approx_eq_opt(
                        resolved.observation.buffer_activation,
                        expect.observation_buffer_activation
                    ),
                    "{} observation_buffer_activation",
                    case.name
                );
                assert_eq!(
                    resolved.observation.block_after, expect.observation_block_after,
                    "{} observation_block_after",
                    case.name
                );
                assert!(
                    approx_eq_opt(
                        resolved.reflection.buffer_activation,
                        expect.reflection_buffer_activation
                    ),
                    "{} reflection_buffer_activation",
                    case.name
                );
                assert_eq!(
                    resolved.reflection.block_after, expect.reflection_block_after,
                    "{} reflection_block_after",
                    case.name
                );
                assert_eq!(
                    resolved.async_buffering_disabled, expect.async_buffering_disabled,
                    "{} async_buffering_disabled",
                    case.name
                );
            }
            (None, Some(expect_error)) => {
                let err =
                    resolve_om_config(input).expect_err(&format!("{} expected error", case.name));
                assert_eq!(format!("{err:?}"), expect_error, "{}", case.name);
            }
            _ => panic!(
                "{} fixture must define exactly one of expect/expect_error",
                case.name
            ),
        }
    }
}

#[test]
fn reflector_trigger_parity_cases_match_expected_results() {
    let fixture = load_fixture();
    for case in fixture.reflector_trigger {
        let actual = should_trigger_reflector(case.observation_tokens, case.threshold);
        assert_eq!(actual, case.expected, "{}", case.name);
    }
}

#[test]
fn reflection_action_parity_cases_match_expected_results() {
    let fixture = load_fixture();
    for case in fixture.reflection_action {
        let actual = select_reflection_action(
            case.observation_tokens,
            case.reflection_threshold,
            case.buffer_activation,
            case.block_after,
            case.has_buffered_reflection,
            case.is_buffering_reflection,
            case.is_reflecting,
        );
        assert_eq!(
            actual,
            ReflectionAction::from(case.expected_action),
            "{}",
            case.name
        );
    }
}

#[test]
fn observer_write_decision_cases_match_expected_results() {
    let fixture = load_fixture();
    let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    for case in fixture.observer_write_decision {
        let mut record = build_om_record(
            now,
            case.record_pending_tokens,
            case.record_observation_tokens,
        );
        record.last_buffered_at_tokens = case.record_last_buffered_at_tokens;
        let observation_config = build_observation_config(&case.observation_config);
        let actual = decide_observer_write_action(&record, observation_config);
        assert_eq!(
            actual.threshold, case.expected.threshold,
            "{} threshold",
            case.name
        );
        assert_eq!(
            actual.threshold_reached, case.expected.trigger.threshold_reached,
            "{} threshold_reached",
            case.name
        );
        assert_eq!(
            actual.interval_triggered, case.expected.trigger.interval_triggered,
            "{} interval_triggered",
            case.name
        );
        assert_eq!(
            actual.block_after_exceeded, case.expected.trigger.block_after_exceeded,
            "{} block_after_exceeded",
            case.name
        );
        assert_eq!(
            actual.should_run_observer, case.expected.action.should_run_observer,
            "{} should_run_observer",
            case.name
        );
        assert_eq!(
            actual.should_activate_after_observer,
            case.expected.action.should_activate_after_observer,
            "{} should_activate_after_observer",
            case.name
        );
    }
}

#[test]
fn process_input_step_parity_cases_match_expected_results() {
    let fixture = load_fixture();
    let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    for case in fixture.process_input_step {
        let record = build_process_input_step_record(&case, now);
        let observation_config = build_observation_config(&case.observation_config);
        let reflection_config = ResolvedReflectionConfig {
            observation_tokens: case.reflection_config.observation_tokens,
            buffer_activation: case.reflection_config.buffer_activation,
            block_after: case.reflection_config.block_after,
        };
        let actual = plan_process_input_step(
            &record,
            observation_config,
            reflection_config,
            "2026-01-01T00:00:00Z",
            ProcessInputStepOptions {
                is_initial_step: case.options.is_initial_step,
                read_only: case.options.read_only,
                has_buffered_observation_chunks: case.options.has_buffered_observation_chunks,
            },
        );
        assert_process_input_step_expectation(&case, &actual);
    }
}

#[test]
fn process_output_result_parity_cases_match_expected_results() {
    let fixture = load_fixture();
    for case in fixture.process_output_result {
        let actual = plan_process_output_result(case.read_only, case.unsaved_message_count);
        assert_eq!(
            actual.should_save_unsaved_messages, case.expected_should_save_unsaved_messages,
            "{} should_save_unsaved_messages",
            case.name
        );
    }
}

#[test]
fn activation_boundary_parity_cases_match_expected_results() {
    let fixture = load_fixture();
    for case in fixture.activation {
        let chunks = case
            .chunks
            .iter()
            .enumerate()
            .map(|(idx, chunk)| OmObservationChunk {
                id: format!("chunk-{idx}"),
                record_id: "record-1".to_string(),
                seq: saturating_usize_to_u32(idx.saturating_add(1)),
                cycle_id: format!("cycle-{}", idx + 1),
                observations: format!("obs-{}", idx + 1),
                token_count: chunk.observation_tokens,
                message_tokens: chunk.message_tokens,
                message_ids: chunk.message_ids.clone(),
                last_observed_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
                created_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            })
            .collect::<Vec<_>>();

        let first = select_activation_boundary(
            &chunks,
            case.activation_ratio,
            case.message_threshold,
            case.current_pending_tokens,
        );
        let second = select_activation_boundary(
            &chunks,
            case.activation_ratio,
            case.message_threshold,
            case.current_pending_tokens,
        );
        assert_eq!(first, second, "{} deterministic result", case.name);
        assert_eq!(
            first.chunks_activated, case.expected.chunks_activated,
            "{} chunks_activated",
            case.name
        );
        assert_eq!(
            first.message_tokens_activated, case.expected.message_tokens_activated,
            "{} message_tokens_activated",
            case.name
        );
        assert_eq!(
            first.observation_tokens_activated, case.expected.observation_tokens_activated,
            "{} observation_tokens_activated",
            case.name
        );
        assert_eq!(
            first.activated_message_ids, case.expected.activated_message_ids,
            "{} activated_message_ids",
            case.name
        );
    }
}
