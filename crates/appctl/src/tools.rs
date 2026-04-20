use schemars::Schema as JsonSchemaDef;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::schema::{Field, FieldType, Schema};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

pub fn schema_to_tools(schema: &Schema) -> Vec<ToolDef> {
    schema
        .resources
        .iter()
        .flat_map(|resource| {
            resource.actions.iter().map(move |action| ToolDef {
                name: action.name.clone(),
                description: action
                    .description
                    .clone()
                    .unwrap_or_else(|| format!("{} {}", action.verb_label(), resource.name)),
                input_schema: fields_to_input_schema(&action.parameters),
            })
        })
        .collect()
}

pub fn fields_to_input_schema(fields: &[Field]) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();

    for field in fields {
        properties.insert(field.name.clone(), field_to_json_schema(field));
        if field.required {
            required.push(Value::String(field.name.clone()));
        }
    }

    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

fn field_to_json_schema(field: &Field) -> Value {
    let schema_type = match field.field_type {
        FieldType::String | FieldType::DateTime | FieldType::Date | FieldType::Uuid => "string",
        FieldType::Integer => "integer",
        FieldType::Number => "number",
        FieldType::Boolean => "boolean",
        FieldType::Object | FieldType::Json => "object",
        FieldType::Array => "array",
    };

    let mut map = Map::new();
    map.insert("type".to_string(), Value::String(schema_type.to_string()));
    if let Some(description) = &field.description {
        map.insert(
            "description".to_string(),
            Value::String(description.clone()),
        );
    }
    if let Some(default) = &field.default {
        map.insert("default".to_string(), default.clone());
    }
    if !field.enum_values.is_empty() {
        map.insert("enum".to_string(), Value::Array(field.enum_values.clone()));
    }

    match field.field_type {
        FieldType::DateTime => {
            map.insert("format".to_string(), Value::String("date-time".to_string()));
        }
        FieldType::Date => {
            map.insert("format".to_string(), Value::String("date".to_string()));
        }
        FieldType::Uuid => {
            map.insert("format".to_string(), Value::String("uuid".to_string()));
        }
        FieldType::Array => {
            map.insert("items".to_string(), json!({ "type": "string" }));
        }
        _ => {}
    }

    Value::Object(map)
}

trait VerbLabel {
    fn verb_label(&self) -> &'static str;
}

impl VerbLabel for crate::schema::Action {
    fn verb_label(&self) -> &'static str {
        match self.verb {
            crate::schema::Verb::List => "List",
            crate::schema::Verb::Get => "Get",
            crate::schema::Verb::Create => "Create",
            crate::schema::Verb::Update => "Update",
            crate::schema::Verb::Delete => "Delete",
            crate::schema::Verb::Custom => "Call",
        }
    }
}

#[allow(dead_code)]
fn _touch_schemars(_: JsonSchemaDef) {}
