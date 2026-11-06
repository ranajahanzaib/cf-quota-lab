use crate::{config::*, model::*};
use std::collections::BTreeMap;
fn metric(value: Option<f64>, p: Provenance, n: usize, complete: bool, note: &str) -> Metric {
    Metric {
        value,
        provenance: if value.is_some() {
            p
        } else {
            Provenance::Unavailable
        },
        samples: n,
        complete,
        note: note.into(),
    }
}
fn finding(r: &mut Report, severity: Severity, code: &str, message: String) {
    r.findings.push(Finding {
        severity,
        code: code.into(),
        message,
    })
}
fn missing(r: &mut Report, c: &Config, name: &str) {
    if c.policy.missing_measurements != Missing::Ignore {
        finding(
            r,
            if c.policy.missing_measurements == Missing::Fail {
                Severity::Critical
            } else {
                Severity::Warning
            },
            "missing_measurement",
            format!("{name}: required complete measurement unavailable"),
        )
    }
}
pub fn analyze(r: &mut Report, c: &Config) -> anyhow::Result<()> {
    c.validate()?;
    r.validate()?;
    r.findings.clear();
    r.projections.clear();
    r.peaks.clear();
    r.queries.clear();
    anyhow::ensure!(
        r.project == c.project.name,
        "report project does not match configuration"
    );
    let expected = c.workload.requests_per_scenario * c.workload.scenarios.len();
    if r.requests.len() != expected {
        finding(
            r,
            Severity::Critical,
            "incomplete_run",
            format!(
                "expected {expected} observations, found {}",
                r.requests.len()
            ),
        )
    }
    let failed = r.requests.iter().filter(|x| !x.passed).count();
    if failed > 0 {
        finding(
            r,
            Severity::Critical,
            "behavior",
            format!("{failed} requests failed HTTP, transport, body or fault assertions"),
        )
    }
    let profile = if let Some(path) = &c.profile.path {
        serde_json::from_str::<Profile>(&std::fs::read_to_string(path)?)?
    } else {
        Profile::builtin()
    };
    anyhow::ensure!(
        profile.schema_version == 1 && profile.name == c.profile.name,
        "invalid profile version or name"
    );
    profile.validate()?;
    r.profile = profile;
    let weights = c.weights();
    r.scenarios = c
        .workload
        .scenarios
        .iter()
        .zip(&weights)
        .map(|(s, w)| {
            let rows: Vec<_> = r.requests.iter().filter(|q| q.scenario == s.name).collect();
            ScenarioSummary {
                name: s.name.clone(),
                weight: s.weight,
                normalized_weight: *w,
                samples: rows.len(),
                successful: rows.iter().filter(|q| q.passed).count(),
                unexpected_status: rows
                    .iter()
                    .filter(|q| q.status.is_some_and(|code| code != s.expect.status))
                    .count(),
                transport_errors: rows.iter().filter(|q| q.status.is_none()).count(),
                timeouts: rows.iter().filter(|q| q.timeout).count(),
                faults_triggered: rows
                    .iter()
                    .filter_map(|q| q.telemetry.as_ref())
                    .map(|t| t.faults.len())
                    .sum(),
            }
        })
        .collect();
    if c.budgets.worker.max_cpu_ms_per_request.is_some() {
        missing(r, c, "cpu_ms");
    }
    let keys = [
        "worker_requests",
        "d1_rows_read",
        "d1_rows_written",
        "kv_reads",
        "kv_writes",
        "kv_deletes",
        "kv_lists",
    ];
    for key in keys {
        let budget = match key {
            "worker_requests" => c.budgets.worker.max_requests_per_day,
            "d1_rows_read" => c.budgets.d1.max_projected_rows_read_per_day,
            "d1_rows_written" => c.budgets.d1.max_projected_rows_written_per_day,
            "kv_reads" => c.budgets.kv.max_projected_reads_per_day,
            "kv_writes" => c.budgets.kv.max_projected_writes_per_day,
            "kv_deletes" => c.budgets.kv.max_projected_deletes_per_day,
            "kv_lists" => c.budgets.kv.max_projected_lists_per_day,
            _ => None,
        }
        .or_else(|| r.profile.daily.get(key).copied())
        .ok_or_else(|| anyhow::anyhow!("profile missing {key}"))?;
        let mut avg = 0.;
        let mut subtotal = 0.;
        let mut samples = 0;
        let mut complete = true;
        for (s, weight) in c.workload.scenarios.iter().zip(&weights) {
            if *weight == 0. {
                continue;
            }
            let rows: Vec<_> = r.requests.iter().filter(|x| x.scenario == s.name).collect();
            if rows.is_empty() {
                complete = false;
                continue;
            }
            let mut sum = 0u128;
            let mut known = 0;
            for row in &rows {
                // Fault scenarios describe synthetic behavior, not normal traffic cost.
                let observed = if key == "worker_requests" {
                    Some(1)
                } else if s.fault.is_some() {
                    None
                } else {
                    row.telemetry
                        .as_ref()
                        .filter(|t| t.provenance == Provenance::MeasuredLocal)
                        .and_then(|t| {
                            t.counters.get(key).copied().flatten().or_else(|| {
                                let values: Vec<_> = t
                                    .queries
                                    .iter()
                                    .filter_map(|q| match key {
                                        "d1_rows_read" => q.rows_read,
                                        "d1_rows_written" => q.rows_written,
                                        _ => None,
                                    })
                                    .collect();
                                if values.is_empty() {
                                    None
                                } else {
                                    values
                                        .into_iter()
                                        .try_fold(0u64, |sum, v| sum.checked_add(v))
                                }
                            })
                        })
                };
                if let Some(v) = observed {
                    subtotal += v as f64;
                }
                let value = if key == "worker_requests" {
                    Some(1)
                } else if s.fault.is_some() {
                    None
                } else {
                    row.telemetry
                        .as_ref()
                        .filter(|t| {
                            t.complete
                                && t.dropped == 0
                                && t.provenance == Provenance::MeasuredLocal
                        })
                        .and_then(|t| t.counters.get(key).copied().flatten())
                };
                if let Some(v) = value {
                    sum = sum
                        .checked_add(u128::from(v))
                        .ok_or_else(|| anyhow::anyhow!("counter overflow"))?;
                    known += 1
                }
            }
            samples += known;
            if known != rows.len() {
                complete = false
            } else {
                avg += weight * sum as f64 / rows.len() as f64
            }
        }
        let existing = c.traffic.existing_usage.get(key).copied().unwrap_or(0);
        let daily = if complete {
            Some(checked_projection(
                c.traffic.requests_per_day,
                avg,
                existing,
            )?)
        } else {
            None
        };
        if !complete {
            missing(r, c, key)
        }
        if let Some(v) = daily {
            if v > budget as f64 {
                finding(
                    r,
                    Severity::Critical,
                    "daily_budget",
                    format!("{key}: projected {v:.2}/day exceeds budget {budget}"),
                )
            } else if budget > 0 && v / budget as f64 >= 0.8 {
                finding(
                    r,
                    Severity::Warning,
                    "budget_headroom",
                    format!("{key}: projected utilization exceeds 80%"),
                )
            }
        }
        r.projections.push(Projection {
            metric: key.into(),
            per_request: metric(
                complete.then_some(avg),
                Provenance::Estimated,
                samples,
                complete,
                "weighted scenario mean; normal traffic only",
            ),
            daily: metric(
                daily,
                Provenance::Estimated,
                samples,
                complete,
                "hypothetical uniform daily traffic plus known existing usage",
            ),
            observed_subtotal: subtotal,
            budget,
            utilization_percent: daily
                .filter(|_| budget > 0)
                .map(|v| v / budget as f64 * 100.),
            exhaustion_hours: daily.filter(|v| *v > 0.).map(|v| 24. * budget as f64 / v),
        });
    }
    let peaks = [
        (
            "external_subrequests",
            c.budgets.worker.max_external_subrequests_per_request.or(r
                .profile
                .invocation
                .get("external_subrequests")
                .copied()),
        ),
        ("d1_rows_read", c.budgets.d1.max_rows_read_per_request),
        ("d1_rows_written", c.budgets.d1.max_rows_written_per_request),
        (
            "d1_queries",
            c.budgets.d1.max_queries_per_request.or(r
                .profile
                .invocation
                .get("d1_queries")
                .copied()),
        ),
    ];
    for (key, budget) in peaks {
        let values: Vec<_> = r
            .requests
            .iter()
            .filter_map(|x| x.telemetry.as_ref())
            .filter(|t| {
                t.complete
                    && t.dropped == 0
                    && t.faults.is_empty()
                    && t.provenance == Provenance::MeasuredLocal
            })
            .filter_map(|t| t.counters.get(key).copied().flatten())
            .collect();
        let complete = values.len() == r.requests.len();
        let peak = values.iter().max().map(|v| *v as f64);
        r.peaks.insert(
            key.into(),
            metric(
                peak,
                Provenance::MeasuredLocal,
                values.len(),
                complete,
                "observed wrapper peak; fetch redirects and bypassed operations unobserved",
            ),
        );
        if let Some(b) = budget {
            if !complete {
                missing(r, c, key)
            }
            if peak.is_some_and(|v| v > b as f64) {
                finding(
                    r,
                    Severity::Critical,
                    "invocation_budget",
                    format!("{key}: peak {} exceeds {b}", peak.unwrap_or_default()),
                )
            }
        }
    }
    let mut groups: BTreeMap<String, Vec<(&RequestResult, &Query)>> = BTreeMap::new();
    for row in &r.requests {
        if let Some(t) = &row.telemetry {
            for q in &t.queries {
                groups
                    .entry(q.fingerprint.clone())
                    .or_default()
                    .push((row, q));
            }
        }
    }
    for (fp, items) in groups {
        let read = items
            .iter()
            .try_fold(0u64, |sum, (_, q)| sum.checked_add(q.rows_read?));
        let written = items
            .iter()
            .try_fold(0u64, |sum, (_, q)| sum.checked_add(q.rows_written?));
        let mut daily = 0.;
        let mut known = true;
        for (s, w) in c.workload.scenarios.iter().zip(&weights) {
            if *w == 0. {
                continue;
            }
            let n = r.requests.iter().filter(|x| x.scenario == s.name).count();
            if n == 0 || s.fault.is_some() {
                known = false;
                continue;
            }
            let total = items
                .iter()
                .filter(|(row, _)| row.scenario == s.name)
                .try_fold(0u64, |sum, (_, q)| sum.checked_add(q.rows_read?));
            if let Some(v) = total {
                daily += w * v as f64 / n as f64 * c.traffic.requests_per_day as f64
            } else {
                known = false
            }
        }
        let peak = items.iter().filter_map(|(_, q)| q.rows_read).max();
        if peak.is_some_and(|p| p > c.budgets.d1.max_rows_read_per_request.unwrap_or(500)) {
            r.findings.push(Finding{severity:Severity::Warning,code:"expensive_query".into(),message:format!("{fp}: peak {} locally observed rows read; inspect EXPLAIN QUERY PLAN and relevant indexes",peak.unwrap_or_default())});
        }
        r.queries.push(QuerySummary {
            fingerprint: fp,
            sql: items[0].1.sql.clone(),
            executions: items.len(),
            rows_read: read,
            peak_rows_read: peak,
            rows_written: written,
            projected_daily_rows_read: known.then_some(daily),
            provenance: Provenance::MeasuredLocal,
        });
    }
    r.queries.sort_by(|a, b| {
        b.projected_daily_rows_read
            .unwrap_or(0.)
            .total_cmp(&a.projected_daily_rows_read.unwrap_or(0.))
            .then(a.fingerprint.cmp(&b.fingerprint))
    });
    let mut latencies: Vec<_> = r.requests.iter().map(|x| x.latency_ms).collect();
    latencies.sort_by(f64::total_cmp);
    r.latency_ms.clear();
    for (name, p) in [("p50", 0.5), ("p95", 0.95), ("p99", 0.99)] {
        let v = if latencies.is_empty() {
            None
        } else {
            Some(latencies[((latencies.len() as f64 * p).ceil() as usize).saturating_sub(1)])
        };
        r.latency_ms.insert(
            name.into(),
            metric(
                v,
                Provenance::MeasuredLocal,
                latencies.len(),
                true,
                "HTTP wall-clock duration, not CPU",
            ),
        );
    }
    r.cpu_ms = metric(
        None,
        Provenance::Unavailable,
        0,
        false,
        "production CPU cannot be inferred locally",
    );
    r.throughput = metric(
        (r.elapsed_ms > 0.).then(|| r.requests.len() as f64 * 1000. / r.elapsed_ms),
        Provenance::MeasuredLocal,
        r.requests.len(),
        true,
        "test throughput, not production forecast",
    );
    r.findings.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then(a.code.cmp(&b.code))
            .then(a.message.cmp(&b.message))
    });
    r.exit_code = if r.requests.iter().any(|x| x.status.is_none() || x.timeout) {
        3
    } else if r.findings.iter().any(|f| f.code == "missing_measurement")
        && c.policy.missing_measurements == Missing::Fail
    {
        4
    } else if r.findings.iter().any(|f| f.severity >= c.policy.fail_on) {
        2
    } else {
        0
    };
    Ok(())
}
pub fn checked_projection(traffic: u64, avg: f64, existing: u64) -> anyhow::Result<f64> {
    anyhow::ensure!(avg.is_finite() && avg >= 0., "invalid projection mean");
    let v = traffic as f64 * avg + existing as f64;
    anyhow::ensure!(v.is_finite() && v <= u64::MAX as f64, "projection overflow");
    Ok(v)
}
