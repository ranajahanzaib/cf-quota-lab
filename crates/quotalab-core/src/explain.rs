use crate::model::Report;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
#[derive(Debug, Serialize)]
pub struct Plan {
    pub fingerprint: String,
    pub sql: String,
    pub details: Vec<String>,
    pub findings: Vec<String>,
}
pub fn explain(database: &std::path::Path, report: &Report) -> anyhow::Result<Vec<Plan>> {
    let db = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    db.pragma_update(None, "query_only", true)?;
    let mut out = Vec::new();
    for q in &report.queries {
        let Some(sql) = &q.sql else { continue };
        let mut stmt = db.prepare(&format!("EXPLAIN QUERY PLAN {sql}"))?;
        for i in 1..=stmt.parameter_count() {
            stmt.raw_bind_parameter(i, rusqlite::types::Null)?
        }
        let mut rows = stmt.raw_query();
        let mut details = Vec::new();
        let mut findings = Vec::new();
        while let Some(row) = rows.next()? {
            let detail: String = row.get(3)?;
            if detail.contains("SCAN ") && !detail.contains("USING") {
                let table = detail.split_whitespace().nth(1).unwrap_or("");
                if !table.is_empty()
                    && table
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                {
                    let count: Result<i64, _> =
                        db.query_row(&format!("SELECT count(*) FROM \"{table}\""), [], |r| {
                            r.get(0)
                        });
                    if count.is_ok_and(|n| n >= 500) || q.peak_rows_read.is_some_and(|n| n >= 500) {
                        findings.push(format!("Substantial full scan: {detail}; inspect filtering/join indexes and rerun with representative fixtures."));
                    }
                }
            }
            if detail.contains("TEMP B-TREE") || detail.contains("CORRELATED") {
                findings.push(format!("Review scaling behavior: {detail}"));
            }
            details.push(detail);
        }
        out.push(Plan {
            fingerprint: q.fingerprint.clone(),
            sql: sql.clone(),
            details,
            findings,
        });
    }
    Ok(out)
}
