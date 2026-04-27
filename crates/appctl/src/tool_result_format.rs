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
    Ok(add_model_next_step_hints(
        text,
        output,
        behavior.max_tool_result_chars,
    ))
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

fn add_model_next_step_hints(text: String, output: &Value, max_chars: usize) -> String {
    let mut hints = Vec::new();

    if text.contains("showing first") {
        hints.push(
            "If the target row is not visible, call the same list tool again with `filter`, or page with `offset`/`limit`.",
        );
    }

    match output {
        Value::Array(rows) if rows.is_empty() => {
            hints.push(
                "This list result is empty. Try another likely filter column, relax the filter, or explain which lookup path failed.",
            );
        }
        Value::Array(rows) if rows.iter().take(5).any(row_has_id_like_key) => {
            hints.push(
                "Use returned `id` or `*_id` values as inputs to related `get_*` or filtered `list_*` tools before answering.",
            );
        }
        _ => {}
    }

    if hints.is_empty() {
        return text;
    }

    let mut out = text;
    out.push_str("\nappctl next step hints:\n");
    for hint in hints {
        out.push_str("- ");
        out.push_str(hint);
        out.push('\n');
    }
    apply_max_tool_result_chars(out, max_chars)
}

fn row_has_id_like_key(row: &Value) -> bool {
    let Some(obj) = row.as_object() else {
        return false;
    };
    obj.keys()
        .any(|key| key == "id" || key.ends_with("_id") || key.ends_with("_ids"))
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
    fn model_message_adds_list_next_step_hints() {
        let v = json!([
            { "id": "record-1", "parcel_id": "parcel-1", "old_code": "DD001" },
            { "id": "record-2", "parcel_id": "parcel-2", "old_code": "DD002" }
        ]);
        let b = BehaviorConfig {
            max_tool_list_items: 1,
            max_tool_result_chars: 0,
            ..Default::default()
        };
        let text = format_tool_result_message(&v, &b).unwrap();
        assert!(text.contains("appctl next step hints"));
        assert!(text.contains("filter"));
        assert!(text.contains("`*_id`"));
    }

    #[test]
    fn model_message_adds_empty_list_hint() {
        let b = BehaviorConfig {
            max_tool_result_chars: 0,
            ..Default::default()
        };
        let text = format_tool_result_message(&json!([]), &b).unwrap();
        assert!(text.contains("This list result is empty"));
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
