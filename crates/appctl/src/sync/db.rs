use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};
use mongodb::bson::Document;
use serde_json::{Map, json};
use sqlx::Row;
use url::Url;

use crate::schema::{
    Action, AuthStrategy, DatabaseKind, Field, FieldType, NoSqlOperation, ParameterLocation,
    Provenance, Resource, Safety, Schema, SqlOperation, SyncSource, Transport, Verb,
};

use super::SyncPlugin;

/// Options for `sync --db` merged from CLI and `[target]` in config.
#[derive(Debug, Clone, Default)]
pub struct DbIntrospectOptions {
    /// Postgres: only these schemas. Empty = all user-visible non-system schemas.
    pub schema_allowlist: Vec<String>,
    /// Exclude by `table` (any schema) or `schema.table`
    pub table_excludes: Vec<String>,
    /// When true, skip a few common framework / PostGIS internal tables
    pub skip_infra: bool,
}

pub struct DbSync {
    connection_string: String,
    options: DbIntrospectOptions,
}

impl DbSync {
    pub fn new(connection_string: String) -> Self {
        Self::with_options(connection_string, DbIntrospectOptions::default())
    }

    pub fn with_options(connection_string: String, options: DbIntrospectOptions) -> Self {
        Self {
            connection_string,
            options,
        }
    }
}

#[async_trait::async_trait]
impl SyncPlugin for DbSync {
    async fn introspect(&self) -> Result<Schema> {
        let (db_kind, resources, metadata_extra) = if self
            .connection_string
            .starts_with("postgres://")
            || self.connection_string.starts_with("postgresql://")
        {
            let (r, m) = introspect_postgres(&self.connection_string, &self.options).await?;
            (DatabaseKind::Postgres, r, m)
        } else if self.connection_string.starts_with("mysql://") {
            let (r, m) = introspect_mysql(&self.connection_string, &self.options).await?;
            (DatabaseKind::Mysql, r, m)
        } else if self.connection_string.starts_with("sqlite:") {
            let (r, m) = introspect_sqlite(&self.connection_string, &self.options).await?;
            (DatabaseKind::Sqlite, r, m)
        } else if self.connection_string.starts_with("mongodb://")
            || self.connection_string.starts_with("mongodb+srv://")
        {
            (
                DatabaseKind::Mongodb,
                introspect_mongodb(&self.connection_string).await?,
                Map::new(),
            )
        } else if self.connection_string.starts_with("redis://")
            || self.connection_string.starts_with("rediss://")
        {
            (
                DatabaseKind::Redis,
                introspect_redis(&self.connection_string).await?,
                Map::new(),
            )
        } else if self.connection_string.starts_with("firestore://") {
            (
                DatabaseKind::Firestore,
                introspect_firestore(&self.connection_string).await?,
                Map::new(),
            )
        } else if self.connection_string.starts_with("dynamodb://") {
            (
                DatabaseKind::Dynamodb,
                introspect_dynamodb(&self.connection_string).await?,
                Map::new(),
            )
        } else {
            bail!(
                "unsupported database connection string; expected postgres://, mysql://, sqlite:, mongodb://, redis://, firestore://, or dynamodb://"
            );
        };

        let mut metadata = {
            let mut meta = Map::new();
            meta.insert(
                "database_kind".to_string(),
                serde_json::Value::String(match db_kind {
                    DatabaseKind::Postgres => "postgres".to_string(),
                    DatabaseKind::Mysql => "mysql".to_string(),
                    DatabaseKind::Sqlite => "sqlite".to_string(),
                    DatabaseKind::Mongodb => "mongodb".to_string(),
                    DatabaseKind::Redis => "redis".to_string(),
                    DatabaseKind::Firestore => "firestore".to_string(),
                    DatabaseKind::Dynamodb => "dynamodb".to_string(),
                }),
            );
            meta
        };
        metadata.extend(metadata_extra);

        Ok(Schema {
            source: SyncSource::Db,
            base_url: None,
            auth: AuthStrategy::None,
            resources,
            metadata,
        })
    }
}

fn db_sync_metadata_for_sql(
    options: &DbIntrospectOptions,
    schema_count: u64,
    table_count: u64,
) -> Map<String, serde_json::Value> {
    let mut m = Map::new();
    let scope = if options.schema_allowlist.is_empty()
        && options.table_excludes.is_empty()
        && !options.skip_infra
    {
        "all_non_system"
    } else {
        "filtered"
    };
    m.insert("db_introspect_scope".to_string(), json!(scope));
    m.insert(
        "db_introspect_schema_count".to_string(),
        json!(schema_count),
    );
    m.insert("db_introspect_table_count".to_string(), json!(table_count));
    m
}

/// Postgres / MySQL: rows from information_schema; SQLite: (None, name).
fn should_skip_table(options: &DbIntrospectOptions, schema: Option<&str>, table: &str) -> bool {
    if is_user_excluded(options, schema, table) {
        return true;
    }
    if options.skip_infra && is_opt_in_infra_table(table) {
        return true;
    }
    false
}

fn is_user_excluded(options: &DbIntrospectOptions, schema: Option<&str>, table: &str) -> bool {
    for pat in &options.table_excludes {
        let pat = pat.trim();
        if pat.is_empty() {
            continue;
        }
        if let Some((s, t)) = pat.split_once('.') {
            if Some(s) == schema && t == table {
                return true;
            }
        } else if pat == table {
            return true;
        }
    }
    false
}

fn is_opt_in_infra_table(name: &str) -> bool {
    matches!(name, "__EFMigrationsHistory" | "spatial_ref_sys")
}

async fn introspect_postgres(
    connection_string: &str,
    options: &DbIntrospectOptions,
) -> Result<(Vec<Resource>, Map<String, serde_json::Value>)> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(connection_string)
        .await
        .context("failed to connect to postgres")?;

    let tables = sqlx::query(
        "select table_schema, table_name from information_schema.tables \
         where table_type = 'BASE TABLE' \
         and table_schema not in ('pg_catalog', 'information_schema', 'pg_toast') \
         order by table_schema, table_name",
    )
    .fetch_all(&pool)
    .await?;

    let want_schemas: Option<BTreeSet<String>> = if options.schema_allowlist.is_empty() {
        None
    } else {
        let s: BTreeSet<String> = options
            .schema_allowlist
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if s.is_empty() { None } else { Some(s) }
    };

    let mut included_schemas = BTreeSet::new();
    let mut resources = Vec::new();
    for row in tables {
        let schema: String = row.try_get("table_schema")?;
        if schema.starts_with("pg_temp") {
            continue;
        }
        if let Some(set) = &want_schemas
            && !set.contains(&schema)
        {
            continue;
        }
        let table: String = row.try_get("table_name")?;
        if should_skip_table(options, Some(&schema), &table) {
            continue;
        }
        let columns = sqlx::query(
            "select c.column_name, c.data_type, c.is_nullable, tc.constraint_type
             from information_schema.columns c
             left join information_schema.key_column_usage kcu
               on c.table_name = kcu.table_name and c.column_name = kcu.column_name and c.table_schema = kcu.table_schema
             left join information_schema.table_constraints tc
               on kcu.constraint_name = tc.constraint_name and kcu.table_schema = tc.table_schema
             where c.table_schema = $1 and c.table_name = $2
             order by c.ordinal_position",
        )
        .bind(&schema)
        .bind(&table)
        .fetch_all(&pool)
        .await?;
        let columns: Vec<ColumnInfo> = columns
            .into_iter()
            .map(|row| ColumnInfo {
                column_name: row.try_get("column_name").unwrap_or_default(),
                data_type: row.try_get("data_type").unwrap_or_default(),
                is_nullable: row
                    .try_get("is_nullable")
                    .unwrap_or_else(|_| "YES".to_string()),
                constraint_type: row.try_get("constraint_type").ok(),
            })
            .collect();
        included_schemas.insert(schema.clone());
        let sch = Some(schema);
        resources.push(resource_from_table(
            sch,
            &table,
            &columns,
            DatabaseKind::Postgres,
        ));
    }
    let n_schema = included_schemas.len() as u64;
    let n_table = resources.len() as u64;
    let meta = db_sync_metadata_for_sql(options, n_schema, n_table);
    Ok((resources, meta))
}

async fn introspect_mysql(
    connection_string: &str,
    options: &DbIntrospectOptions,
) -> Result<(Vec<Resource>, Map<String, serde_json::Value>)> {
    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(5)
        .connect(connection_string)
        .await
        .context("failed to connect to mysql")?;

    let db_name: String = sqlx::query_scalar("select database()")
        .fetch_one(&pool)
        .await
        .context("failed to determine mysql database")?;

    let tables = sqlx::query(
        "select table_name from information_schema.tables where table_schema = ? and table_type='BASE TABLE' order by table_name",
    )
    .bind(&db_name)
    .fetch_all(&pool)
    .await?;

    let mut resources = Vec::new();
    for row in tables {
        let table: String = row.try_get("table_name")?;
        if should_skip_table(options, Some(&db_name), &table) {
            continue;
        }
        let columns = sqlx::query(
            "select c.column_name, c.data_type, c.is_nullable, tc.constraint_type
             from information_schema.columns c
             left join information_schema.key_column_usage kcu
               on c.table_name = kcu.table_name and c.column_name = kcu.column_name and c.table_schema = kcu.table_schema
             left join information_schema.table_constraints tc
               on kcu.constraint_name = tc.constraint_name and kcu.table_schema = tc.table_schema
             where c.table_schema = ? and c.table_name = ?
             order by c.ordinal_position",
        )
        .bind(&db_name)
        .bind(&table)
        .fetch_all(&pool)
        .await?;
        let columns = columns
            .into_iter()
            .map(|row| ColumnInfo {
                column_name: row.try_get("column_name").unwrap_or_default(),
                data_type: row.try_get("data_type").unwrap_or_default(),
                is_nullable: row
                    .try_get("is_nullable")
                    .unwrap_or_else(|_| "YES".to_string()),
                constraint_type: row.try_get("constraint_type").ok(),
            })
            .collect::<Vec<_>>();

        resources.push(resource_from_table(
            Some(db_name.clone()),
            &table,
            &columns,
            DatabaseKind::Mysql,
        ));
    }
    let n_table = resources.len() as u64;
    let meta = db_sync_metadata_for_sql(options, 1, n_table);
    Ok((resources, meta))
}

async fn introspect_sqlite(
    connection_string: &str,
    options: &DbIntrospectOptions,
) -> Result<(Vec<Resource>, Map<String, serde_json::Value>)> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(5)
        .connect(connection_string)
        .await
        .context("failed to connect to sqlite")?;

    let tables = sqlx::query(
        "select name from sqlite_master where type = 'table' and name not like 'sqlite_%' order by name",
    )
    .fetch_all(&pool)
    .await?;

    let mut resources = Vec::new();
    for row in tables {
        let table: String = row.try_get("name")?;
        if should_skip_table(options, None, &table) {
            continue;
        }
        let escaped_table = table.replace('\'', "''");
        let pragma = format!("pragma table_info('{escaped_table}')");
        let columns = sqlx::query(&pragma).fetch_all(&pool).await?;
        let columns = columns
            .into_iter()
            .map(|row| {
                let pk: i64 = row.try_get("pk").unwrap_or(0);
                let notnull: i64 = row.try_get("notnull").unwrap_or(0);
                ColumnInfo {
                    column_name: row.try_get("name").unwrap_or_default(),
                    data_type: row
                        .try_get::<String, _>("type")
                        .unwrap_or_else(|_| "text".to_string())
                        .to_lowercase(),
                    is_nullable: if notnull == 1 {
                        "NO".to_string()
                    } else {
                        "YES".to_string()
                    },
                    constraint_type: if pk > 0 {
                        Some("PRIMARY KEY".to_string())
                    } else {
                        None
                    },
                }
            })
            .collect::<Vec<_>>();

        resources.push(resource_from_table(
            None,
            &table,
            &columns,
            DatabaseKind::Sqlite,
        ));
    }
    let n_table = resources.len() as u64;
    let meta = db_sync_metadata_for_sql(options, 1, n_table);
    Ok((resources, meta))
}

struct ColumnInfo {
    column_name: String,
    data_type: String,
    is_nullable: String,
    constraint_type: Option<String>,
}

fn sql_resource_label(
    schema: Option<&str>,
    table: &str,
    db_kind: &DatabaseKind,
) -> (String, String) {
    let table_label = table.trim().to_string();
    let desc = match (db_kind, schema) {
        (DatabaseKind::Postgres | DatabaseKind::Mysql, Some(s)) if !s.is_empty() => {
            format!("Table {s}.{}", table_label)
        }
        _ => format!("Table {table_label}"),
    };
    let name = match (db_kind, schema) {
        (DatabaseKind::Postgres, Some(s)) if !s.is_empty() => {
            let ts = table_singular_stem(&table_label);
            format!("{}__{}", ident_part(s), ident_part(&ts))
        }
        (DatabaseKind::Mysql, Some(s)) if !s.is_empty() => {
            let ts = table_singular_stem(&table_label);
            format!("{}__{}", ident_part(s), ident_part(&ts))
        }
        _ => table_singular_stem(&table_label),
    };
    (name, desc)
}

fn table_singular_stem(table: &str) -> String {
    let t = table.trim();
    if t.is_empty() {
        return "table".to_string();
    }
    ident_part(t)
}

fn ident_part(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// `schema` is the Postgres / MySQL namespace; `None` for SQLite (unqualified).
fn resource_from_table(
    schema: Option<String>,
    table: &str,
    rows: &[ColumnInfo],
    db_kind: DatabaseKind,
) -> Resource {
    let mut fields = Vec::new();
    let mut seen_columns = BTreeSet::<String>::new();
    let mut primary_keys = Vec::<String>::new();

    for row in rows {
        if row.constraint_type.as_deref() == Some("PRIMARY KEY")
            && !primary_keys.contains(&row.column_name)
        {
            primary_keys.push(row.column_name.clone());
        }
        if !seen_columns.insert(row.column_name.clone()) {
            continue;
        }

        fields.push(Field {
            name: row.column_name.clone(),
            description: None,
            field_type: sql_type_to_field_type(&row.data_type),
            required: row.is_nullable == "NO",
            location: Some(ParameterLocation::Body),
            default: None,
            enum_values: Vec::new(),
        });
    }

    let single_primary_key = match primary_keys.as_slice() {
        [pk] => Some(pk.clone()),
        [] => Some("id".to_string()),
        _ => None,
    };

    let (resource_name, table_desc) = sql_resource_label(schema.as_deref(), table, &db_kind);
    let sql_schema = match db_kind {
        DatabaseKind::Sqlite => None,
        _ => schema,
    };
    let transport_table = table.to_string();
    let mut actions = vec![
        Action {
            name: format!("list_{}", resource_name),
            description: Some(format!("List rows from {}", table_desc)),
            verb: Verb::List,
            transport: Transport::Sql {
                database_kind: db_kind.clone(),
                schema: sql_schema.clone(),
                table: transport_table.clone(),
                operation: SqlOperation::Select,
                primary_key: single_primary_key.clone(),
            },
            parameters: Vec::new(),
            safety: Safety::ReadOnly,
            resource: Some(resource_name.clone()),
            provenance: Provenance::Declared,
            metadata: Map::new(),
        },
        Action {
            name: format!("create_{}", resource_name),
            description: Some(format!("Insert one row into {}", table_desc)),
            verb: Verb::Create,
            transport: Transport::Sql {
                database_kind: db_kind.clone(),
                schema: sql_schema.clone(),
                table: transport_table.clone(),
                operation: SqlOperation::Insert,
                primary_key: single_primary_key.clone(),
            },
            parameters: fields
                .iter()
                .filter(|field| {
                    single_primary_key
                        .as_ref()
                        .is_none_or(|pk| field.name != *pk)
                })
                .cloned()
                .collect(),
            safety: Safety::Mutating,
            resource: Some(resource_name.clone()),
            provenance: Provenance::Declared,
            metadata: Map::new(),
        },
    ];

    if let Some(pk) = single_primary_key {
        let pk_field = Field {
            name: pk.clone(),
            description: Some("Primary key".to_string()),
            field_type: FieldType::Integer,
            required: true,
            location: Some(ParameterLocation::Path),
            default: None,
            enum_values: Vec::new(),
        };
        actions.extend([
            Action {
                name: format!("get_{}", resource_name),
                description: Some(format!("Fetch one row from {}", table_desc)),
                verb: Verb::Get,
                transport: Transport::Sql {
                    database_kind: db_kind.clone(),
                    schema: sql_schema.clone(),
                    table: transport_table.clone(),
                    operation: SqlOperation::GetByPk,
                    primary_key: Some(pk.clone()),
                },
                parameters: vec![pk_field.clone()],
                safety: Safety::ReadOnly,
                resource: Some(resource_name.clone()),
                provenance: Provenance::Declared,
                metadata: Map::new(),
            },
            Action {
                name: format!("update_{}", resource_name),
                description: Some(format!("Update one row in {}", table_desc)),
                verb: Verb::Update,
                transport: Transport::Sql {
                    database_kind: db_kind.clone(),
                    schema: sql_schema.clone(),
                    table: transport_table.clone(),
                    operation: SqlOperation::UpdateByPk,
                    primary_key: Some(pk.clone()),
                },
                parameters: {
                    let mut params = vec![pk_field.clone()];
                    params.extend(fields.iter().filter(|field| field.name != pk).cloned());
                    params
                },
                safety: Safety::Mutating,
                resource: Some(resource_name.clone()),
                provenance: Provenance::Declared,
                metadata: Map::new(),
            },
            Action {
                name: format!("delete_{}", resource_name),
                description: Some(format!("Delete one row from {}", table_desc)),
                verb: Verb::Delete,
                transport: Transport::Sql {
                    database_kind: db_kind.clone(),
                    schema: sql_schema.clone(),
                    table: transport_table.clone(),
                    operation: SqlOperation::DeleteByPk,
                    primary_key: Some(pk),
                },
                parameters: vec![pk_field],
                safety: Safety::Destructive,
                resource: Some(resource_name.clone()),
                provenance: Provenance::Declared,
                metadata: Map::new(),
            },
        ]);
    }

    let mut metadata = Map::new();
    if primary_keys.len() > 1 {
        metadata.insert(
            "primary_key_warning".to_string(),
            json!("composite primary key detected; get/update/delete tools are not generated yet"),
        );
        metadata.insert("primary_keys".to_string(), json!(primary_keys));
    }

    Resource {
        name: resource_name.clone(),
        description: Some(table_desc.clone()),
        fields: fields.clone(),
        actions,
        metadata,
    }
}

fn sql_type_to_field_type(data_type: &str) -> FieldType {
    match data_type {
        "integer" | "bigint" | "smallint" | "serial" | "bigserial" | "int" => FieldType::Integer,
        "numeric" | "decimal" | "float" | "double" | "real" | "double precision" => {
            FieldType::Number
        }
        "bool" | "boolean" => FieldType::Boolean,
        "timestamp" | "timestamp without time zone" | "timestamp with time zone" | "datetime" => {
            FieldType::DateTime
        }
        "date" => FieldType::Date,
        "uuid" => FieldType::Uuid,
        "json" | "jsonb" => FieldType::Json,
        value if value.contains("int") => FieldType::Integer,
        value if value.contains("char") || value.contains("text") || value.contains("clob") => {
            FieldType::String
        }
        value if value.contains("real") || value.contains("floa") || value.contains("doub") => {
            FieldType::Number
        }
        value if value.contains("bool") => FieldType::Boolean,
        value if value.contains("date") || value.contains("time") => FieldType::DateTime,
        value if value.contains("json") => FieldType::Json,
        _ => FieldType::String,
    }
}

async fn introspect_mongodb(connection_string: &str) -> Result<Vec<Resource>> {
    let client = mongodb::Client::with_uri_str(connection_string)
        .await
        .context("failed to connect to mongodb")?;
    let db = client
        .default_database()
        .context("mongodb connection string must include a default database name")?;
    let collections = db
        .list_collection_names()
        .await
        .context("failed to list mongodb collections")?;
    let mut resources = Vec::new();
    for collection in collections {
        let coll = db.collection::<Document>(&collection);
        let sample = coll.find_one(Document::new()).await.ok().flatten();
        resources.push(resource_from_document_store(
            DatabaseKind::Mongodb,
            &collection,
            sample
                .as_ref()
                .and_then(|doc| mongodb::bson::to_bson(doc).ok())
                .and_then(|value| mongodb::bson::from_bson::<serde_json::Value>(value).ok())
                .as_ref(),
            "_id",
            None,
        ));
    }
    Ok(resources)
}

async fn introspect_redis(_connection_string: &str) -> Result<Vec<Resource>> {
    Ok(vec![resource_from_kv_store(
        DatabaseKind::Redis,
        "redis_keys",
        "key",
    )])
}

async fn introspect_firestore(connection_string: &str) -> Result<Vec<Resource>> {
    let project = firestore_project(connection_string)?;
    let token = crate::auth::gcloud::adc_access_token(Some(&project))?;
    let client = reqwest::Client::new();
    let url = format!(
        "https://firestore.googleapis.com/v1/projects/{project}/databases/(default)/documents:listCollectionIds"
    );
    let response = client
        .post(url)
        .bearer_auth(token.access_token)
        .json(&serde_json::json!({}))
        .send()
        .await
        .context("failed to query firestore collection ids")?
        .json::<serde_json::Value>()
        .await
        .context("failed to parse firestore collection id response")?;

    let collection_ids = response
        .get("collectionIds")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();

    if collection_ids.is_empty() {
        return Ok(vec![resource_from_document_store(
            DatabaseKind::Firestore,
            "documents",
            None,
            "id",
            None,
        )]);
    }

    Ok(collection_ids
        .into_iter()
        .filter_map(|value| value.as_str().map(|name| name.to_string()))
        .map(|name| resource_from_document_store(DatabaseKind::Firestore, &name, None, "id", None))
        .collect())
}

async fn introspect_dynamodb(connection_string: &str) -> Result<Vec<Resource>> {
    let (region, endpoint) = dynamodb_config(connection_string)?;
    let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_config::Region::new(region));
    if let Some(endpoint) = endpoint {
        loader = loader.endpoint_url(endpoint);
    }
    let config = loader.load().await;
    let client = aws_sdk_dynamodb::Client::new(&config);
    let response = client
        .list_tables()
        .send()
        .await
        .context("failed to list dynamodb tables")?;
    let mut resources = Vec::new();
    for table_name in response.table_names() {
        let describe = client
            .describe_table()
            .table_name(table_name)
            .send()
            .await
            .with_context(|| format!("failed to describe dynamodb table {table_name}"))?;
        let Some(table) = describe.table() else {
            continue;
        };
        let mut key_iter = table
            .key_schema()
            .iter()
            .map(|key| key.attribute_name().to_string());
        let primary_key = key_iter.next().unwrap_or_else(|| "id".to_string());
        let secondary_key = key_iter.next();
        resources.push(resource_from_document_store(
            DatabaseKind::Dynamodb,
            table_name,
            None,
            &primary_key,
            secondary_key.as_deref(),
        ));
    }
    Ok(resources)
}

fn resource_from_document_store(
    db_kind: DatabaseKind,
    collection: &str,
    sample: Option<&serde_json::Value>,
    primary_key: &str,
    secondary_key: Option<&str>,
) -> Resource {
    let resource_name = ident_part(collection);
    let mut fields = vec![Field {
        name: primary_key.to_string(),
        description: Some("Primary key".to_string()),
        field_type: FieldType::String,
        required: true,
        location: Some(ParameterLocation::Path),
        default: None,
        enum_values: Vec::new(),
    }];
    if let Some(secondary_key) = secondary_key {
        fields.push(Field {
            name: secondary_key.to_string(),
            description: Some("Secondary key".to_string()),
            field_type: FieldType::String,
            required: true,
            location: Some(ParameterLocation::Path),
            default: None,
            enum_values: Vec::new(),
        });
    }
    fields.push(Field {
        name: "document".to_string(),
        description: Some("Document payload".to_string()),
        field_type: sample
            .and_then(|value| value.as_object())
            .map(|_| FieldType::Json)
            .unwrap_or(FieldType::Json),
        required: false,
        location: Some(ParameterLocation::Body),
        default: sample.cloned(),
        enum_values: Vec::new(),
    });

    let pk_field = fields[0].clone();
    let secondary_field = secondary_key.map(|name| Field {
        name: name.to_string(),
        description: Some("Secondary key".to_string()),
        field_type: FieldType::String,
        required: true,
        location: Some(ParameterLocation::Path),
        default: None,
        enum_values: Vec::new(),
    });
    let document_field = fields.last().cloned().unwrap();

    Resource {
        name: resource_name.clone(),
        description: Some(format!(
            "{} collection {}",
            nosql_label(&db_kind),
            collection
        )),
        fields,
        actions: vec![
            Action {
                name: format!("list_{}", resource_name),
                description: Some(format!("List records from {}", collection)),
                verb: Verb::List,
                transport: Transport::NoSql {
                    database_kind: db_kind.clone(),
                    collection: collection.to_string(),
                    operation: NoSqlOperation::List,
                    primary_key: Some(primary_key.to_string()),
                    secondary_key: secondary_key.map(str::to_string),
                },
                parameters: Vec::new(),
                safety: Safety::ReadOnly,
                resource: Some(resource_name.clone()),
                provenance: Provenance::Declared,
                metadata: Map::new(),
            },
            Action {
                name: format!("get_{}", resource_name),
                description: Some(format!("Fetch one record from {}", collection)),
                verb: Verb::Get,
                transport: Transport::NoSql {
                    database_kind: db_kind.clone(),
                    collection: collection.to_string(),
                    operation: NoSqlOperation::GetByPk,
                    primary_key: Some(primary_key.to_string()),
                    secondary_key: secondary_key.map(str::to_string),
                },
                parameters: {
                    let mut params = vec![pk_field.clone()];
                    if let Some(secondary_field) = &secondary_field {
                        params.push(secondary_field.clone());
                    }
                    params
                },
                safety: Safety::ReadOnly,
                resource: Some(resource_name.clone()),
                provenance: Provenance::Declared,
                metadata: Map::new(),
            },
            Action {
                name: format!("create_{}", resource_name),
                description: Some(format!("Create one record in {}", collection)),
                verb: Verb::Create,
                transport: Transport::NoSql {
                    database_kind: db_kind.clone(),
                    collection: collection.to_string(),
                    operation: NoSqlOperation::Insert,
                    primary_key: Some(primary_key.to_string()),
                    secondary_key: secondary_key.map(str::to_string),
                },
                parameters: vec![document_field.clone()],
                safety: Safety::Mutating,
                resource: Some(resource_name.clone()),
                provenance: Provenance::Declared,
                metadata: Map::new(),
            },
            Action {
                name: format!("update_{}", resource_name),
                description: Some(format!("Update one record in {}", collection)),
                verb: Verb::Update,
                transport: Transport::NoSql {
                    database_kind: db_kind.clone(),
                    collection: collection.to_string(),
                    operation: NoSqlOperation::UpdateByPk,
                    primary_key: Some(primary_key.to_string()),
                    secondary_key: secondary_key.map(str::to_string),
                },
                parameters: {
                    let mut params = vec![pk_field.clone()];
                    if let Some(secondary_field) = &secondary_field {
                        params.push(secondary_field.clone());
                    }
                    params.push(document_field.clone());
                    params
                },
                safety: Safety::Mutating,
                resource: Some(resource_name.clone()),
                provenance: Provenance::Declared,
                metadata: Map::new(),
            },
            Action {
                name: format!("delete_{}", resource_name),
                description: Some(format!("Delete one record from {}", collection)),
                verb: Verb::Delete,
                transport: Transport::NoSql {
                    database_kind: db_kind,
                    collection: collection.to_string(),
                    operation: NoSqlOperation::DeleteByPk,
                    primary_key: Some(primary_key.to_string()),
                    secondary_key: secondary_key.map(str::to_string),
                },
                parameters: {
                    let mut params = vec![pk_field];
                    if let Some(secondary_field) = secondary_field {
                        params.push(secondary_field);
                    }
                    params
                },
                safety: Safety::Destructive,
                resource: Some(resource_name),
                provenance: Provenance::Declared,
                metadata: Map::new(),
            },
        ],
        metadata: Map::new(),
    }
}

fn resource_from_kv_store(db_kind: DatabaseKind, collection: &str, primary_key: &str) -> Resource {
    resource_from_document_store(db_kind, collection, None, primary_key, None)
}

fn nosql_label(kind: &DatabaseKind) -> &'static str {
    match kind {
        DatabaseKind::Mongodb => "MongoDB",
        DatabaseKind::Redis => "Redis",
        DatabaseKind::Firestore => "Firestore",
        DatabaseKind::Dynamodb => "DynamoDB",
        DatabaseKind::Postgres => "Postgres",
        DatabaseKind::Mysql => "MySQL",
        DatabaseKind::Sqlite => "SQLite",
    }
}

fn firestore_project(connection_string: &str) -> Result<String> {
    let url = Url::parse(connection_string).context("invalid firestore connection string")?;
    if let Some(host) = url.host_str()
        && !host.is_empty()
    {
        return Ok(host.to_string());
    }
    url.path_segments()
        .and_then(|mut segments| segments.next().map(str::to_string))
        .filter(|value| !value.is_empty())
        .context(
            "firestore connection string must include a project id, e.g. firestore://my-project",
        )
}

fn dynamodb_config(connection_string: &str) -> Result<(String, Option<String>)> {
    let url = Url::parse(connection_string).context("invalid dynamodb connection string")?;
    let region = url
        .host_str()
        .map(str::to_string)
        .or_else(|| {
            url.path_segments()
                .and_then(|mut segments| segments.next().map(str::to_string))
        })
        .filter(|value| !value.is_empty())
        .context("dynamodb connection string must include a region, e.g. dynamodb://us-east-1")?;
    let endpoint = url
        .query_pairs()
        .find(|(key, _)| key == "endpoint")
        .map(|(_, value)| value.to_string());
    Ok((region, endpoint))
}

#[cfg(test)]
mod tests {
    use super::{DbSync, dynamodb_config, firestore_project};
    use crate::sync::SyncPlugin;
    use tempfile::tempdir;

    #[tokio::test]
    async fn sqlite_sync_introspects_tables() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("ordering.db");
        let connection = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&connection)
            .await
            .unwrap();

        sqlx::query(
            "create table products (
                id integer primary key,
                name text not null,
                price real not null
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        drop(pool);

        let schema = DbSync::new(connection).introspect().await.unwrap();
        assert_eq!(schema.resources.len(), 1);
        assert_eq!(schema.metadata["database_kind"], "sqlite");
        let resource = &schema.resources[0];
        assert_eq!(resource.name, "products");
        assert!(
            resource
                .actions
                .iter()
                .any(|action| action.name == "list_products")
        );
        assert!(
            resource
                .actions
                .iter()
                .any(|action| action.name == "get_products")
        );
        assert!(
            resource
                .actions
                .iter()
                .any(|action| action.name == "delete_products")
        );
    }

    #[tokio::test]
    async fn redis_sync_builds_generic_resource() {
        let schema = DbSync::new("redis://127.0.0.1".to_string())
            .introspect()
            .await
            .unwrap();
        assert_eq!(schema.metadata["database_kind"], "redis");
        assert_eq!(schema.resources.len(), 1);
        let resource = &schema.resources[0];
        assert_eq!(resource.name, "redis_keys");
        assert!(
            resource
                .actions
                .iter()
                .any(|action| action.name == "list_redis_keys")
        );
        assert!(
            resource
                .actions
                .iter()
                .any(|action| action.name == "get_redis_keys")
        );
    }

    #[tokio::test]
    async fn sqlite_composite_primary_key_skips_single_row_mutations() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("composite.db");
        let connection = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&connection)
            .await
            .unwrap();

        sqlx::query(
            "create table memberships (
                account_id integer not null,
                user_id integer not null,
                role text not null,
                primary key (account_id, user_id)
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        drop(pool);

        let schema = DbSync::new(connection).introspect().await.unwrap();
        let resource = &schema.resources[0];
        assert_eq!(resource.name, "memberships");
        assert_eq!(resource.fields.len(), 3);
        assert!(resource.metadata.get("primary_keys").is_some());
        assert!(
            resource
                .actions
                .iter()
                .any(|action| action.name == "list_memberships")
        );
        assert!(
            resource
                .actions
                .iter()
                .any(|action| action.name == "create_memberships")
        );
        assert!(
            !resource
                .actions
                .iter()
                .any(|action| action.name == "get_memberships")
        );
        assert!(
            !resource
                .actions
                .iter()
                .any(|action| action.name == "delete_memberships")
        );
    }

    #[test]
    fn firestore_and_dynamodb_connection_strings_parse() {
        assert_eq!(
            firestore_project("firestore://project-123").unwrap(),
            "project-123"
        );
        let (region, endpoint) =
            dynamodb_config("dynamodb://us-east-1?endpoint=http://localhost:8000").unwrap();
        assert_eq!(region, "us-east-1");
        assert_eq!(endpoint.as_deref(), Some("http://localhost:8000"));
    }
}
