use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use thiserror::Error;

#[derive(Debug, Error)]
#[error("invalid configuration: {0}")]
pub struct ConfigError(pub String);
macro_rules! model { ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct $name { $(pub $field: $ty),* }
}; }
model!(Config {
    version: u32,
    project: Project,
    target: Target,
    profile: ProfileSelection,
    workload: Workload,
    traffic: Traffic,
    budgets: Budgets,
    policy: Policy,
    reporting: Reporting
});
model!(Project { name: String });
model!(Target {
    url: String,
    mode: Mode
});
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Local,
    Remote,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProfileSelection {
    pub name: String,
    #[serde(default)]
    pub path: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Workload {
    pub seed: u64,
    pub concurrency: usize,
    pub requests_per_scenario: usize,
    pub timeout_ms: u64,
    #[serde(default)]
    pub warmup_requests: usize,
    pub scenarios: Vec<Scenario>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub name: String,
    pub weight: u32,
    pub request: Request,
    pub expect: Expectation,
    #[serde(default)]
    pub fault: Option<Fault>,
    #[serde(default)]
    pub fixtures: Vec<serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub method: String,
    pub path: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub json: Option<serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Expectation {
    pub status: u16,
    #[serde(default)]
    pub json_contains: Option<serde_json::Value>,
    #[serde(default)]
    pub fault_triggered: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Fault {
    pub service: Service,
    pub kind: FaultKind,
    #[serde(default = "one")]
    pub after: u32,
    #[serde(default)]
    pub delay_ms: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
}
fn one() -> u32 {
    1
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Service {
    D1,
    Kv,
    Fetch,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FaultKind {
    Unavailable,
    Exhausted,
    Latency,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Traffic {
    pub requests_per_day: u64,
    #[serde(default)]
    pub existing_usage: BTreeMap<String, u64>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Budgets {
    #[serde(default)]
    pub worker: WorkerBudgets,
    #[serde(default)]
    pub d1: D1Budgets,
    #[serde(default)]
    pub kv: KvBudgets,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkerBudgets {
    pub max_external_subrequests_per_request: Option<u64>,
    pub max_requests_per_day: Option<u64>,
    pub max_cpu_ms_per_request: Option<u64>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct D1Budgets {
    pub max_rows_read_per_request: Option<u64>,
    pub max_rows_written_per_request: Option<u64>,
    pub max_queries_per_request: Option<u64>,
    pub max_projected_rows_read_per_day: Option<u64>,
    pub max_projected_rows_written_per_day: Option<u64>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KvBudgets {
    pub max_projected_reads_per_day: Option<u64>,
    pub max_projected_writes_per_day: Option<u64>,
    pub max_projected_deletes_per_day: Option<u64>,
    pub max_projected_lists_per_day: Option<u64>,
}
model!(Policy {
    fail_on: Severity,
    missing_measurements: Missing
});
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Informational,
    Warning,
    Critical,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Missing {
    Warn,
    Fail,
    Ignore,
}
model!(Reporting { formats:Vec<Format> });
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Terminal,
    Json,
    Markdown,
}
impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let mut c: Self = serde_yaml_ng::from_str(&std::fs::read_to_string(path)?)?;
        // Substitution is intentionally limited to the operational target URL.
        if c.target.url.starts_with("${") && c.target.url.ends_with('}') {
            let key = &c.target.url[2..c.target.url.len() - 1];
            c.target.url = std::env::var(key)
                .map_err(|_| ConfigError(format!("environment variable {key} is not set")))?;
        }
        c.validate()?;
        Ok(c)
    }
    pub fn validate(&self) -> Result<(), ConfigError> {
        let bad = |s: &str| Err(ConfigError(s.into()));
        if self.version != 1 {
            return bad("version must be 1");
        }
        if self.project.name.is_empty() || self.project.name.len() > 128 {
            return bad("project name must have 1..128 characters");
        }
        if self.profile.path.is_none() && self.profile.name != "workers-free" {
            return bad("unknown profile; supply profile.path for a custom profile");
        }
        let url = url::Url::parse(&self.target.url)
            .map_err(|_| ConfigError("invalid target URL".into()))?;
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return bad("target requires HTTP(S), without credentials, query or fragment");
        }
        if self.target.mode == Mode::Local && !is_loopback(&url) {
            return bad("local mode requires a literal loopback address");
        }
        let w = &self.workload;
        if !(1..=64).contains(&w.concurrency)
            || !(1..=10000).contains(&w.requests_per_scenario)
            || !(1..=60000).contains(&w.timeout_ms)
            || w.warmup_requests > 1000
            || w.scenarios.is_empty()
            || w.scenarios.len() > 100
        {
            return bad(
                "workload limits: concurrency 1..64, requests 1..10000, timeout 1..60000ms, warmup <=1000, scenarios 1..100",
            );
        }
        if w.requests_per_scenario
            .checked_mul(w.scenarios.len())
            .is_none_or(|n| n > 100000)
        {
            return bad("maximum total requests is 100000");
        }
        let mut names = BTreeSet::new();
        let mut positive = false;
        for s in &w.scenarios {
            if s.name.is_empty()
                || s.name.len() > 80
                || !s
                    .name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
                || !names.insert(&s.name)
            {
                return bad("scenario names must be unique safe identifiers (1..80)");
            }
            positive |= s.weight > 0;
            if !matches!(
                s.request.method.as_str(),
                "GET" | "POST" | "PUT" | "PATCH" | "DELETE"
            ) {
                return bad("unsupported HTTP method");
            }
            if !s.request.path.starts_with('/')
                || s.request.path.starts_with("//")
                || s.request.path.contains('\\')
            {
                return bad("scenario paths must be origin-relative");
            }
            if !(100..=599).contains(&s.expect.status) {
                return bad("invalid expected status");
            }
            if s.request.headers.keys().any(|k| {
                k.to_ascii_lowercase().starts_with("x-quotalab") || k.eq_ignore_ascii_case("host")
            }) {
                return bad("reserved request header");
            }
            if s.fixtures.len() > 1000 {
                return bad("at most 1000 fixtures");
            }
            if let Some(f) = &s.fault {
                if self.target.mode != Mode::Local || !is_loopback(&url) {
                    return bad("remote fault injection is forbidden");
                };
                if f.operation.as_ref().is_some_and(|o| {
                    ![
                        "get",
                        "getWithMetadata",
                        "put",
                        "delete",
                        "list",
                        "run",
                        "all",
                        "first",
                        "raw",
                        "batch",
                        "exec",
                        "fetch",
                    ]
                    .contains(&o.as_str())
                }) {
                    return bad("unknown fault operation");
                }
                if f.after == 0 || f.delay_ms > 10000 {
                    return bad("fault.after must be positive; delay_ms <=10000");
                }
            }
        }
        if !positive {
            return bad("at least one traffic weight must be positive");
        }
        let allowed = [
            "worker_requests",
            "d1_rows_read",
            "d1_rows_written",
            "kv_reads",
            "kv_writes",
            "kv_deletes",
            "kv_lists",
        ];
        if self
            .traffic
            .existing_usage
            .keys()
            .any(|k| !allowed.contains(&k.as_str()))
        {
            return bad("unknown existing_usage metric");
        }
        if self.reporting.formats.is_empty() {
            return bad("reporting.formats must not be empty");
        }
        Ok(())
    }
    pub fn weights(&self) -> Vec<f64> {
        let sum: u64 = self
            .workload
            .scenarios
            .iter()
            .map(|s| u64::from(s.weight))
            .sum();
        self.workload
            .scenarios
            .iter()
            .map(|s| f64::from(s.weight) / sum as f64)
            .collect()
    }
}
pub fn is_loopback(url: &url::Url) -> bool {
    matches!(url.host(),Some(url::Host::Ipv4(ip)) if ip.is_loopback())
        || matches!(url.host(),Some(url::Host::Ipv6(ip)) if ip.is_loopback())
}
