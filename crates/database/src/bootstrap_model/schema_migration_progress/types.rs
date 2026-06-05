use serde::{
    Deserialize,
    Serialize,
};
use value::{
    codegen_convex_serialization,
    DeveloperDocumentId,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaMigrationProgressMetadata {
    pub schema_id: DeveloperDocumentId,
    pub migration_id: String,
    pub num_docs_processed: u64,
    pub total_docs: Option<u64>,
    pub complete: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SerializedSchemaMigrationProgressMetadata {
    pub schema_id: String,
    pub migration_id: String,
    pub num_docs_processed: i64,
    pub total_docs: Option<i64>,
    pub complete: bool,
}

impl From<SchemaMigrationProgressMetadata> for SerializedSchemaMigrationProgressMetadata {
    fn from(metadata: SchemaMigrationProgressMetadata) -> Self {
        SerializedSchemaMigrationProgressMetadata {
            schema_id: metadata.schema_id.to_string(),
            migration_id: metadata.migration_id,
            num_docs_processed: metadata.num_docs_processed as i64,
            total_docs: metadata.total_docs.map(|x| x as i64),
            complete: metadata.complete,
        }
    }
}

impl TryFrom<SerializedSchemaMigrationProgressMetadata> for SchemaMigrationProgressMetadata {
    type Error = anyhow::Error;

    fn try_from(serialized: SerializedSchemaMigrationProgressMetadata) -> anyhow::Result<Self> {
        Ok(SchemaMigrationProgressMetadata {
            schema_id: serialized.schema_id.parse()?,
            migration_id: serialized.migration_id,
            num_docs_processed: serialized.num_docs_processed as u64,
            total_docs: serialized.total_docs.map(|x| x as u64),
            complete: serialized.complete,
        })
    }
}

codegen_convex_serialization!(
    SchemaMigrationProgressMetadata,
    SerializedSchemaMigrationProgressMetadata
);
