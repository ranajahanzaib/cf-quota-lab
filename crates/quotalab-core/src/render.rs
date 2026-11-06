use crate::model::*;
pub fn markdown(r: &Report) -> String {
    let mut s = format!(
        "# QuotaLab — Resource Budget Report\n\nProject: {} · Profile: {} · Mode: {}\n\nRequests: {} · Failed: {} · Seed: {}\n\nCPU: unavailable (production CPU cannot be inferred locally)\n\n| Resource | Per request | Projected daily | Budget | Utilization |\n|---|---:|---:|---:|---:|\n",
        r.project,
        r.profile.name,
        r.mode,
        r.requests.len(),
        r.requests.iter().filter(|x| !x.passed).count(),
        r.seed
    );
    for p in &r.projections {
        s.push_str(&format!(
            "| {} | {} | {} (estimated) | {} | {} |\n",
            p.metric,
            number(p.per_request.value),
            number(p.daily.value),
            p.budget,
            p.utilization_percent
                .map(|v| format!("{v:.1}%"))
                .unwrap_or_else(|| "unknown".into())
        ));
    }
    s.push_str("\nPer-request observations: measured_local; wrapper coverage only. Unknown totals retain an observed subtotal in JSON.\n\n");
    for q in r.queries.iter().take(10) {
        s.push_str(&format!("- Query `{}`: {} executions, peak {} rows read (measured_local), estimated daily rows {}.\n",q.fingerprint,q.executions,q.peak_rows_read.map(|n|n.to_string()).unwrap_or_else(||"unknown".into()),number(q.projected_daily_rows_read)));
    }
    s.push_str("\nFindings:\n\n");
    if r.findings.is_empty() {
        s.push_str("No enforced policy violations.\n")
    }
    for f in &r.findings {
        s.push_str(&format!("- {:?} [{}]: {}\n", f.severity, f.code, f.message));
    }
    let faults = r
        .requests
        .iter()
        .filter_map(|x| x.telemetry.as_ref())
        .flat_map(|t| &t.faults)
        .count();
    s.push_str(&format!("\nSimulated faults triggered: {faults}; HTTP behavior assertions are evaluated separately.\n\n"));
    for a in &r.assumptions {
        s.push_str(&format!("- {a}\n"));
    }
    s.push_str(&format!(
        "\nRESULT: {} (exit {})\n",
        if r.exit_code == 0 { "PASSED" } else { "FAILED" },
        r.exit_code
    ));
    s
}
fn number(n: Option<f64>) -> String {
    n.map(|v| format!("{v:.2}"))
        .unwrap_or_else(|| "unavailable".into())
}
pub fn terminal(r: &Report) -> String {
    markdown(r)
        .replace("# QuotaLab", "QuotaLab")
        .replace('`', "")
}
