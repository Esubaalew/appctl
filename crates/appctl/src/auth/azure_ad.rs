//! Azure AD (Microsoft identity platform) device-code OAuth flow.
//!
//! This runs a real device-code flow against
//! `https://login.microsoftonline.com/{tenant}/oauth2/v2.0/`. It opens the
//! verification URL in the real browser and polls the token endpoint until the
//! user completes sign-in.
//!
//! The default scope is `https://cognitiveservices.azure.com/.default`, which
//! is what Azure OpenAI accepts when the data-plane is configured for AAD
//! instead of API keys.

use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

use crate::auth::oauth::{OAuthTokenNamespace, StoredTokens, save_tokens_for, webbrowser_open};

pub const DEFAULT_SCOPE: &str = "https://cognitiveservices.azure.com/.default";

#[derive(Debug, Clone)]
pub struct AzureAdDeviceConfig {
    pub tenant: String,
    pub client_id: String,
    pub scope: String,
    pub storage_key: String,
    /// Override the Microsoft identity platform base URL. Defaults to
    /// `https://login.microsoftonline.com` in production. Tests point this at
    /// a wiremock instance.
    pub authority_base: Option<String>,
    /// When true, don't open the real OS browser. Used in unit tests.
    pub suppress_browser: bool,
}

impl AzureAdDeviceConfig {
    fn authority(&self) -> &str {
        self.authority_base
            .as_deref()
            .unwrap_or("https://login.microsoftonline.com")
    }
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: i64,
    interval: u64,
    #[allow(dead_code)]
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    token_type: Option<String>,
    #[serde(default)]
    scope: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenError {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

pub async fn device_code_login(cfg: AzureAdDeviceConfig) -> Result<StoredTokens> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("failed to build http client for Azure AD")?;

    let authority = cfg.authority().trim_end_matches('/').to_string();
    let device_endpoint = format!("{authority}/{}/oauth2/v2.0/devicecode", cfg.tenant);
    let token_endpoint = format!("{authority}/{}/oauth2/v2.0/token", cfg.tenant);

    let device_resp = client
        .post(&device_endpoint)
        .form(&[
            ("client_id", cfg.client_id.as_str()),
            ("scope", cfg.scope.as_str()),
        ])
        .send()
        .await
        .context("failed to start Azure AD device code flow")?;

    let device_status = device_resp.status();
    let device_body = device_resp
        .text()
        .await
        .context("failed to read Azure AD device code response body")?;
    if !device_status.is_success() {
        bail!(
            "Azure AD device code request failed with {}: {}",
            device_status,
            device_body
        );
    }
    let device: DeviceCodeResponse = serde_json::from_str(&device_body)
        .with_context(|| format!("failed to parse device code response: {device_body}"))?;

    println!(
        "To sign in to Azure, open {} and enter code {}",
        device.verification_uri, device.user_code
    );
    if !cfg.suppress_browser {
        let _ = webbrowser_open(&device.verification_uri);
    }

    let interval = Duration::from_secs(device.interval.max(1));
    let mut deadline =
        std::time::Instant::now() + Duration::from_secs(device.expires_in.max(60) as u64);

    loop {
        if std::time::Instant::now() >= deadline {
            bail!("Azure AD device code expired before the user completed sign-in")
        }

        tokio::time::sleep(interval).await;

        let token_resp = client
            .post(&token_endpoint)
            .form(&[
                ("client_id", cfg.client_id.as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", device.device_code.as_str()),
            ])
            .send()
            .await
            .context("failed to poll Azure AD token endpoint")?;

        let status = token_resp.status();
        let body = token_resp
            .text()
            .await
            .context("failed to read Azure AD token response body")?;

        if status.is_success() {
            let token: TokenResponse = serde_json::from_str(&body)
                .with_context(|| format!("failed to parse Azure AD token response: {body}"))?;
            let scopes = token
                .scope
                .unwrap_or_default()
                .split_whitespace()
                .map(str::to_string)
                .collect();
            let expires_at = token
                .expires_in
                .map(|seconds| chrono::Utc::now().timestamp() + seconds);
            let stored = StoredTokens {
                access_token: token.access_token,
                refresh_token: token.refresh_token,
                expires_at,
                token_type: token.token_type,
                scopes,
            };
            if let Err(err) =
                save_tokens_for(OAuthTokenNamespace::Provider, &cfg.storage_key, &stored)
            {
                eprintln!(
                    "warning: failed to persist Azure AD tokens to the OS keychain: {err}. The session token is still usable until it expires."
                );
            }
            return Ok(stored);
        }

        let err: TokenError = match serde_json::from_str(&body) {
            Ok(err) => err,
            Err(_) => bail!("Azure AD token response returned {status}: {body}"),
        };

        match err.error.as_str() {
            "authorization_pending" => continue,
            "slow_down" => {
                deadline += Duration::from_secs(5);
                continue;
            }
            "expired_token" => bail!("Azure AD device code expired"),
            "access_denied" => bail!("User declined Azure AD sign-in"),
            other => {
                return Err(anyhow!(
                    "Azure AD token exchange failed: {other} {}",
                    err.error_description.unwrap_or_default()
                ));
            }
        }
    }
}

/// Refresh a previously obtained Azure AD access token.
pub async fn refresh_token(
    tenant: &str,
    client_id: &str,
    refresh_token: &str,
    scope: &str,
) -> Result<StoredTokens> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let endpoint = format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token");
    let resp = client
        .post(&endpoint)
        .form(&[
            ("client_id", client_id),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("scope", scope),
        ])
        .send()
        .await
        .context("failed to refresh Azure AD token")?;
    let status = resp.status();
    let body = resp.text().await.context("failed to read refresh body")?;
    if !status.is_success() {
        bail!("Azure AD refresh failed with {}: {}", status, body);
    }
    let token: TokenResponse = serde_json::from_str(&body)
        .with_context(|| format!("failed to parse Azure AD refresh response: {body}"))?;
    let scopes = token
        .scope
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let expires_at = token
        .expires_in
        .map(|seconds| chrono::Utc::now().timestamp() + seconds);
    Ok(StoredTokens {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at,
        token_type: token.token_type,
        scopes,
    })
}
