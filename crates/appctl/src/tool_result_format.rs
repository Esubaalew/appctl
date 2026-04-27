//! Shared limits for large tool results (operator LLM, MCP clients): list row cap and char cap.
use serde_json::Value;

use crate::config::BehaviorConfig;
use crate::executor::tool_result_summary;

/// Same text as used in the operator chat path — compact, capped to model/IPC limits.
pub fn format_tool_result_message(
    output: &Value,
    behavior: &BehaviorConfig,
) -> std::result::Result<String, serde_json::Error> {
    let (text, _) = tool_result_capped_for_clients(output, behavior)?;
    Ok(text)
}

/// Capped `text` for the MCP `content[0].text` field and capped `structuredContent` (JSON `Value`).
pub fn tool_result_capped_for_clients(
    output: &Value,
    behavior: &BehaviorConfig,
) -> std::result::Result<(String, Value), serde_json::Error> {
    let (shrunk, list_note) = shrink_list_at_root(output, behavior.max_tool_list_items);
    let list_prefix = list_note
        .as_deref()
        .map(|n| format!("{n}\n"))
        .unwrap_or_default();
    let body = if let Some(summary) = tool_result_summary(output) {
        let json_part = serde_json::to_string(&shrunk)?;
        format!("appctl tool summary: {summary}\n{list_prefix}tool result JSON: {json_part}")
    } else if list_prefix.is_empty() {
        serde_json::to_string(&shrunk)?
    } else {
        format!("{list_prefix}{}", serde_json::to_string(&shrunk)?)
    };
    let text = apply_max_tool_result_chars(body, behavior.max_tool_result_chars);
    let structured = mcp_structured_content(&shrunk, behavior.max_tool_result_chars);
    Ok((text, structured))
}

/// If the root is a JSON array, keep at most N elements (N > 0). 0 = no list cap.
fn shrink_list_at_root(value: &Value, max_list_items: usize) -> (Value, Option<String>) {
    if max_list_items == 0 {
        return (value.clone(), None);
    }
    match value {
        Value::Array(arr) if arr.len() > max_list_items => {
            let total = arr.len();
            let kept: Vec<Value> = arr.iter().take(max_list_items).cloned().collect();
            let note = format!(
                "appctl: showing first {max_list_items} of {total} list items in this result"
            );
            (Value::Array(kept), Some(note))
        }
        _ => (value.clone(), None),
    }
}

fn apply_max_tool_result_chars(s: String, max: usize) -> String {
    if max == 0 {
        return s;
    }
    if s.chars().count() <= max {
        return s;
    }
    const SUFFIX: &str = "\n… [appctl: tool result truncated: max_tool_result_chars]";
    let take = max.saturating_sub(SUFFIX.chars().count());
    if take == 0 {
        return SUFFIX.chars().take(max).collect();
    }
    let mut out: String = s.chars().take(take).collect();
    out.push_str(SUFFIX);
    out
}

/// Prefer a real JSON `Value` for small payloads; if the serial form still exceeds the cap, use
/// a JSON string holding truncated minified JSON so the envelope stays small and valid.
fn mcp_structured_content(shrunk: &Value, max: usize) -> Value {
    if max == 0 {
        return shrunk.clone();
    }
    let s = match serde_json::to_string(shrunk) {
        Ok(s) => s,
        Err(_) => return shrunk.clone(),
    };
    if s.chars().count() <= max {
        return shrunk.clone();
    }
    Value::String(apply_max_tool_result_chars(s, max))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn list_shrink_affects_mcp_structured() {
        let v: Value = (0..5).map(|i| json!({"i": i})).collect();
        let mut b = BehaviorConfig {
            max_tool_list_items: 2,
            ..Default::default()
        };
        b.max_tool_result_chars = 0;
        let (text, structured) = tool_result_capped_for_clients(&v, &b).unwrap();
        let arr = structured.as_array().expect("structural cap off");
        assert_eq!(arr.len(), 2);
        assert!(text.contains("first 2 of 5"));
    }

    #[test]
    fn huge_value_becomes_string_structured() {
        let s = "x".repeat(8_000);
        let v = json!({ "blob": s });
        let b = BehaviorConfig {
            max_tool_list_items: 0,
            max_tool_result_chars: 1000,
            ..Default::default()
        };
        let (text, structured) = tool_result_capped_for_clients(&v, &b).unwrap();
        assert!(matches!(structured, Value::String(_)));
        assert!(text.contains("max_tool_result_chars") || text.len() <= 1500);
    }
}
