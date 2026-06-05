pub mod types;

use std::sync::{
    Arc,
    LazyLock,
};

use common::{
    document::{
        ParsedDocument,
        CREATION_TIME_FIELD_PATH,
    },
    runtime::Runtime,
};
use value::{
    DeveloperDocumentId,
    FieldPath,
    ResolvedDocumentId,
    TableName,
    TableNamespace,
};

use crate::{
    system_tables::{
        SystemIndex,
        SystemTable,
    },
    SchemaMigrationProgressMetadata,
    SystemMetadataModel,
    Transaction,
};

pub static SCHEMA_MIGRATION_PROGRESS_TABLE: TableName =
    TableName::const_new("_schema_migration_progress");

pub static SCHEMA_MIGRATION_PROGRESS_BY_SCHEMA_ID: LazyLock<
    SystemIndex<SchemaMigrationProgressTable>,
> = LazyLock::new(|| {
    SystemIndex::new(
        "by_schema_id",
        [&SCHEMA_ID_FIELD, &MIGRATION_ID_FIELD, &CREATION_TIME_FIELD_PATH],
    )
    .unwrap()
});

static SCHEMA_ID_FIELD: LazyLock<FieldPath> =
    LazyLock::new(|| "schemaId".parse().expect("invalid schemaId field"));
static MIGRATION_ID_FIELD: LazyLock<FieldPath> =
    LazyLock::new(|| "migrationId".parse().expect("invalid migrationId field"));

pub struct SchemaMigrationProgressTable;

impl SystemTable for SchemaMigrationProgressTable {
    type Metadata = types::SchemaMigrationProgressMetadata;

    fn table_name() -> &'static TableName {
        &SCHEMA_MIGRATION_PROGRESS_TABLE
    }

    fn indexes() -> Vec<SystemIndex<Self>> {
        vec![SCHEMA_MIGRATION_PROGRESS_BY_SCHEMA_ID.clone()]
    }
}

pub struct SchemaMigrationProgressModel<'a, RT: Runtime> {
    tx: &'a mut Transaction<RT>,
    namespace: TableNamespace,
}

impl<'a, RT: Runtime> SchemaMigrationProgressModel<'a, RT> {
    pub fn new(tx: &'a mut Transaction<RT>, namespace: TableNamespace) -> Self {
        Self { tx, namespace }
    }

    pub async fn progress_for_schema(
        &mut self,
        schema_id: ResolvedDocumentId,
    ) -> anyhow::Result<Vec<Arc<ParsedDocument<SchemaMigrationProgressMetadata>>>> {
        self.tx
            .query_system(self.namespace, &*SCHEMA_MIGRATION_PROGRESS_BY_SCHEMA_ID)?
            .eq(&[schema_id.developer_id.encode_into(&mut Default::default())])?
            .all()
            .await
    }

    pub async fn initialize_progress(
        &mut self,
        schema_id: ResolvedDocumentId,
        migration_id: String,
        total_docs: Option<u64>,
    ) -> anyhow::Result<()> {
        let mut system_model = SystemMetadataModel::new(self.tx, self.namespace);
        let metadata = SchemaMigrationProgressMetadata {
            schema_id: schema_id.developer_id,
            migration_id,
            num_docs_processed: 0,
            total_docs,
            complete: false,
        };
        system_model
            .insert(
                &SCHEMA_MIGRATION_PROGRESS_TABLE,
                metadata.try_into()?,
            )
            .await?;
        Ok(())
    }

    pub async fn update_progress(
        &mut self,
        progress_id: ResolvedDocumentId,
        num_docs_processed: u64,
        complete: bool,
    ) -> anyhow::Result<()> {
        let existing = self
            .tx
            .get(progress_id)
            .await?
            .context("Missing schema migration progress")?;
        let metadata: SchemaMigrationProgressMetadata = existing.into_value().0.try_into()?;
        let mut system_model = SystemMetadataModel::new(self.tx, self.namespace);
        let new_metadata = SchemaMigrationProgressMetadata {
            num_docs_processed,
            complete,
            ..metadata
        };
        system_model
            .replace(progress_id, new_metadata.try_into()?)
            .await?;
        Ok(())
    }

    pub async fn delete_progress_for_schema(
        &mut self,
        schema_id: ResolvedDocumentId,
    ) -> anyhow::Result<()> {
        let progress_docs = self.progress_for_schema(schema_id).await?;
        let mut system_model = SystemMetadataModel::new_global(self.tx);
        for doc in progress_docs {
            system_model.delete(doc.id()).await?;
        }
        Ok(())
    }
}

use anyhow::Context;
