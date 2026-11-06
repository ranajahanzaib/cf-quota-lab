use anyhow::{Context, ensure};
use quotalab_core::config::{Config, Mode};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::process::{Child, Command};
struct ChildGuard {
    child: Child,
    pid: Option<u32>,
}
impl ChildGuard {
    fn new(child: Child) -> Self {
        let pid = child.id();
        Self { child, pid }
    }
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(id) = self.pid {
            // SAFETY: the child was spawned in its own process group; the stored positive
            // PID identifies that group. No pointers are passed. Errors mean it already exited.
            if let Ok(id) = i32::try_from(id) {
                unsafe {
                    libc::kill(-id, libc::SIGTERM);
                }
            }
        }
        let _ = self.child.start_kill();
    }
}
pub struct Managed {
    process: ChildGuard,
    _dir: tempfile::TempDir,
}
pub fn wrangler() -> anyhow::Result<(PathBuf, Vec<String>)> {
    for ancestor in std::env::current_dir()?.ancestors() {
        let p = ancestor.join("node_modules/wrangler/bin/wrangler.js");
        if p.exists() {
            return Ok((
                PathBuf::from("node"),
                vec![p.to_string_lossy().into_owned()],
            ));
        }
    }
    Ok((PathBuf::from("wrangler"), vec![]))
}
pub fn command() -> anyhow::Result<Command> {
    let (bin, args) = wrangler()?;
    let mut c = Command::new(bin);
    c.args(args)
        .env("WRANGLER_SEND_METRICS", "false")
        .env("CI", "true")
        .kill_on_drop(true);
    #[cfg(unix)]
    {
        c.process_group(0);
    }
    Ok(c)
}
pub async fn start(
    c: &Config,
    config: &Path,
    seed: Option<&Path>,
    token: &str,
) -> anyhow::Result<Managed> {
    ensure!(
        c.target.mode == Mode::Local,
        "managed runner requires local mode"
    );
    let config = std::fs::canonicalize(config).context("Wrangler config not found")?;
    let data: serde_json::Value = json5::from_str(&std::fs::read_to_string(&config)?)
        .context("managed runner requires Wrangler JSON/JSONC")?;
    for key in ["d1_databases", "kv_namespaces", "services"] {
        if let Some(bindings) = data.get(key).and_then(|v| v.as_array()) {
            ensure!(
                bindings
                    .iter()
                    .all(|v| v.get("remote") != Some(&serde_json::Value::Bool(true))),
                "managed local bindings cannot have remote:true"
            );
        }
    }
    let url = url::Url::parse(&c.target.url)?;
    let host = url.host_str().context("target host missing")?;
    let port = url.port_or_known_default().context("port missing")?;
    ensure!(url.scheme() == "http", "managed target must use HTTP");
    ensure!(
        tokio::net::TcpStream::connect((host, port)).await.is_err(),
        "managed target port already occupied; refuse to test an unrelated server"
    );
    let dir = tempfile::tempdir()?;
    let persist = dir.path().join("state");
    let envfile = dir.path().join("test.env");
    std::fs::write(
        &envfile,
        format!("QUOTALAB_LOCAL=true\nQUOTALAB_TOKEN={token}\n"),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&envfile, std::fs::Permissions::from_mode(0o600))?;
    }
    if let Some(seed) = seed {
        let seed = std::fs::canonicalize(seed)?;
        let mut seed_command = command()?;
        seed_command
            .args(["d1", "execute", "DB", "--local", "--config"])
            .arg(&config)
            .arg("--persist-to")
            .arg(&persist)
            .arg("--file")
            .arg(seed)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        let mut seed_process = ChildGuard::new(seed_command.spawn()?);
        let status = tokio::time::timeout(Duration::from_secs(60), seed_process.child.wait())
            .await
            .context("fixture seed timed out")??;
        ensure!(status.success(), "local D1 seed failed")
    }
    let child = command()?
        .args(["dev", "--local", "--config"])
        .arg(config)
        .arg("--persist-to")
        .arg(persist)
        .arg("--env-file")
        .arg(envfile)
        .args([
            "--ip",
            host,
            "--port",
            &port.to_string(),
            "--inspector-port",
            "0",
            "--log-level",
            "error",
            "--show-interactive-dev-session=false",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .context("cannot start Wrangler; run npm ci")?;
    let mut managed = Managed {
        process: ChildGuard::new(child),
        _dir: dir,
    };
    let http = crate::runner::client(500)?;
    for _ in 0..120 {
        if let Some(status) = managed.process.child.try_wait()? {
            anyhow::bail!("Wrangler exited during startup: {status}")
        }
        if http
            .get(url.join("/__quotalab_ready")?)
            .send()
            .await
            .is_ok()
        {
            return Ok(managed);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    anyhow::bail!("Wrangler did not become ready within startup deadline")
}
