use std::time::Duration;

use serde_json::Value;
use tokio::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("Obsidian CLI timed out after {0}s — is Obsidian running?")]
    Timeout(u64),
    #[error("Could not start Obsidian CLI: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("Obsidian CLI error (exit {code:?}): {stderr}")]
    NonZero { code: Option<i32>, stderr: String },
    #[error("CLI returned empty output")]
    Empty,
}

impl CliError {
    pub fn to_tool_error(&self) -> String {
        self.to_string()
    }
}

#[derive(Clone, Debug)]
pub struct ObsidianCli {
    bin: String,
    vault: Option<String>,
    timeout: Duration,
}

impl ObsidianCli {
    pub fn from_env() -> Result<Self, anyhow::Error> {
        let vault = std::env::var("OBSIDIAN_VAULT").ok();
        let bin = std::env::var("OBSIDIAN_BIN").unwrap_or_else(|_| "obsidian".into());
        let timeout_secs: u64 = std::env::var("OBSIDIAN_TIMEOUT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);

        Ok(Self {
            bin,
            vault,
            timeout: Duration::from_secs(timeout_secs),
        })
    }

    pub fn with_vault(mut self, vault: String) -> Self {
        self.vault = Some(vault);
        self
    }

    pub fn vault_name(&self) -> &str {
        self.vault.as_deref().unwrap_or("(auto)")
    }

    /// Retries up to 3 times with 10s pauses, then opens the vault if configured.
    pub async fn startup_check(&self) -> Result<(), anyhow::Error> {
        const MAX_RETRIES: u32 = 3;
        const RETRY_DELAY: Duration = Duration::from_secs(10);

        for attempt in 1..=MAX_RETRIES {
            tracing::info!("Health check attempt {}/{}...", attempt, MAX_RETRIES);
            match self.run_bare(&["version"]).await {
                Ok(_) => {
                    tracing::info!("Obsidian CLI is reachable");
                    self.ensure_vault_open().await;
                    return Ok(());
                }
                Err(e) if attempt < MAX_RETRIES => {
                    tracing::warn!(
                        "Health check failed: {}. Retrying in {}s...",
                        e,
                        RETRY_DELAY.as_secs()
                    );
                    tokio::time::sleep(RETRY_DELAY).await;
                }
                Err(e) => {
                    anyhow::bail!("CLI health check failed after {MAX_RETRIES} attempts: {e}");
                }
            }
        }
        unreachable!()
    }

    /// Exit code -1 from vault:open means already open.
    async fn ensure_vault_open(&self) {
        let Some(ref vault) = self.vault else { return };
        tracing::info!("Ensuring vault '{}' is open...", vault);
        match self
            .run_bare(&["vault:open", &format!("vault={vault}")])
            .await
        {
            Ok(_) => tracing::info!("Vault '{vault}' opened"),
            Err(CliError::NonZero { code: Some(-1), .. }) => {
                tracing::info!("Vault '{vault}' is already open");
            }
            Err(e) => tracing::warn!("Could not open vault '{vault}': {e}"),
        }
    }

    async fn run_bare(&self, args: &[&str]) -> Result<String, CliError> {
        let output = tokio::time::timeout(self.timeout, {
            Command::new(&self.bin)
                .args(args)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .output()
        })
        .await
        .map_err(|_| CliError::Timeout(self.timeout.as_secs()))?
        .map_err(CliError::Spawn)?;

        if !output.status.success() {
            return Err(CliError::NonZero {
                code: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn build_command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(&self.bin);
        cmd.args(args);
        if let Some(ref vault) = self.vault {
            cmd.arg(format!("vault={vault}"));
        }
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd
    }

    /// Runs a CLI command, returning parsed JSON. Falls back to converting
    /// plain-text lines into a JSON array when the CLI doesn't return JSON.
    pub async fn run(&self, args: &[&str]) -> Result<Value, CliError> {
        let output = tokio::time::timeout(self.timeout, self.build_command(args).output())
            .await
            .map_err(|_| CliError::Timeout(self.timeout.as_secs()))?
            .map_err(CliError::Spawn)?;

        if !output.status.success() {
            return Err(CliError::NonZero {
                code: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        if output.stdout.is_empty() {
            return Err(CliError::Empty);
        }

        let raw = String::from_utf8_lossy(&output.stdout);
        tracing::debug!(cmd = ?args, output = %raw, "CLI raw output");

        let trimmed = raw.trim();
        if trimmed.starts_with("Error:") || trimmed.starts_with("error:") {
            return Err(CliError::NonZero {
                code: Some(0),
                stderr: trimmed.to_owned(),
            });
        }

        if let Ok(val) = serde_json::from_str::<Value>(&raw) {
            return Ok(val);
        }
        if let Some(first_line) = raw.lines().next()
            && let Ok(val) = serde_json::from_str::<Value>(first_line)
        {
            return Ok(val);
        }
        if let Some(start) = raw.find('[').or_else(|| raw.find('{'))
            && let Ok(val) = serde_json::from_str::<Value>(&raw[start..])
        {
            return Ok(val);
        }

        tracing::debug!(cmd = ?args, "Plain text output, converting to JSON array");
        let lines: Vec<Value> = raw
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(|l| Value::String(l.to_owned()))
            .collect();
        Ok(Value::Array(lines))
    }

    pub async fn run_raw(&self, args: &[&str]) -> Result<String, CliError> {
        let output = tokio::time::timeout(self.timeout, self.build_command(args).output())
            .await
            .map_err(|_| CliError::Timeout(self.timeout.as_secs()))?
            .map_err(CliError::Spawn)?;

        if !output.status.success() {
            return Err(CliError::NonZero {
                code: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}
