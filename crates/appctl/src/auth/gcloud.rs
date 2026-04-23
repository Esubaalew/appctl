use std::{
    io::ErrorKind,
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct GcloudAdcToken {
    pub access_token: String,
    pub expires_at: Option<i64>,
    pub project_id: Option<String>,
}

pub fn ensure_gcloud_installed() -> Result<()> {
    let output = match Command::new("gcloud")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
    {
        Ok(output) => output,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            bail!(
                "Google Cloud CLI (`gcloud`) is not installed; {}",
                install_hint()
            )
        }
        Err(err) => return Err(err).context("failed to spawn `gcloud --version`"),
    };
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            bail!("gcloud is installed but not usable; {}", install_hint())
        } else {
            bail!(
                "gcloud is installed but not usable: {}. {}",
                stderr,
                install_hint()
            )
        }
    }
}

pub fn login_application_default(project_override: Option<&str>) -> Result<GcloudAdcToken> {
    ensure_gcloud_installed()?;

    let status = Command::new("gcloud")
        .args(["auth", "application-default", "login"])
        .status()
        .context("failed to spawn `gcloud auth application-default login`")?;
    if !status.success() {
        bail!(
            "`gcloud auth application-default login` failed. Complete the browser login and try again."
        );
    }

    if let Some(project) = project_override {
        let set_project = Command::new("gcloud")
            .args(["config", "set", "project", project])
            .status()
            .with_context(|| format!("failed to set gcloud project to '{project}'"))?;
        if !set_project.success() {
            bail!("gcloud login succeeded, but `gcloud config set project {project}` failed");
        }
    }

    adc_access_token(project_override)
}

pub fn adc_access_token(project_override: Option<&str>) -> Result<GcloudAdcToken> {
    ensure_gcloud_installed()?;

    let output = Command::new("gcloud")
        .args([
            "auth",
            "application-default",
            "print-access-token",
            "--format=json",
        ])
        .output()
        .context("failed to spawn `gcloud auth application-default print-access-token`")?;
    if !output.status.success() {
        bail!(
            "`gcloud auth application-default print-access-token` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let (access_token, expires_at) = parse_print_access_token_output(&stdout)?;
    let project_id = project_override.map(str::to_string).or_else(detect_project);

    Ok(GcloudAdcToken {
        access_token,
        expires_at,
        project_id,
    })
}

pub fn detect_project() -> Option<String> {
    Command::new("gcloud")
        .args(["config", "get-value", "project"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
                (!value.is_empty() && value != "(unset)").then_some(value)
            } else {
                None
            }
        })
}

pub fn install_hint() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "install it with `brew install --cask google-cloud-sdk`, then run `gcloud init`"
    }
    #[cfg(target_os = "linux")]
    {
        "install the Google Cloud CLI from https://cloud.google.com/sdk/docs/install"
    }
    #[cfg(target_os = "windows")]
    {
        "install the Google Cloud CLI from https://cloud.google.com/sdk/docs/install"
    }
}

fn parse_print_access_token_output(stdout: &str) -> Result<(String, Option<i64>)> {
    if stdout.is_empty() {
        bail!("gcloud returned an empty ADC access token")
    }

    if !stdout.starts_with('{') {
        return Ok((stdout.to_string(), None));
    }

    let payload: Value =
        serde_json::from_str(stdout).context("failed to parse gcloud JSON output")?;
    let access_token = payload
        .get("access_token")
        .or_else(|| payload.get("token"))
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .context("gcloud JSON output did not include an access token")?
        .to_string();
    let expires_at = payload
        .get("token_expiry")
        .or_else(|| payload.get("expires_at"))
        .and_then(Value::as_str)
        .and_then(parse_timestamp);
    Ok((access_token, expires_at))
}

fn parse_timestamp(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|timestamp| timestamp.timestamp())
}
