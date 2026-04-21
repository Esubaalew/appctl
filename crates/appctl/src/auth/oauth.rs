//! OAuth2 PKCE login flow backed by the system keychain.
//!
//! Tokens are stored as a JSON blob in the OS keychain keyed by
//! `appctl_oauth::<provider>` for target-app auth and
//! `appctl_llm_oauth::<profile>` for provider auth.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use oauth2::basic::BasicClient;
use oauth2::reqwest;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge, RedirectUrl,
    RefreshToken, Scope, TokenResponse, TokenUrl,
};
use serde::{Deserialize, Serialize};

use crate::config::{delete_secret, load_secret, save_secret};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTokens {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at: Option<i64>,
    #[serde(default)]
    pub token_type: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum OAuthTokenNamespace {
    Target,
    Provider,
}

fn secret_key(namespace: OAuthTokenNamespace, key: &str) -> String {
    match namespace {
        OAuthTokenNamespace::Target => format!("appctl_oauth::{key}"),
        OAuthTokenNamespace::Provider => format!("appctl_llm_oauth::{key}"),
    }
}

fn load_tokens_for(namespace: OAuthTokenNamespace, key: &str) -> Option<StoredTokens> {
    let raw = load_secret(&secret_key(namespace, key)).ok()?;
    if raw.is_empty() {
        return None;
    }
    serde_json::from_str(&raw).ok()
}

fn save_tokens_for(namespace: OAuthTokenNamespace, key: &str, tokens: &StoredTokens) -> Result<()> {
    let encoded = serde_json::to_string(tokens)?;
    save_secret(&secret_key(namespace, key), &encoded)
}

fn delete_tokens_for(namespace: OAuthTokenNamespace, key: &str) -> Result<()> {
    delete_secret(&secret_key(namespace, key))
}

pub fn load_tokens(provider: &str) -> Option<StoredTokens> {
    load_tokens_for(OAuthTokenNamespace::Target, provider)
}

pub fn save_tokens(provider: &str, tokens: &StoredTokens) -> Result<()> {
    save_tokens_for(OAuthTokenNamespace::Target, provider, tokens)
}

pub fn load_provider_tokens(profile: &str) -> Option<StoredTokens> {
    load_tokens_for(OAuthTokenNamespace::Provider, profile)
}

pub fn save_provider_tokens(profile: &str, tokens: &StoredTokens) -> Result<()> {
    save_tokens_for(OAuthTokenNamespace::Provider, profile, tokens)
}

pub fn load_access_token(provider: &str) -> Option<String> {
    load_tokens(provider).map(|t| t.access_token)
}

pub fn load_provider_access_token(profile: &str) -> Option<String> {
    load_provider_tokens(profile).map(|t| t.access_token)
}

pub fn delete_provider_tokens(profile: &str) -> Result<()> {
    delete_tokens_for(OAuthTokenNamespace::Provider, profile)
}

#[derive(Debug, Clone)]
pub struct OAuthLoginConfig {
    pub provider: String,
    pub storage_key: String,
    pub namespace: OAuthTokenNamespace,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub auth_url: String,
    pub token_url: String,
    pub scopes: Vec<String>,
    pub redirect_port: u16,
}

pub async fn login(config: OAuthLoginConfig) -> Result<StoredTokens> {
    let redirect_uri = format!("http://127.0.0.1:{}/callback", config.redirect_port);
    let client = BasicClient::new(ClientId::new(config.client_id.clone()))
        .set_auth_uri(AuthUrl::new(config.auth_url.clone()).context("invalid auth_url")?)
        .set_token_uri(TokenUrl::new(config.token_url.clone()).context("invalid token_url")?)
        .set_redirect_uri(RedirectUrl::new(redirect_uri.clone()).context("invalid redirect_uri")?);
    let client = if let Some(secret) = &config.client_secret {
        client.set_client_secret(ClientSecret::new(secret.clone()))
    } else {
        client
    };

    let http = reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let mut auth_request = client.authorize_url(CsrfToken::new_random);
    for scope in &config.scopes {
        auth_request = auth_request.add_scope(Scope::new(scope.clone()));
    }
    let (auth_url, csrf_state) = auth_request.set_pkce_challenge(pkce_challenge).url();

    println!(
        "Opening browser for OAuth2 login. If it does not open, visit:\n{}",
        auth_url
    );
    let _ = webbrowser_open(auth_url.as_str());

    let (code, state) = wait_for_callback(config.redirect_port)?;
    if state.secret() != csrf_state.secret() {
        bail!("OAuth2 CSRF state mismatch");
    }

    let token_resp = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(pkce_verifier)
        .request_async(&http)
        .await
        .map_err(|err| anyhow!("token exchange failed: {err}"))?;

    let stored = StoredTokens {
        access_token: token_resp.access_token().secret().clone(),
        refresh_token: token_resp.refresh_token().map(|t| t.secret().clone()),
        expires_at: token_resp
            .expires_in()
            .map(|d| chrono::Utc::now().timestamp() + d.as_secs() as i64),
        token_type: Some(format!("{:?}", token_resp.token_type()).to_lowercase()),
        scopes: config.scopes.clone(),
    };
    save_tokens_for(config.namespace, &config.storage_key, &stored)?;
    Ok(stored)
}

pub async fn refresh(config: &OAuthLoginConfig) -> Result<StoredTokens> {
    let current = load_tokens_for(config.namespace, &config.storage_key)
        .context("no stored tokens to refresh")?;
    let refresh_token = current
        .refresh_token
        .clone()
        .context("no refresh token on file")?;

    let redirect_uri = format!("http://127.0.0.1:{}/callback", config.redirect_port);
    let client = BasicClient::new(ClientId::new(config.client_id.clone()))
        .set_auth_uri(AuthUrl::new(config.auth_url.clone()).context("invalid auth_url")?)
        .set_token_uri(TokenUrl::new(config.token_url.clone()).context("invalid token_url")?)
        .set_redirect_uri(RedirectUrl::new(redirect_uri).context("invalid redirect_uri")?);
    let client = if let Some(secret) = &config.client_secret {
        client.set_client_secret(ClientSecret::new(secret.clone()))
    } else {
        client
    };

    let http = reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    let token_resp = client
        .exchange_refresh_token(&RefreshToken::new(refresh_token.clone()))
        .request_async(&http)
        .await
        .map_err(|err| anyhow!("refresh failed: {err}"))?;

    let stored = StoredTokens {
        access_token: token_resp.access_token().secret().clone(),
        refresh_token: token_resp
            .refresh_token()
            .map(|t| t.secret().clone())
            .or(Some(refresh_token)),
        expires_at: token_resp
            .expires_in()
            .map(|d| chrono::Utc::now().timestamp() + d.as_secs() as i64),
        token_type: current.token_type,
        scopes: current.scopes,
    };
    save_tokens_for(config.namespace, &config.storage_key, &stored)?;
    Ok(stored)
}

fn wait_for_callback(port: u16) -> Result<(String, CsrfToken)> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .with_context(|| format!("cannot bind 127.0.0.1:{port} for OAuth callback"))?;
    listener
        .set_nonblocking(false)
        .context("set_nonblocking on callback listener")?;

    // One-shot accept with a generous timeout.
    listener.set_ttl(60).ok();

    let (mut stream, _) = listener
        .accept()
        .context("failed to accept OAuth callback")?;
    stream.set_read_timeout(Some(Duration::from_secs(60))).ok();

    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).context("read callback request")?;
    let req = String::from_utf8_lossy(&buf[..n]).to_string();
    let first_line = req.lines().next().unwrap_or_default();
    let path = first_line.split_whitespace().nth(1).unwrap_or("/");

    let query = path.split_once('?').map(|(_, q)| q).unwrap_or_default();
    let mut code = None;
    let mut state = None;
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let decoded = percent_decode(v);
        match k {
            "code" => code = Some(decoded),
            "state" => state = Some(decoded),
            _ => {}
        }
    }

    let body =
        "<html><body><h2>appctl: login successful — you can close this tab.</h2></body></html>";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();

    let code = code.context("callback missing `code`")?;
    let state = state.context("callback missing `state`")?;
    Ok((code, CsrfToken::new(state)))
}

fn percent_decode(s: &str) -> String {
    let mut out = Vec::with_capacity(s.len());
    let mut chars = s.bytes();
    while let Some(b) = chars.next() {
        if b == b'+' {
            out.push(b' ');
        } else if b == b'%' {
            let h1 = chars.next();
            let h2 = chars.next();
            if let (Some(h1), Some(h2)) = (h1, h2)
                && let (Ok(h1), Ok(h2)) = (
                    u8::from_str_radix(std::str::from_utf8(&[h1]).unwrap_or("0"), 16),
                    u8::from_str_radix(std::str::from_utf8(&[h2]).unwrap_or("0"), 16),
                )
            {
                out.push(h1 * 16 + h2);
            }
        } else {
            out.push(b);
        }
    }
    String::from_utf8(out).unwrap_or_default()
}

fn webbrowser_open(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn()?;
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(url).spawn()?;
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()?;
        return Ok(());
    }
    #[allow(unreachable_code)]
    Err(anyhow!("cannot open browser on this platform"))
}
