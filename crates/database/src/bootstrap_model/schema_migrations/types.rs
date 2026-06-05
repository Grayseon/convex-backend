use serde::{
    Deserialize,
    Serialize,
};
use value::{
    codegen_convex_serialization,
    TableName,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchemaMigrationState {
    Completed,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaMigrationRecordMetadata {
    pub migration_id: String,
    pub component_path: String,
    pub table_name: TableName,
    pub field_path: Option<String>,
    pub handler_hash: String,
    pub state: SchemaMigrationState,
    pub completed_at: i64,
    pub docs_processed: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SerializedSchemaMigrationRecordMetadata {
    pub migration_id: String,
    pub component_path: String,
    pub table_name: String,
    pub field_path: Option<String>,
    pub handler_hash: String,
    pub state: String,
    pub completed_at: i64,
    pub docs_processed: i64,
}

impl From<SchemaMigrationRecordMetadata> for SerializedSchemaMigrationRecordMetadata {
    fn from(metadata: SchemaMigrationRecordMetadata) -> Self {
        SerializedSchemaMigrationRecordMetadata {
            migration_id: metadata.migration_id,
            component_path: metadata.component_path,
            table_name: metadata.table_name.to_string(),
            field_path: metadata.field_path,
            handler_hash: metadata.handler_hash,
            state: match metadata.state {
                SchemaMigrationState::Completed => "completed".to_string(),
                SchemaMigrationState::Failed => "failed".to_string(),
            },
            completed_at: metadata.completed_at,
            docs_processed: metadata.docs_processed as i64,
        }
    }
}

impl TryFrom<SerializedSchemaMigrationRecordMetadata> for SchemaMigrationRecordMetadata {
    type Error = anyhow::Error;

    fn try_from(serialized: SerializedSchemaMigrationRecordMetadata) -> anyhow::Result<Self> {
        let state = match serialized.state.as_str() {
            "completed" => SchemaMigrationState::Completed,
            "failed" => SchemaMigrationState::Failed,
            other => anyhow::bail!("Invalid schema migration state \"{other}\""),
        };
        Ok(SchemaMigrationRecordMetadata {
            migration_id: serialized.migration_id,
            component_path: serialized.component_path,
            table_name: serialized.table_name.parse()?,
            field_path: serialized.field_path,
            handler_hash: serialized.handler_hash,
            state,
            completed_at: serialized.completed_at,
            docs_processed: serialized.docs_processed as u64,
        })
    }
}

codegen_convex_serialization!(
    SchemaMigrationRecordMetadata,
    SerializedSchemaMigrationRecordMetadata
);
