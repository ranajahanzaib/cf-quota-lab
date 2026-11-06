use anyhow::{Context, ensure};
use base64::Engine;
use futures_util::{StreamExt, stream};
use quotalab_core::{config::*, model::*};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
use uuid::Uuid;

pub fn client(timeout: u64) -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()?)
}
pub fn validate_target(
    c: &Config,
    allow_remote: bool,
    allow_mutations: bool,
) -> anyhow::Result<()> {
    c.validate()?;
    for s in &c.workload.scenarios {
        for (k, v) in &s.request.headers {
            reqwest::header::HeaderName::from_bytes(k.as_bytes())
                .context("invalid configured header name")?;
            reqwest::header::HeaderValue::from_str(v).context("invalid configured header value")?;
        }
    }
    if c.target.mode == Mode::Remote {
        ensure!(allow_remote, "remote targets require --allow-remote");
        ensure!(
            c.workload.scenarios.iter().all(|s| s.fault.is_none()),
            "remote fault injection is forbidden"
        );
        if c.workload
            .scenarios
            .iter()
            .any(|s| s.request.method != "GET")
        {
            ensure!(
                allow_mutations,
                "remote write methods require --allow-remote-mutations; review configured workload first"
            )
        }
    }
    Ok(())
}
// SplitMix64 schedule: stable across platforms and independent of task completion order.
pub fn selection(seed: u64, index: usize, len: usize) -> usize {
    let mut z = seed.wrapping_add((index as u64).wrapping_mul(0x9e3779b97f4a7c15));
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    ((z ^ (z >> 31)) % len.max(1) as u64) as usize
}
fn contains(actual: &serde_json::Value, expected: &serde_json::Value) -> bool {
    match expected {
        serde_json::Value::Object(m) => m
            .iter()
            .all(|(k, v)| actual.get(k).is_some_and(|a| contains(a, v))),
        _ => actual == expected,
    }
}
async fn execute(
    client: &reqwest::Client,
    c: &Config,
    scenario: &Scenario,
    index: usize,
    token: Option<&str>,
    run: &str,
) -> RequestResult {
    let start = Instant::now();
    let mut row = RequestResult {
        index,
        scenario: scenario.name.clone(),
        status: None,
        passed: false,
        error: None,
        timeout: false,
        latency_ms: 0.,
        telemetry: None,
    };
    let outcome = async {
        let url = url::Url::parse(&c.target.url)?.join(&scenario.request.path)?;
        ensure!(
            url.origin() == url::Url::parse(&c.target.url)?.origin(),
            "request escaped configured origin"
        );
        let mut req = client.request(
            reqwest::Method::from_bytes(scenario.request.method.as_bytes())?,
            url,
        );
        for (k, v) in &scenario.request.headers {
            req = req.header(k, v)
        }
        if let Some(body) = scenario
            .fixtures
            .get(selection(c.workload.seed, index, scenario.fixtures.len()))
            .or(scenario.request.json.as_ref())
        {
            req = req.json(body)
        }
        if c.target.mode == Mode::Local
            && let Some(token) = token
        {
            req = req
                .header("x-quotalab-token", token)
                .header("x-quotalab-run", run)
                .header("x-quotalab-request", format!("r-{index}"))
                .header("x-quotalab-scenario", &scenario.name);
            if let Some(fault) = &scenario.fault {
                req = req.header("x-quotalab-fault", serde_json::to_string(fault)?);
            }
        }
        let mut response = req.send().await?;
        row.status = Some(response.status().as_u16());
        if c.target.mode == Mode::Local
            && let Some(encoded) = response.headers().get("x-quotalab-telemetry")
        {
            ensure!(encoded.len() <= 8192, "oversized telemetry");
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded.as_bytes())
                .context("invalid telemetry encoding")?;
            let t: Telemetry =
                serde_json::from_slice(&bytes).context("invalid telemetry payload")?;
            ensure!(
                t.version == 1
                    && t.run_id == run
                    && t.scenario == scenario.name
                    && t.request_id == format!("r-{index}")
                    && t.provenance == Provenance::MeasuredLocal,
                "invalid telemetry identity or provenance"
            );
            ensure!(
                t.queries.len() <= 128 && t.faults.len() <= 32 && t.counters.len() <= 32,
                "telemetry exceeds bounds"
            );
            row.telemetry = Some(t);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            ensure!(
                body.len() + chunk.len() <= 1048576,
                "response exceeds 1 MiB assertion limit"
            );
            body.extend_from_slice(&chunk);
        }
        row.passed = row.status == Some(scenario.expect.status);
        if let Some(expected) = &scenario.expect.json_contains {
            let actual: serde_json::Value =
                serde_json::from_slice(&body).context("expected JSON response")?;
            row.passed &= contains(&actual, expected)
        }
        if scenario.expect.fault_triggered {
            row.passed &= row.telemetry.as_ref().is_some_and(|t| !t.faults.is_empty())
        }
        if !row.passed {
            row.error = Some("HTTP status, response shape or fault-trigger assertion failed".into())
        }
        Ok::<_, anyhow::Error>(())
    }
    .await;
    if let Err(e) = outcome {
        row.timeout = e
            .downcast_ref::<reqwest::Error>()
            .is_some_and(|e| e.is_timeout());
        row.error = Some(
            if row.timeout {
                "request timed out"
            } else {
                "transport, body or telemetry failure (inspect target locally)"
            }
            .into(),
        );
    }
    row.latency_ms = start.elapsed().as_secs_f64() * 1000.;
    row
}
pub async fn run(c: &Config, token: Option<&str>) -> anyhow::Result<Report> {
    let http = client(c.workload.timeout_ms)?;
    let run = Uuid::new_v4().to_string();
    for (si, s) in c.workload.scenarios.iter().enumerate() {
        for n in 0..c.workload.warmup_requests {
            let _ = execute(&http, c, s, 100000 + si * 1000 + n, token, &run).await;
        }
    }
    let start = Instant::now();
    let total = c.workload.requests_per_scenario * c.workload.scenarios.len();
    let mut requests = stream::iter(0..total)
        .map(|i| {
            let http = &http;
            let run = &run;
            async move {
                // Shuffle scenario ordering without changing equal sample counts.
                let block = i / c.workload.scenarios.len();
                let offset = selection(c.workload.seed, block, c.workload.scenarios.len());
                let s = &c.workload.scenarios
                    [(i % c.workload.scenarios.len() + offset) % c.workload.scenarios.len()];
                execute(http, c, s, i, token, run).await
            }
        })
        .buffer_unordered(c.workload.concurrency)
        .collect::<Vec<_>>()
        .await;
    requests.sort_by_key(|r| r.index);
    let unknown = Metric {
        value: None,
        provenance: Provenance::Unavailable,
        samples: 0,
        complete: false,
        note: String::new(),
    };
    let assumptions = vec![
 "Local binding metadata is not authoritative production consumption.".into(),
 "Traffic is hypothetical and uniform; account estimates are partial unless all other usage is configured.".into(),
 "Wrapper bypasses, redirects and background work cannot be fully observed; failed operations are attempts, not guaranteed billed units.".into(),
 "Fixed fixtures and seed reproduce request selection and per-invocation faults; concurrency, data mutations and latency can vary.".into(),
 ];
    let mut r = Report {
        schema_version: 1,
        tool_version: env!("CARGO_PKG_VERSION").into(),
        run_id: run,
        timestamp: chrono::Utc::now().to_rfc3339(),
        project: c.project.name.clone(),
        mode: format!("{:?}", c.target.mode).to_lowercase(),
        profile: Profile::builtin(),
        seed: c.workload.seed,
        requests,
        scenarios: vec![],
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.,
        throughput: unknown.clone(),
        latency_ms: BTreeMap::new(),
        cpu_ms: unknown,
        projections: vec![],
        peaks: BTreeMap::new(),
        queries: vec![],
        findings: vec![],
        assumptions,
        exit_code: 0,
    };
    quotalab_core::analysis::analyze(&mut r, c)?;
    Ok(r)
}
