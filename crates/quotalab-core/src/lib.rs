pub mod analysis;
pub mod config;
pub mod explain;
pub mod model;
pub mod render;
pub fn config_schema() -> serde_json::Value {
    let mut s =
        serde_json::to_value(schemars::schema_for!(config::Config)).expect("schema serialization");
    s["properties"]["version"]["const"] = 1.into();
    for (field, min, max) in [
        ("concurrency", 1, 64),
        ("requests_per_scenario", 1, 10000),
        ("timeout_ms", 1, 60000),
        ("warmup_requests", 0, 1000),
    ] {
        s["$defs"]["Workload"]["properties"][field]["minimum"] = min.into();
        s["$defs"]["Workload"]["properties"][field]["maximum"] = max.into();
    }
    s["$defs"]["Workload"]["properties"]["scenarios"]["minItems"] = 1.into();
    s["$defs"]["Workload"]["properties"]["scenarios"]["maxItems"] = 100.into();
    s["$defs"]["Request"]["properties"]["method"]["enum"] =
        serde_json::json!(["GET", "POST", "PUT", "PATCH", "DELETE"]);
    s["$defs"]["Expectation"]["properties"]["status"]["minimum"] = 100.into();
    s["$defs"]["Expectation"]["properties"]["status"]["maximum"] = 599.into();
    s["$defs"]["Fault"]["properties"]["after"]["minimum"] = 1.into();
    s["$defs"]["Fault"]["properties"]["delay_ms"]["maximum"] = 10000.into();
    s
}
pub fn report_schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(model::Report)).expect("schema serialization")
}
