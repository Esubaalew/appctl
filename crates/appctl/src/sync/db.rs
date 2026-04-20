use anyhow::{Context, Result, bail};
use serde_json::Map;
use sqlx::Row;

use crate::schema::{
    Action, AuthStrategy, DatabaseKind, Field, FieldType, ParameterLocation, Resource, Safety,
    Schema, SqlOperation, SyncSource, Transport, Verb,
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
        } else {
            bail!("unsupported database connection string; expected postgres:// or mysql://");
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
                metadata: Map::new(),
            },
        ],
    }
}

fn sql_type_to_field_type(data_type: &str) -> FieldType {
    match data_type {
        "integer" | "bigint" | "smallint" | "serial" | "bigserial" | "int" => FieldType::Integer,
        "numeric" | "decimal" | "float" | "double" | "real" => FieldType::Number,
        "bool" | "boolean" => FieldType::Boolean,
        "timestamp" | "timestamp without time zone" | "timestamp with time zone" | "datetime" => {
            FieldType::DateTime
        }
        "date" => FieldType::Date,
        "uuid" => FieldType::Uuid,
        "json" | "jsonb" => FieldType::Json,
        _ => FieldType::String,
    }
}
