use crate::config::Severity;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    MeasuredLocal,
    MeasuredRemote,
    Estimated,
    Simulated,
    Unavailable,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Metric {
    pub value: Option<f64>,
    pub provenance: Provenance,
    pub samples: usize,
    pub complete: bool,
    pub note: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Query {
    pub fingerprint: String,
    pub sql: Option<String>,
    pub method: String,
    pub rows_read: Option<u64>,
    pub rows_written: Option<u64>,
    pub duration_ms: Option<f64>,
    pub success: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    pub attempts: u64,
    pub successes: u64,
    pub failures: u64,
    pub units: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Telemetry {
    pub version: u32,
    pub request_id: String,
    pub run_id: String,
    pub scenario: String,
    pub provenance: Provenance,
    pub counters: BTreeMap<String, Option<u64>>,
    pub operations: BTreeMap<String, Operation>,
    pub queries: Vec<Query>,
    pub faults: Vec<String>,
    pub dropped: u64,
    pub complete: bool,
    pub coverage: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RequestResult {
    pub index: usize,
    pub scenario: String,
    pub status: Option<u16>,
    pub passed: bool,
    pub error: Option<String>,
    pub timeout: bool,
    pub latency_ms: f64,
    pub telemetry: Option<Telemetry>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub severity: Severity,
    pub code: String,
    pub message: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Projection {
    pub metric: String,
    pub per_request: Metric,
    pub daily: Metric,
    pub observed_subtotal: f64,
    pub budget: u64,
    pub utilization_percent: Option<f64>,
    pub exhaustion_hours: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QuerySummary {
    pub fingerprint: String,
    pub sql: Option<String>,
    pub executions: usize,
    pub rows_read: Option<u64>,
    pub peak_rows_read: Option<u64>,
    pub rows_written: Option<u64>,
    pub projected_daily_rows_read: Option<f64>,
    pub provenance: Provenance,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub schema_version: u32,
    pub name: String,
    pub verified_on: String,
    pub sources: Vec<String>,
    pub daily: BTreeMap<String, u64>,
    pub invocation: BTreeMap<String, u64>,
    pub storage: BTreeMap<String, u64>,
    pub notes: Vec<String>,
}
impl Profile {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(self.schema_version == 1, "unsupported profile schema");
        for key in [
            "worker_requests",
            "d1_rows_read",
            "d1_rows_written",
            "kv_reads",
            "kv_writes",
            "kv_deletes",
            "kv_lists",
        ] {
            anyhow::ensure!(
                self.daily.contains_key(key),
                "profile missing daily limit {key}"
            );
        }
        for key in ["external_subrequests", "d1_queries"] {
            anyhow::ensure!(
                self.invocation.contains_key(key),
                "profile missing invocation limit {key}"
            );
        }
        Ok(())
    }
    pub fn builtin() -> Self {
        serde_json::from_str(include_str!("../../../profiles/workers-free.v1.json"))
            .expect("embedded profile must be valid")
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScenarioSummary {
    pub name: String,
    pub weight: u32,
    pub normalized_weight: f64,
    pub samples: usize,
    pub successful: usize,
    pub unexpected_status: usize,
    pub transport_errors: usize,
    pub timeouts: usize,
    pub faults_triggered: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub schema_version: u32,
    pub tool_version: String,
    pub run_id: String,
    pub timestamp: String,
    pub project: String,
    pub mode: String,
    pub profile: Profile,
    pub seed: u64,
    pub requests: Vec<RequestResult>,
    pub scenarios: Vec<ScenarioSummary>,
    pub elapsed_ms: f64,
    pub throughput: Metric,
    pub latency_ms: BTreeMap<String, Metric>,
    pub cpu_ms: Metric,
    pub projections: Vec<Projection>,
    pub peaks: BTreeMap<String, Metric>,
    pub queries: Vec<QuerySummary>,
    pub findings: Vec<Finding>,
    pub assumptions: Vec<String>,
    pub exit_code: u8,
}
impl Report {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(self.schema_version == 1, "unsupported report schema");
        anyhow::ensure!(self.requests.len() <= 100000, "oversized report");
        anyhow::ensure!(
            self.elapsed_ms.is_finite() && self.elapsed_ms >= 0.,
            "invalid elapsed time"
        );
        let mut indexes = std::collections::BTreeSet::new();
        for row in &self.requests {
            anyhow::ensure!(indexes.insert(row.index), "duplicate request index");
            anyhow::ensure!(
                row.latency_ms.is_finite() && row.latency_ms >= 0.,
                "invalid request latency"
            );
            if let Some(t) = &row.telemetry {
                anyhow::ensure!(
                    t.version == 1 && t.queries.len() <= 128 && t.faults.len() <= 32,
                    "invalid telemetry bounds or version"
                );
            }
        }
        Ok(())
    }
}
