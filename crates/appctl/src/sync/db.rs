use anyhow::{Context, Result, bail};
use mongodb::bson::Document;
use serde_json::Map;
use sqlx::Row;
use url::Url;

use crate::schema::{
    Action, AuthStrategy, DatabaseKind, Field, FieldType, NoSqlOperation, ParameterLocation,
    Provenance, Resource, Safety, Schema, SqlOperation, SyncSource, Transport, Verb,
};

use super::SyncPlugin;

pub struct DbSync {
    connection_string: String,
}

impl DbSync {
    pub fn new(connection_string: String) -> Self {
        Self { connection_string }
    }
}

#[async_trait::async_trait]
impl SyncPlugin for DbSync {
    async fn introspect(&self) -> Result<Schema> {
        let (db_kind, resources) = if self.connection_string.starts_with("postgres://")
            || self.connection_string.starts_with("postgresql://")
        {
            (
                DatabaseKind::Postgres,
                introspect_postgres(&self.connection_string).await?,
            )
        } else if self.connection_string.starts_with("mysql://") {
            (
                DatabaseKind::Mysql,
                introspect_mysql(&self.connection_string).await?,
            )
        } else if self.connection_string.starts_with("sqlite:") {
            (
                DatabaseKind::Sqlite,
                introspect_sqlite(&self.connection_string).await?,
            )
        } else if self.connection_string.starts_with("mongodb://")
            || self.connection_string.starts_with("mongodb+srv://")
        {
            (
                DatabaseKind::Mongodb,
                introspect_mongodb(&self.connection_string).await?,
            )
        } else if self.connection_string.starts_with("redis://")
            || self.connection_string.starts_with("rediss://")
        {
            (
                DatabaseKind::Redis,
                introspect_redis(&self.connection_string).await?,
            )
        } else if self.connection_string.starts_with("firestore://") {
            (
                DatabaseKind::Firestore,
                introspect_firestore(&self.connection_string).await?,
            )
        } else if self.connection_string.starts_with("dynamodb://") {
            (
                DatabaseKind::Dynamodb,
                introspect_dynamodb(&self.connection_string).await?,
            )
        } else {
            bail!(
                "unsupported database connection string; expected postgres://, mysql://, sqlite:, mongodb://, redis://, firestore://, or dynamodb://"
            );
        };

        Ok(Schema {
            source: SyncSource::Db,
            base_url: None,
            auth: AuthStrategy::None,
            resources,
            metadata: {
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
            },
        })
    }
}

async fn introspect_postgres(connection_string: &str) -> Result<Vec<Resource>> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(connection_string)
        .await
        .context("failed to connect to postgres")?;

    let tables = sqlx::query(
        "select table_name from information_schema.tables where table_schema = 'public' and table_type='BASE TABLE' order by table_name",
    )
    .fetch_all(&pool)
    .await?;

    let mut resources = Vec::new();
    for row in tables {
        let table: String = row.try_get("table_name")?;
        let columns = sqlx::query(
            "select c.column_name, c.data_type, c.is_nullable, tc.constraint_type
             from information_schema.columns c
             left join information_schema.key_column_usage kcu
               on c.table_name = kcu.table_name and c.column_name = kcu.column_name and c.table_schema = kcu.table_schema
             left join information_schema.table_constraints tc
               on kcu.constraint_name = tc.constraint_name and kcu.table_schema = tc.table_schema
             where c.table_schema = 'public' and c.table_name = $1
             order by c.ordinal_position",
        )
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
            &table,
            &columns,
            DatabaseKind::Postgres,
        ));
    }
    Ok(resources)
}

async fn introspect_mysql(connection_string: &str) -> Result<Vec<Resource>> {
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

        resources.push(resource_from_table(&table, &columns, DatabaseKind::Mysql));
    }
    Ok(resources)
}

async fn introspect_sqlite(connection_string: &str) -> Result<Vec<Resource>> {
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
        let pragma = format!("pragma table_info('{table}')");
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

        resources.push(resource_from_table(&table, &columns, DatabaseKind::Sqlite));
    }
    Ok(resources)
}

struct ColumnInfo {
    column_name: String,
    data_type: String,
    is_nullable: String,
    constraint_type: Option<String>,
}

fn resource_from_table(table: &str, rows: &[ColumnInfo], db_kind: DatabaseKind) -> Resource {
    let mut fields = Vec::new();
    let mut primary_key = None::<String>;

    for row in rows {
        if row.constraint_type.as_deref() == Some("PRIMARY KEY") && primary_key.is_none() {
            primary_key = Some(row.column_name.clone());
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

    let pk = primary_key.unwrap_or_else(|| "id".to_string());
    let pk_field = Field {
        name: pk.clone(),
        description: Some("Primary key".to_string()),
        field_type: FieldType::Integer,
        required: true,
        location: Some(ParameterLocation::Path),
        default: None,
        enum_values: Vec::new(),
    };

    let resource_name = table.trim_end_matches('s').to_string();
    Resource {
        name: resource_name.clone(),
        description: Some(format!("Table {}", table)),
        fields: fields.clone(),
        actions: vec![
            Action {
                name: format!("list_{}s", resource_name),
                description: Some(format!("List rows from {}", table)),
                verb: Verb::List,
                transport: Transport::Sql {
                    database_kind: db_kind.clone(),
                    table: table.to_string(),
                    operation: SqlOperation::Select,
                    primary_key: Some(pk.clone()),
                },
                parameters: Vec::new(),
                safety: Safety::ReadOnly,
                resource: Some(resource_name.clone()),
                provenance: Provenance::Declared,
                metadata: Map::new(),
            },
            Action {
                name: format!("get_{}", resource_name),
                description: Some(format!("Fetch one row from {}", table)),
                verb: Verb::Get,
                transport: Transport::Sql {
                    database_kind: db_kind.clone(),
                    table: table.to_string(),
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
                name: format!("create_{}", resource_name),
                description: Some(format!("Insert one row into {}", table)),
                verb: Verb::Create,
                transport: Transport::Sql {
                    database_kind: db_kind.clone(),
                    table: table.to_string(),
                    operation: SqlOperation::Insert,
                    primary_key: Some(pk.clone()),
                },
                parameters: fields
                    .iter()
                    .filter(|field| field.name != pk)
                    .cloned()
                    .collect(),
                safety: Safety::Mutating,
                resource: Some(resource_name.clone()),
                provenance: Provenance::Declared,
                metadata: Map::new(),
            },
            Action {
                name: format!("update_{}", resource_name),
                description: Some(format!("Update one row in {}", table)),
                verb: Verb::Update,
                transport: Transport::Sql {
                    database_kind: db_kind.clone(),
                    table: table.to_string(),
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
                description: Some(format!("Delete one row from {}", table)),
                verb: Verb::Delete,
                transport: Transport::Sql {
                    database_kind: db_kind,
                    table: table.to_string(),
                    operation: SqlOperation::DeleteByPk,
                    primary_key: Some(pk),
                },
                parameters: vec![pk_field],
                safety: Safety::Destructive,
                resource: Some(resource_name),
                provenance: Provenance::Declared,
                metadata: Map::new(),
            },
        ],
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
    let resource_name = collection.trim_end_matches('s').replace('-', "_");
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
                name: format!("list_{}s", resource_name),
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
        assert_eq!(resource.name, "product");
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
                .any(|action| action.name == "get_product")
        );
        assert!(
            resource
                .actions
                .iter()
                .any(|action| action.name == "delete_product")
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
        assert_eq!(resource.name, "redis_key");
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
                .any(|action| action.name == "get_redis_key")
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
