pub mod types;

use std::sync::{
    Arc,
    LazyLock,
};

use common::{
    document::CREATION_TIME_FIELD_PATH,
    runtime::Runtime,
};
use value::{
    FieldPath,
    TableName,
    TableNamespace,
};

use crate::{
    system_tables::{
        SystemIndex,
        SystemTable,
    },
    SchemaMigrationRecordMetadata,
    SystemMetadataModel,
    Transaction,
};

pub static SCHEMA_MIGRATIONS_TABLE: TableName = TableName::const_new("_schema_migrations");

pub static SCHEMA_MIGRATIONS_BY_MIGRATION_ID: LazyLock<SystemIndex<SchemaMigrationsTable>> =
    LazyLock::new(|| {
        SystemIndex::new(
            "by_migration_id",
            [
                &MIGRATION_ID_FIELD,
                &COMPONENT_PATH_FIELD,
                &CREATION_TIME_FIELD_PATH,
            ],
        )
        .unwrap()
    });

static MIGRATION_ID_FIELD: LazyLock<FieldPath> =
    LazyLock::new(|| "migrationId".parse().expect("invalid migrationId field"));
static COMPONENT_PATH_FIELD: LazyLock<FieldPath> =
    LazyLock::new(|| "componentPath".parse().expect("invalid componentPath field"));

pub struct SchemaMigrationsTable;

impl SystemTable for SchemaMigrationsTable {
    type Metadata = types::SchemaMigrationRecordMetadata;

    fn table_name() -> &'static TableName {
        &SCHEMA_MIGRATIONS_TABLE
    }

    fn indexes() -> Vec<SystemIndex<Self>> {
        vec![SCHEMA_MIGRATIONS_BY_MIGRATION_ID.clone()]
    }
}

pub struct SchemaMigrationsModel<'a, RT: Runtime> {
    tx: &'a mut Transaction<RT>,
    namespace: TableNamespace,
}

impl<'a, RT: Runtime> SchemaMigrationsModel<'a, RT> {
    pub fn new(tx: &'a mut Transaction<RT>, namespace: TableNamespace) -> Self {
        Self { tx, namespace }
    }

    pub async fn get_completed_migration(
        &mut self,
        migration_id: &str,
        component_path: &str,
    ) -> anyhow::Result<Option<Arc<common::document::ParsedDocument<SchemaMigrationRecordMetadata>>>>
    {
        self.tx
            .query_system(self.namespace, &*SCHEMA_MIGRATIONS_BY_MIGRATION_ID)?
            .eq(&[migration_id, component_path])?
            .unique()
            .await
    }

    pub async fn insert_completed_migration(
        &mut self,
        metadata: SchemaMigrationRecordMetadata,
    ) -> anyhow::Result<()> {
        let mut system_model = SystemMetadataModel::new(self.tx, self.namespace);
        system_model
            .insert(&SCHEMA_MIGRATIONS_TABLE, metadata.try_into()?)
            .await?;
        Ok(())
    }
}
