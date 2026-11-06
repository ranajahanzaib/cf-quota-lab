use quotalab_core::{analysis::*, config::*, explain, model::*, render};
use std::collections::BTreeMap;
fn config() -> Config {
    serde_yaml_ng::from_str(include_str!("../../../examples/basic-worker/quotalab.yaml")).unwrap()
}
fn report(c: &Config) -> Report {
    let telemetry = Telemetry {
        version: 1,
        request_id: "r".into(),
        run_id: "test".into(),
        scenario: String::new(),
        provenance: Provenance::MeasuredLocal,
        counters: [
            ("d1_rows_read", Some(2)),
            ("d1_rows_written", Some(0)),
            ("d1_queries", Some(1)),
            ("kv_reads", Some(1)),
            ("kv_writes", Some(1)),
            ("kv_deletes", Some(0)),
            ("kv_lists", Some(0)),
            ("external_subrequests", Some(0)),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v))
        .collect(),
        operations: BTreeMap::new(),
        queries: vec![],
        faults: vec![],
        dropped: 0,
        complete: true,
        coverage: vec![],
    };
    let mut requests: Vec<_> = c
        .workload
        .scenarios
        .iter()
        .flat_map(|s| {
            (0..c.workload.requests_per_scenario).map(|i| RequestResult {
                index: i,
                scenario: s.name.clone(),
                status: Some(s.expect.status),
                passed: true,
                error: None,
                timeout: false,
                latency_ms: 1.,
                telemetry: Some(telemetry.clone()),
            })
        })
        .collect();
    for (i, row) in requests.iter_mut().enumerate() {
        row.index = i;
    }
    let unknown = Metric {
        value: None,
        provenance: Provenance::Unavailable,
        samples: 0,
        complete: false,
        note: String::new(),
    };
    Report {
        schema_version: 1,
        tool_version: "test".into(),
        run_id: "test".into(),
        timestamp: "2026-11-01T00:00:00Z".into(),
        project: c.project.name.clone(),
        mode: "local".into(),
        profile: Profile::builtin(),
        seed: c.workload.seed,
        requests,
        scenarios: vec![],
        elapsed_ms: 100.,
        throughput: unknown.clone(),
        latency_ms: BTreeMap::new(),
        cpu_ms: unknown,
        projections: vec![],
        peaks: BTreeMap::new(),
        queries: vec![],
        findings: vec![],
        assumptions: vec![],
        exit_code: 0,
    }
}
#[test]
fn config_validation_and_weights() {
    let mut c = config();
    c.validate().unwrap();
    assert!((c.weights().iter().sum::<f64>() - 1.).abs() < 1e-10);
    c.workload.concurrency = 0;
    assert!(c.validate().is_err());
    c = config();
    for s in &mut c.workload.scenarios {
        s.weight = 0
    }
    assert!(c.validate().is_err());
}
#[test]
fn reject_misspellings_and_negative_budgets() {
    let s = include_str!("../../../examples/basic-worker/quotalab.yaml");
    assert!(serde_yaml_ng::from_str::<Config>(&s.replace("concurrency:", "concurency:")).is_err());
    assert!(serde_yaml_ng::from_str::<Config>(&s.replace("800", "-1")).is_err());
}
#[test]
fn local_target_is_literal_loopback_only() {
    let mut c = config();
    for host in [
        "https://example.com",
        "http://localhost:8787",
        "http://127.0.0.1.evil.test",
    ] {
        c.target.url = host.into();
        assert!(c.validate().is_err())
    }
    c.target.url = "http://[::1]:8787".into();
    assert!(c.validate().is_ok());
}
#[test]
fn remote_faults_rejected() {
    let mut c = config();
    c.target.mode = Mode::Remote;
    c.target.url = "https://example.com".into();
    c.workload.scenarios[0].fault = Some(Fault {
        service: Service::Kv,
        kind: FaultKind::Unavailable,
        after: 1,
        delay_ms: 0,
        operation: None,
    });
    assert!(c.validate().is_err());
}
#[test]
fn profile_data_is_versioned() {
    let p = Profile::builtin();
    assert_eq!(p.daily["kv_writes"], 1000);
    assert_eq!(p.invocation["external_subrequests"], 50);
    assert!(!p.sources.is_empty());
    assert_eq!(p.schema_version, 1);
}
#[test]
fn checked_overflow_and_invalid_means() {
    assert_eq!(checked_projection(100, 2.5, 10).unwrap(), 260.);
    assert!(checked_projection(u64::MAX, 2., 0).is_err());
    assert!(checked_projection(1, f64::NAN, 0).is_err());
    assert!(checked_projection(1, -1., 0).is_err());
}
#[test]
fn weighted_projection_and_kv_regression() {
    let c = config();
    let mut r = report(&c);
    analyze(&mut r, &c).unwrap();
    let p = r
        .projections
        .iter()
        .find(|p| p.metric == "kv_writes")
        .unwrap();
    assert_eq!(p.daily.value, Some(20000.));
    assert_eq!(p.utilization_percent, Some(2500.));
    assert_eq!(r.exit_code, 2);
    assert!(r.findings.iter().any(|f| f.code == "daily_budget"));
    assert_eq!(p.exhaustion_hours, Some(0.96));
}
#[test]
fn unknown_is_not_zero_and_missing_policy_controls_exit() {
    let mut c = config();
    let mut r = report(&c);
    r.requests[0].telemetry = None;
    c.policy.missing_measurements = Missing::Fail;
    analyze(&mut r, &c).unwrap();
    assert_eq!(r.exit_code, 4);
    assert!(
        r.projections
            .iter()
            .find(|p| p.metric == "kv_writes")
            .unwrap()
            .daily
            .value
            .is_none()
    );
    assert_eq!(r.cpu_ms.provenance, Provenance::Unavailable);
    assert!(r.cpu_ms.value.is_none());
    c.policy.missing_measurements = Missing::Ignore;
    analyze(&mut r, &c).unwrap();
    assert_eq!(r.exit_code, 0);
}
#[test]
fn dropped_telemetry_invalidates_totals() {
    let mut c = config();
    c.policy.missing_measurements = Missing::Fail;
    let mut r = report(&c);
    r.requests[0].telemetry.as_mut().unwrap().dropped = 1;
    analyze(&mut r, &c).unwrap();
    assert_eq!(r.exit_code, 4);
}
#[test]
fn serialization_order_and_rendering_stable() {
    let c = config();
    let mut r = report(&c);
    analyze(&mut r, &c).unwrap();
    let serialized = serde_json::to_string(&r).unwrap();
    let mut copy: Report = serde_json::from_str(&serialized).unwrap();
    analyze(&mut copy, &c).unwrap();
    assert_eq!(serialized, serde_json::to_string(&copy).unwrap());
    assert!(render::markdown(&r).contains("CPU: unavailable"));
}
#[test]
fn incomplete_run_cannot_pass() {
    let c = config();
    let mut r = report(&c);
    r.requests.pop();
    analyze(&mut r, &c).unwrap();
    assert_ne!(r.exit_code, 0);
    assert!(r.findings.iter().any(|f| f.code == "incomplete_run"));
}
#[test]
fn behavior_failures_enforced() {
    let c = config();
    let mut r = report(&c);
    r.requests[0].passed = false;
    analyze(&mut r, &c).unwrap();
    assert!(r.findings.iter().any(|f| f.code == "behavior"));
}
#[test]
fn scan_plan_changes_with_index() {
    let c = config();
    let mut r = report(&c);
    r.queries.push(QuerySummary {
        fingerprint: "Q-test".into(),
        sql: Some("SELECT * FROM users WHERE email=?".into()),
        executions: 10,
        rows_read: Some(20000),
        peak_rows_read: Some(2000),
        rows_written: Some(0),
        projected_daily_rows_read: Some(2000000.),
        provenance: Provenance::MeasuredLocal,
    });
    let path = std::env::temp_dir().join(format!("quotalab-explain-{}.sqlite", std::process::id()));
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(include_str!(
        "../../../examples/budget-violations/migrations/0001_users.sql"
    ))
    .unwrap();
    let plans = explain::explain(&path, &r).unwrap();
    assert!(!plans[0].findings.is_empty());
    db.execute_batch("CREATE INDEX users_email ON users(email)")
        .unwrap();
    let plans = explain::explain(&path, &r).unwrap();
    assert!(plans[0].findings.is_empty());
    assert!(plans[0].details.iter().any(|d| d.contains("INDEX")));
    drop(db);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn query_findings_rank_by_expected_frequency() {
    let mut c = config();
    c.workload.requests_per_scenario = 1;
    let mut r = report(&c);
    for (i, row) in r.requests.iter_mut().enumerate() {
        row.telemetry.as_mut().unwrap().queries.push(Query {
            fingerprint: format!("Q-{i}"),
            sql: None,
            method: "all".into(),
            rows_read: Some(1000),
            rows_written: Some(0),
            duration_ms: None,
            success: true,
        })
    }
    analyze(&mut r, &c).unwrap();
    assert_eq!(r.queries[0].fingerprint, "Q-0");
}

#[test]
fn explicit_cpu_budget_uses_missing_coverage_policy() {
    let mut c = config();
    c.budgets.worker.max_cpu_ms_per_request = Some(10);
    c.policy.missing_measurements = Missing::Fail;
    let mut r = report(&c);
    analyze(&mut r, &c).unwrap();
    assert_eq!(r.exit_code, 4);
    assert!(r.findings.iter().any(|f| f.message.contains("cpu_ms")));
}
#[test]
fn warning_threshold_can_be_enforced() {
    let mut c = config();
    c.traffic.requests_per_day = 900;
    c.budgets.kv.max_projected_writes_per_day = Some(1000);
    c.policy.fail_on = Severity::Warning;
    let mut r = report(&c);
    analyze(&mut r, &c).unwrap();
    assert_eq!(r.exit_code, 2);
    assert!(r.findings.iter().any(|f| f.code == "budget_headroom"));
}
#[test]
fn incomplete_d1_keeps_known_query_subtotal() {
    let c = config();
    let mut r = report(&c);
    let t = r.requests[0].telemetry.as_mut().unwrap();
    t.counters.insert("d1_rows_read".into(), None);
    t.queries.push(Query {
        fingerprint: "partial".into(),
        sql: None,
        method: "all".into(),
        rows_read: Some(41),
        rows_written: Some(0),
        duration_ms: None,
        success: true,
    });
    analyze(&mut r, &c).unwrap();
    let p = r
        .projections
        .iter()
        .find(|p| p.metric == "d1_rows_read")
        .unwrap();
    assert!(p.daily.value.is_none());
    assert!(p.observed_subtotal >= 41.);
}
