//! HTTP route probes for synced schemas (`appctl doctor`).

use std::time::Duration;

use crate::{
    config::{AppConfig, ConfigPaths, write_json},
    executor::build_headers,
    schema::{HttpMethod, Provenance, Transport},
    sync::load_schema,
    tools::schema_to_tools,
};
use anyhow::{Context, Result};
use reqwest::Method;

#[derive(Debug, Clone)]
pub struct DoctorRunArgs {
    pub write: bool,
    pub timeout_secs: u64,
}

pub async fn run_doctor(paths: &ConfigPaths, args: DoctorRunArgs) -> Result<()> {
    let mut schema = load_schema(paths)?;
    let config = AppConfig::load_or_init(paths)?;
    let client = reqwest::Client::new();
    let timeout = Duration::from_secs(args.timeout_secs.max(1));

    let base = schema
        .base_url
        .clone()
        .or_else(|| config.target.base_url.clone())
        .context("schema has no base_url; pass --base-url on sync or set target.base_url")?;

    let headers = build_headers(&schema.auth, &config, None)?;

    println!(
        "{:<32} {:<6} {:<48} {:>5}  verdict",
        "tool", "method", "path", "HTTP"
    );
    println!("{}", "-".repeat(100));

    let mut any_http = false;
    let mut updates: Vec<(String, u16, bool)> = Vec::new();

    for resource in &schema.resources {
        for action in &resource.actions {
            let Transport::Http {
                method: ref hm,
                ref path,
                ..
            } = action.transport
            else {
                continue;
            };
            any_http = true;
            let path_resolved = resolve_path_placeholders(path);
            let url = format!(
                "{}{}",
                base.trim_end_matches('/'),
                path_resolved.trim_start_matches('/')
            );

            let (status, verdict) =
                match probe_http_tool(&client, hm, &url, headers.clone(), timeout).await {
                    Ok(code) => {
                        let ok = verifies_route(code);
                        let v = if ok {
                            "reachable"
                        } else if code == 404 {
                            "missing (404)"
                        } else {
                            "check"
                        };
                        (code, v.to_string())
                    }
                    Err(e) => (0, format!("error: {e:#}")),
                };

            let verified = status != 0 && status != 404;
            updates.push((action.name.clone(), status, verified));

            println!(
                "{:<32} {:<6} {:<48} {:>5}  {}",
                action.name,
                http_method_label(hm),
                truncate(&path_resolved, 48),
                if status == 0 {
                    "-".to_string()
                } else {
                    status.to_string()
                },
                verdict
            );
        }
    }

    if !any_http {
        println!("(no HTTP tools in schema — nothing to probe)");
        return Ok(());
    }

    if args.write {
        let mut changed = 0;
        for resource in &mut schema.resources {
            for action in &mut resource.actions {
                if let Some((_, status, verified)) =
                    updates.iter().find(|(n, _, _)| n == &action.name)
                {
                    if *verified && *status != 404 && action.provenance != Provenance::Verified {
                        action.provenance = Provenance::Verified;
                        changed += 1;
                    }
                }
            }
        }
        let tools = schema_to_tools(&schema);
        write_json(&paths.schema, &schema)?;
        write_json(&paths.tools, &tools)?;
        println!(
            "\nWrote {} provenance update(s) to {} (use --write only after reviewing probes).",
            changed,
            paths.schema.display()
        );
    } else {
        println!(
            "\nTip: pass --write to mark reachable (non-404) routes as provenance=verified in the schema."
        );
    }

    if let Some(w) = schema.metadata.get("warnings") {
        println!("\nSync warnings: {}", w);
    }

    Ok(())
}

fn verifies_route(status: u16) -> bool {
    status != 404 && status != 0
}

fn http_method_label(m: &HttpMethod) -> &'static str {
    match m {
        HttpMethod::GET => "GET",
        HttpMethod::POST => "POST",
        HttpMethod::PUT => "PUT",
        HttpMethod::PATCH => "PATCH",
        HttpMethod::DELETE => "DELETE",
    }
}

fn resolve_path_placeholders(path: &str) -> String {
    path.replace("{id}", "1")
        .replace("{Id}", "1")
        .replace("{uuid}", "00000000-0000-0000-0000-000000000001")
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}

/// Probe without side effects: HEAD/OPTIONS first, then light GET for read routes.
async fn probe_http_tool(
    client: &reqwest::Client,
    tool_method: &HttpMethod,
    url: &str,
    headers: reqwest::header::HeaderMap,
    timeout: Duration,
) -> Result<u16> {
    match tool_method {
        HttpMethod::GET => {
            if let Ok(resp) = client
                .request(Method::HEAD, url)
                .headers(headers.clone())
                .timeout(timeout)
                .send()
                .await
            {
                let c = resp.status().as_u16();
                if c != 405 && c != 404 {
                    return Ok(c);
                }
            }
            let resp = client
                .get(url)
                .headers(headers)
                .timeout(timeout)
                .send()
                .await?;
            Ok(resp.status().as_u16())
        }
        HttpMethod::DELETE => {
            if let Ok(resp) = client
                .request(Method::HEAD, url)
                .headers(headers.clone())
                .timeout(timeout)
                .send()
                .await
            {
                return Ok(resp.status().as_u16());
            }
            let resp = client
                .request(Method::OPTIONS, url)
                .headers(headers)
                .timeout(timeout)
                .send()
                .await?;
            Ok(resp.status().as_u16())
        }
        HttpMethod::POST | HttpMethod::PUT | HttpMethod::PATCH => {
            if let Ok(resp) = client
                .request(Method::OPTIONS, url)
                .headers(headers.clone())
                .timeout(timeout)
                .send()
                .await
            {
                let c = resp.status().as_u16();
                if c != 404 {
                    return Ok(c);
                }
            }
            let resp = client
                .request(Method::HEAD, url)
                .headers(headers)
                .timeout(timeout)
                .send()
                .await?;
            Ok(resp.status().as_u16())
        }
    }
}
