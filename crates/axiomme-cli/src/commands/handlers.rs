use anyhow::Result;
use axiomme_core::AxiomMe;
use axiomme_core::client::BenchmarkFixtureCreateOptions;
use axiomme_core::models::{
    BenchmarkGateOptions, BenchmarkRunOptions, EvalRunOptions, ReleaseGateBenchmarkGatePlan,
    ReleaseGateBenchmarkRunPlan, ReleaseGateEvalPlan, ReleaseGateOperabilityPlan,
    ReleaseGatePackOptions, ReleaseGateReplayPlan, ReleaseSecurityAuditMode,
};

use crate::cli::{
    BenchmarkCommand, BenchmarkFixtureCommand, EvalCommand, EvalGoldenCommand, RelationCommand,
    ReleaseCommand, ReleaseSecurityAuditModeArg, SecurityAuditModeArg, SecurityCommand,
    SessionCommand, TraceCommand,
};

use super::print_json;

pub(super) fn handle_session(app: &AxiomMe, command: SessionCommand) -> Result<()> {
    match command {
        SessionCommand::Create { id } => {
            let session = app.session(id.as_deref());
            session.load()?;
            println!("{}", session.session_id);
        }
        SessionCommand::Add { id, role, text } => {
            let session = app.session(Some(&id));
            session.load()?;
            let message = session.add_message(&role, text)?;
            print_json(&message)?;
        }
        SessionCommand::Commit { id } => {
            let session = app.session(Some(&id));
            session.load()?;
            let result = session.commit()?;
            print_json(&result)?;
        }
        SessionCommand::List => {
            let sessions = app.sessions()?;
            print_json(&sessions)?;
        }
        SessionCommand::Delete { id } => {
            let deleted = app.delete(&id)?;
            println!("{deleted}");
        }
    }
    Ok(())
}

fn run_benchmark_fixture_command(app: &AxiomMe, command: BenchmarkFixtureCommand) -> Result<()> {
    match command {
        BenchmarkFixtureCommand::Create {
            name,
            query_limit,
            include_golden,
            include_trace,
            include_stress,
            trace_expectations,
        } => {
            let summary = app.create_benchmark_fixture(
                &name,
                BenchmarkFixtureCreateOptions {
                    query_limit,
                    include_golden,
                    include_trace,
                    include_stress,
                    trace_expectations,
                },
            )?;
            print_json(&summary)?;
        }
        BenchmarkFixtureCommand::List { limit } => {
            let fixtures = app.list_benchmark_fixtures(limit)?;
            print_json(&fixtures)?;
        }
    }
    Ok(())
}

pub(super) fn handle_trace(app: &AxiomMe, command: TraceCommand) -> Result<()> {
    match command {
        TraceCommand::Requests {
            limit,
            operation,
            status,
        } => {
            let logs =
                app.list_request_logs_filtered(limit, operation.as_deref(), status.as_deref())?;
            print_json(&logs)?;
        }
        TraceCommand::List { limit } => {
            let traces = app.list_traces(limit)?;
            print_json(&traces)?;
        }
        TraceCommand::Get { trace_id } => {
            let trace = app.get_trace(&trace_id)?;
            print_json(&trace)?;
        }
        TraceCommand::Replay { trace_id, limit } => {
            let replay = app.replay_trace(&trace_id, limit)?;
            print_json(&replay)?;
        }
        TraceCommand::Stats {
            limit,
            include_replays,
        } => {
            let stats = app.trace_metrics(limit, include_replays)?;
            print_json(&stats)?;
        }
        TraceCommand::Snapshot {
            limit,
            include_replays,
        } => {
            let snapshot = app.create_trace_metrics_snapshot(limit, include_replays)?;
            print_json(&snapshot)?;
        }
        TraceCommand::Snapshots { limit } => {
            let snapshots = app.list_trace_metrics_snapshots(limit)?;
            print_json(&snapshots)?;
        }
        TraceCommand::Trend {
            limit,
            request_type,
        } => {
            let trend = app.trace_metrics_trend(limit, request_type.as_deref())?;
            print_json(&trend)?;
        }
        TraceCommand::Evidence {
            trace_limit,
            request_limit,
            enforce,
        } => {
            let report = app.collect_operability_evidence(trace_limit, request_limit)?;
            print_json(&report)?;
            if enforce && !report.passed {
                anyhow::bail!("operability evidence checks failed");
            }
        }
    }
    Ok(())
}

pub(super) fn handle_relation(app: &AxiomMe, command: RelationCommand) -> Result<()> {
    match command {
        RelationCommand::List { owner_uri } => {
            let relations = app.relations(&owner_uri)?;
            print_json(&serde_json::json!({
                "status": "ok",
                "owner_uri": owner_uri,
                "relations": relations
            }))?;
        }
        RelationCommand::Link {
            owner_uri,
            relation_id,
            uris,
            reason,
        } => {
            let relation = app.link(&owner_uri, &relation_id, uris, &reason)?;
            print_json(&serde_json::json!({
                "status": "ok",
                "owner_uri": owner_uri,
                "relation": relation
            }))?;
        }
        RelationCommand::Unlink {
            owner_uri,
            relation_id,
        } => {
            let removed = app.unlink(&owner_uri, &relation_id)?;
            print_json(&serde_json::json!({
                "status": "ok",
                "owner_uri": owner_uri,
                "relation_id": relation_id,
                "removed": removed
            }))?;
        }
    }
    Ok(())
}

pub(super) fn handle_eval(app: &AxiomMe, command: EvalCommand) -> Result<()> {
    match command {
        EvalCommand::Run {
            trace_limit,
            query_limit,
            search_limit,
            include_golden,
            golden_only,
        } => {
            let report = app.run_eval_loop_with_options(&EvalRunOptions {
                trace_limit,
                query_limit,
                search_limit,
                include_golden,
                golden_only,
            })?;
            print_json(&report)?;
        }
        EvalCommand::Golden { command } => match command {
            EvalGoldenCommand::List => {
                let cases = app.list_eval_golden_queries()?;
                print_json(&cases)?;
            }
            EvalGoldenCommand::Add {
                query,
                target,
                expected_top,
            } => {
                let result =
                    app.add_eval_golden_query(&query, target.as_deref(), expected_top.as_deref())?;
                print_json(&result)?;
            }
            EvalGoldenCommand::MergeFromTraces {
                trace_limit,
                max_add,
            } => {
                let result = app.merge_eval_golden_from_traces(trace_limit, max_add)?;
                print_json(&result)?;
            }
        },
    }
    Ok(())
}

pub(super) fn handle_benchmark(app: &AxiomMe, command: BenchmarkCommand) -> Result<()> {
    match command {
        BenchmarkCommand::Run {
            query_limit,
            search_limit,
            include_golden,
            include_trace,
            include_stress,
            trace_expectations,
            fixture_name,
        } => {
            let options = BenchmarkRunOptions {
                query_limit,
                search_limit,
                include_golden,
                include_trace,
                include_stress,
                trace_expectations,
                fixture_name,
            };
            let report = app.run_benchmark_suite(&options)?;
            print_json(&report)?;
        }
        BenchmarkCommand::Amortized {
            query_limit,
            search_limit,
            include_golden,
            include_trace,
            include_stress,
            trace_expectations,
            fixture_name,
            iterations,
        } => {
            let options = BenchmarkRunOptions {
                query_limit,
                search_limit,
                include_golden,
                include_trace,
                include_stress,
                trace_expectations,
                fixture_name,
            };
            let report = app.run_benchmark_suite_amortized(options, iterations)?;
            print_json(&report)?;
        }
        BenchmarkCommand::List { limit } => {
            let reports = app.list_benchmark_reports(limit)?;
            print_json(&reports)?;
        }
        BenchmarkCommand::Trend { limit } => {
            let trend = app.benchmark_trend(limit)?;
            print_json(&trend)?;
        }
        BenchmarkCommand::Gate {
            threshold_p95_ms,
            min_top1_accuracy,
            min_stress_top1_accuracy,
            gate_profile,
            max_p95_regression_pct,
            max_top1_regression_pct,
            window_size,
            required_passes,
            record,
            write_release_check,
            enforce,
        } => {
            let result = app.benchmark_gate_with_options(BenchmarkGateOptions {
                gate_profile,
                threshold_p95_ms,
                min_top1_accuracy,
                min_stress_top1_accuracy,
                max_p95_regression_pct,
                max_top1_regression_pct,
                window_size,
                required_passes,
                record,
                write_release_check,
            })?;
            print_json(&result)?;
            if enforce && !result.passed {
                anyhow::bail!("benchmark gate failed");
            }
        }
        BenchmarkCommand::Fixture { command } => run_benchmark_fixture_command(app, command)?,
    }
    Ok(())
}

pub(super) fn handle_security(app: &AxiomMe, command: SecurityCommand) -> Result<()> {
    match command {
        SecurityCommand::Audit {
            workspace_dir,
            mode,
            enforce,
        } => {
            let mode = match mode {
                SecurityAuditModeArg::Offline => "offline",
                SecurityAuditModeArg::Strict => "strict",
            };
            let report = app.run_security_audit_with_mode(workspace_dir.as_deref(), Some(mode))?;
            print_json(&report)?;
            if enforce && !report.passed {
                anyhow::bail!("security audit failed");
            }
        }
    }
    Ok(())
}

pub(super) fn handle_release(app: &AxiomMe, command: ReleaseCommand) -> Result<()> {
    match command {
        ReleaseCommand::Pack {
            workspace_dir,
            replay_limit,
            replay_max_cycles,
            trace_limit,
            request_limit,
            eval_trace_limit,
            eval_query_limit,
            eval_search_limit,
            benchmark_query_limit,
            benchmark_search_limit,
            benchmark_threshold_p95_ms,
            benchmark_min_top1_accuracy,
            benchmark_min_stress_top1_accuracy,
            benchmark_max_p95_regression_pct,
            benchmark_max_top1_regression_pct,
            benchmark_window_size,
            benchmark_required_passes,
            security_audit_mode,
            enforce,
        } => {
            let security_audit_mode = match security_audit_mode {
                ReleaseSecurityAuditModeArg::Offline => ReleaseSecurityAuditMode::Offline,
                ReleaseSecurityAuditModeArg::Strict => ReleaseSecurityAuditMode::Strict,
            };
            let report = app.collect_release_gate_pack(&ReleaseGatePackOptions {
                workspace_dir,
                replay: ReleaseGateReplayPlan {
                    replay_limit,
                    replay_max_cycles,
                },
                operability: ReleaseGateOperabilityPlan {
                    trace_limit,
                    request_limit,
                },
                eval: ReleaseGateEvalPlan {
                    eval_trace_limit,
                    eval_query_limit,
                    eval_search_limit,
                },
                benchmark_run: ReleaseGateBenchmarkRunPlan {
                    benchmark_query_limit,
                    benchmark_search_limit,
                },
                benchmark_gate: ReleaseGateBenchmarkGatePlan {
                    benchmark_threshold_p95_ms,
                    benchmark_min_top1_accuracy,
                    benchmark_min_stress_top1_accuracy,
                    benchmark_max_p95_regression_pct,
                    benchmark_max_top1_regression_pct,
                    benchmark_window_size,
                    benchmark_required_passes,
                },
                security_audit_mode,
            })?;
            print_json(&report)?;
            if enforce && !report.passed {
                anyhow::bail!("release gate pack failed");
            }
        }
    }
    Ok(())
}
