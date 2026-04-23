use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Schema {
    pub source: SyncSource,
    #[serde(default)]
    pub base_url: Option<String>,
    pub auth: AuthStrategy,
    #[serde(default)]
    pub resources: Vec<Resource>,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Resource {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub fields: Vec<Field>,
    #[serde(default)]
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    /// Route / operation was guessed from framework conventions (may 404).
    #[default]
    Inferred,
    /// Declared by OpenAPI, explicit plugin, or introspected DB schema.
    Declared,
    /// Confirmed reachable by `appctl doctor` (HTTP probe).
    Verified,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Action {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub verb: Verb,
    pub transport: Transport,
    #[serde(default)]
    pub parameters: Vec<Field>,
    pub safety: Safety,
    #[serde(default)]
    pub resource: Option<String>,
    /// How we know this action exists (inferred routes are often wrong for non-API Django apps).
    #[serde(default)]
    pub provenance: Provenance,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Field {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub field_type: FieldType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub location: Option<ParameterLocation>,
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default)]
    pub enum_values: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncSource {
    Openapi,
    Django,
    Db,
    Url,
    Mcp,
    Rails,
    Laravel,
    Aspnet,
    Strapi,
    Supabase,
    Plugin,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Verb {
    List,
    Get,
    Create,
    Update,
    Delete,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Safety {
    ReadOnly,
    Mutating,
    Destructive,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Transport {
    Http {
        method: HttpMethod,
        path: String,
        #[serde(default)]
        query: Vec<String>,
    },
    Sql {
        database_kind: DatabaseKind,
        table: String,
        operation: SqlOperation,
        #[serde(default)]
        primary_key: Option<String>,
    },
    Form {
        method: HttpMethod,
        action: String,
    },
    Mcp {
        server_url: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseKind {
    Postgres,
    Mysql,
    Sqlite,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SqlOperation {
    Select,
    GetByPk,
    Insert,
    UpdateByPk,
    DeleteByPk,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    PATCH,
    DELETE,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthStrategy {
    None,
    ApiKey {
        header: String,
        env_ref: String,
    },
    Bearer {
        env_ref: String,
    },
    Basic {
        username_ref: String,
        password_ref: String,
    },
    Cookie {
        #[serde(default)]
        env_ref: Option<String>,
        #[serde(default)]
        session_file: Option<String>,
    },
    OAuth2 {
        #[serde(default)]
        provider: Option<String>,
        client_id_ref: String,
        #[serde(default)]
        client_secret_ref: Option<String>,
        auth_url: String,
        token_url: String,
        #[serde(default)]
        scopes: Vec<String>,
        #[serde(default = "default_redirect_port")]
        redirect_port: u16,
    },
}

fn default_redirect_port() -> u16 {
    8421
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    String,
    Integer,
    Number,
    Boolean,
    Object,
    Array,
    DateTime,
    Date,
    Uuid,
    Json,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ParameterLocation {
    Path,
    Query,
    Body,
    Header,
}

impl Schema {
    pub fn action(&self, name: &str) -> Option<&Action> {
        self.resources
            .iter()
            .flat_map(|resource| resource.actions.iter())
            .find(|action| action.name == name)
    }
}

impl FieldType {
    pub fn from_openapi_type(ty: Option<&str>, format: Option<&str>) -> Self {
        match (ty.unwrap_or("string"), format.unwrap_or_default()) {
            ("integer", _) => Self::Integer,
            ("number", _) => Self::Number,
            ("boolean", _) => Self::Boolean,
            ("object", _) => Self::Object,
            ("array", _) => Self::Array,
            ("string", "date-time") => Self::DateTime,
            ("string", "date") => Self::Date,
            ("string", "uuid") => Self::Uuid,
            ("string", _) => Self::String,
            _ => Self::Json,
        }
    }
}
