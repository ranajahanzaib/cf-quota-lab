mod managed;
mod runner;
use anyhow::Context;
use clap::{Args, Parser, Subcommand};
use quotalab_core::{config::*, model::*, render};
use std::path::{Path, PathBuf};
#[derive(Parser)]
#[command(
    name = "quotalab",
    version,
    about = "Test your Cloudflare resource budgets before production does."
)]
struct Cli {
    #[arg(long, global = true)]
    no_color: bool,
    #[command(subcommand)]
    command: Commands,
}
#[derive(Args)]
struct ConfigArg {
    #[arg(long, default_value = "quotalab.yaml")]
    config: PathBuf,
}
#[derive(Args)]
struct Output {
    #[arg(long, value_enum)]
    format: Option<OutputFormat>,
    #[arg(long)]
    output: Option<PathBuf>,
}
#[derive(Clone, Copy, clap::ValueEnum)]
enum OutputFormat {
    Terminal,
    Json,
    Markdown,
}
#[derive(Subcommand)]
enum Commands {
    /// Create a documented starter configuration without overwriting files.
    Init {
        #[arg(long)]
        force: bool,
    },
    /// Check configuration, tooling, target connectivity and telemetry.
    Doctor {
        #[command(flatten)]
        config: ConfigArg,
        #[arg(long)]
        wrangler_config: Option<PathBuf>,
    },
    /// Execute bounded HTTP workloads and enforce resource policies.
    Test {
        #[command(flatten)]
        config: ConfigArg,
        #[command(flatten)]
        output: Output,
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        managed: bool,
        #[arg(long, default_value = "wrangler.jsonc")]
        wrangler_config: PathBuf,
        #[arg(long)]
        seed_sql: Option<PathBuf>,
        #[arg(long)]
        allow_remote: bool,
        #[arg(long)]
        allow_remote_mutations: bool,
    },
    /// Reanalyze saved observations without issuing HTTP requests.
    Analyze {
        #[arg(long)]
        input: PathBuf,
        #[command(flatten)]
        config: ConfigArg,
        #[command(flatten)]
        output: Output,
    },
    /// Inspect query plans against an explicitly selected local SQLite file.
    Explain {
        #[arg(long)]
        database: PathBuf,
        #[arg(long)]
        input: PathBuf,
    },
    /// Regenerate reports from a versioned observation file.
    Report {
        #[arg(long)]
        input: PathBuf,
        #[command(flatten)]
        output: Output,
    },
    /// Show versioned limit reference data and official documentation sources.
    Limits {
        #[command(subcommand)]
        command: Limits,
    },
    /// Generate configuration and report JSON Schemas.
    Schemas {
        #[arg(long, default_value = "schemas")]
        output: PathBuf,
    },
}
#[derive(Subcommand)]
enum Limits {
    Show {
        #[arg(long, default_value = "workers-free")]
        profile: String,
        #[arg(long)]
        path: Option<PathBuf>,
    },
}
fn load(path: &Path) -> anyhow::Result<Config> {
    let mut c = Config::load(path)?;
    if let Some(p) = &c.profile.path {
        let p = PathBuf::from(p);
        if p.is_relative() {
            c.profile.path = Some(
                path.parent()
                    .unwrap_or(Path::new("."))
                    .join(p)
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    Ok(c)
}
fn read_report(path: &Path) -> anyhow::Result<Report> {
    anyhow::ensure!(
        std::fs::metadata(path)?.len() <= 256 * 1024 * 1024,
        "report exceeds 256 MiB limit"
    );
    let r: Report = serde_json::from_slice(&std::fs::read(path)?)?;
    r.validate()?;
    Ok(r)
}
fn output(r: &Report, o: &Output, formats: &[Format]) -> anyhow::Result<()> {
    if let Some(dir) = &o.output {
        std::fs::create_dir_all(dir)?;
        for format in formats {
            match format {
                Format::Json => {
                    std::fs::write(dir.join("run.json"), serde_json::to_string_pretty(r)?)?
                }
                Format::Markdown => std::fs::write(dir.join("report.md"), render::markdown(r))?,
                Format::Terminal => {}
            }
        }
    }
    let s = match o.format.unwrap_or(OutputFormat::Terminal) {
        OutputFormat::Terminal => render::terminal(r),
        OutputFormat::Json => serde_json::to_string_pretty(r)?,
        OutputFormat::Markdown => render::markdown(r),
    };
    println!("{s}");
    Ok(())
}
#[tokio::main]
async fn main() -> std::process::ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
    let cli = Cli::parse();
    let result = tokio::select! {r=dispatch(cli)=>r,_=interrupted()=>Err(anyhow::anyhow!("interrupted; managed processes stopped"))};
    match result {
        Ok(code) => std::process::ExitCode::from(code),
        Err(e) => {
            eprintln!("quotalab: {e:#}");
            std::process::ExitCode::from(3)
        }
    }
}
async fn dispatch(cli: Cli) -> anyhow::Result<u8> {
    match cli.command {
        Commands::Init { force } => {
            let path = Path::new("quotalab.yaml");
            anyhow::ensure!(
                force || !path.exists(),
                "quotalab.yaml already exists; use --force to overwrite"
            );
            std::fs::write(
                path,
                include_str!("../../../examples/basic-worker/quotalab.yaml").replace(
                    "../../schemas/config.schema.json",
                    "schemas/config.schema.json",
                ),
            )?;
            println!("Created quotalab.yaml. Review scenarios and bindings before testing.");
            for p in ["wrangler.jsonc", "wrangler.json", "wrangler.toml"] {
                if Path::new(p).exists() {
                    println!("Detected {p}");
                    if p.ends_with("json") || p.ends_with("jsonc") {
                        let v: serde_json::Value = json5::from_str(&std::fs::read_to_string(p)?)?;
                        for key in ["d1_databases", "kv_namespaces"] {
                            if let Some(items) = v.get(key).and_then(|x| x.as_array()) {
                                for i in items {
                                    println!(
                                        "{key}: {}",
                                        i.get("binding")
                                            .and_then(|x| x.as_str())
                                            .unwrap_or("unknown")
                                    )
                                }
                            }
                        }
                    }
                }
            }
            Ok(0)
        }
        Commands::Test {
            config,
            output: o,
            url,
            managed: manage,
            wrangler_config,
            seed_sql,
            allow_remote,
            allow_remote_mutations,
        } => {
            let mut c = load(&config.config)?;
            if let Some(url) = url {
                c.target.url = url
            }
            runner::validate_target(&c, allow_remote, allow_remote_mutations)?;
            // Resolve profile before starting a process or sending workload requests.
            if let Some(path) = &c.profile.path {
                let p: Profile = serde_json::from_str(&std::fs::read_to_string(path)?)?;
                p.validate()?;
                anyhow::ensure!(
                    p.schema_version == 1 && p.name == c.profile.name,
                    "invalid custom profile"
                )
            }
            let token = if manage {
                Some(uuid::Uuid::new_v4().simple().to_string())
            } else {
                std::env::var("QUOTALAB_TOKEN").ok()
            };
            let _managed = if manage {
                Some(
                    managed::start(
                        &c,
                        &wrangler_config,
                        seed_sql.as_deref(),
                        token.as_deref().context("missing token")?,
                    )
                    .await?,
                )
            } else {
                None
            };
            if c.workload.scenarios.iter().any(|s| s.fault.is_some()) {
                anyhow::ensure!(
                    token.is_some(),
                    "fault scenarios require managed runner or QUOTALAB_TOKEN"
                )
            }
            let r = runner::run(&c, token.as_deref()).await?;
            output(&r, &o, &c.reporting.formats)?;
            Ok(r.exit_code)
        }
        Commands::Analyze {
            input,
            config,
            output: o,
        } => {
            let c = load(&config.config)?;
            let mut r = read_report(&input)?;
            quotalab_core::analysis::analyze(&mut r, &c)?;
            output(&r, &o, &c.reporting.formats)?;
            Ok(r.exit_code)
        }
        Commands::Report { input, output: o } => {
            let r = read_report(&input)?;
            output(&r, &o, &[Format::Json, Format::Markdown])?;
            Ok(r.exit_code)
        }
        Commands::Explain { database, input } => {
            let plans = quotalab_core::explain::explain(&database, &read_report(&input)?)?;
            println!("{}", serde_json::to_string_pretty(&plans)?);
            Ok(0)
        }
        Commands::Limits {
            command: Limits::Show { profile, path },
        } => {
            let p = if let Some(path) = path {
                serde_json::from_str::<Profile>(&std::fs::read_to_string(path)?)?
            } else {
                anyhow::ensure!(profile == "workers-free", "unknown built-in profile");
                Profile::builtin()
            };
            println!("{}", serde_json::to_string_pretty(&p)?);
            Ok(0)
        }
        Commands::Schemas { output } => {
            std::fs::create_dir_all(&output)?;
            std::fs::write(
                output.join("config.schema.json"),
                format!("{}\n", serde_json::to_string_pretty(&schemars_config())?),
            )?;
            std::fs::write(
                output.join("report.schema.json"),
                format!("{}\n", serde_json::to_string_pretty(&schemars_report())?),
            )?;
            Ok(0)
        }
        Commands::Doctor {
            config,
            wrangler_config,
        } => {
            let c = load(&config.config)?;
            println!(
                "Configuration: valid; mode {:?}; target origin {}",
                c.target.mode,
                url::Url::parse(&c.target.url)?
                    .origin()
                    .ascii_serialization()
            );
            for (bin, args) in [("node", vec!["--version"]), ("npm", vec!["--version"])] {
                match tokio::process::Command::new(bin).args(args).output().await {
                    Ok(o) if o.status.success() => {
                        println!("{bin}: {}", String::from_utf8_lossy(&o.stdout).trim())
                    }
                    _ => anyhow::bail!("{bin} unavailable; install Node.js LTS and npm"),
                }
            }
            let status = managed::command()?
                .arg("--version")
                .stdout(std::process::Stdio::null())
                .status()
                .await?;
            anyhow::ensure!(status.success(), "Wrangler unavailable; run npm ci");
            let worker_path = wrangler_config
                .or_else(|| {
                    ["wrangler.jsonc", "wrangler.json"]
                        .iter()
                        .map(|p| config.config.parent().unwrap_or(Path::new(".")).join(p))
                        .find(|p| p.exists())
                })
                .context(
                    "Worker configuration missing; pass --wrangler-config with a JSON/JSONC file",
                )?;
            let data: serde_json::Value = json5::from_str(&std::fs::read_to_string(&worker_path)?)
                .context("invalid Worker JSON/JSONC configuration")?;
            for key in ["d1_databases", "kv_namespaces"] {
                if let Some(items) = data.get(key).and_then(|v| v.as_array()) {
                    for item in items {
                        let binding = item
                            .get("binding")
                            .and_then(|v| v.as_str())
                            .context("binding name missing in Worker config")?;
                        println!(
                            "{key}: {binding} configured (runtime access checked by workload)"
                        );
                    }
                } else {
                    println!(
                        "{key}: no bindings configured; corresponding measurements may be unavailable"
                    );
                }
            }
            println!(
                "Transport: token-guarded local response headers; remote resource telemetry is unavailable"
            );
            let http = runner::client(c.workload.timeout_ms)?;
            let mut req = http.get(url::Url::parse(&c.target.url)?.join("/__quotalab_ready")?);
            if let Ok(token) = std::env::var("QUOTALAB_TOKEN") {
                req = req.header("x-quotalab-token", token)
            }
            let response = req
                .send()
                .await
                .context("target unreachable; start Wrangler or use test --managed")?;
            println!(
                "Target: reachable; local header telemetry: {}; CPU: unavailable",
                if response.headers().contains_key("x-quotalab-telemetry") {
                    "active"
                } else {
                    "absent (set QUOTALAB_TOKEN and enable local instrumentation)"
                }
            );
            Ok(0)
        }
    }
}
fn schemars_config() -> serde_json::Value {
    quotalab_core::config_schema()
}
fn schemars_report() -> serde_json::Value {
    quotalab_core::report_schema()
}

async fn interrupted() {
    #[cfg(unix)]
    {
        if let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! {_ = tokio::signal::ctrl_c()=>{},_ = terminate.recv()=>{}};
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}
