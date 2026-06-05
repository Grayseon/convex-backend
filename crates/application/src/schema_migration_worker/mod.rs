use std::{
    collections::BTreeMap,
    sync::Arc,
    time::Duration,
};

use anyhow::Context;
use common::{
    backoff::Backoff,
    bootstrap_model::schema::SchemaState,
    errors::report_error,
    persistence::LatestDocument,
    runtime::Runtime,
    schemas::{
        DatabaseSchema,
        SchemaMigration,
        SchemaMigrationScope,
    },
    types::FieldName,
    virtual_system_mapping::VirtualSystemMapping,
};
use database::{
    Database,
    IndexModel,
    PatchValue,
    SchemaMigrationRecordMetadata,
    SchemaMigrationState,
    SchemaMigrationsModel,
    SchemaModel,
    Token,
    Transaction,
    UserFacingModel,
    SCHEMAS_TABLE,
};
use errors::{
    ErrorMetadata,
    ErrorMetadataAnyhowExt,
};
use futures::{
    pin_mut,
    Future,
    TryStreamExt,
};
use keybroker::Identity;
use rand::Rng;
use serde_json::{
    json,
    Value as JsonValue,
};
use value::{
    ConvexObject,
    ConvexValue,
    FieldPath,
    NamespacedTableMapping,
    ResolvedDocumentId,
    TableNamespace,
    TabletId,
};

use crate::{
    application_function_runner::ApplicationFunctionRunner,
    metrics::log_worker_starting,
};

const INITIAL_BACKOFF: Duration = Duration::from_millis(10);
const MAX_BACKOFF: Duration = Duration::from_secs(5);

pub struct SchemaMigrationWorker<RT: Runtime> {
    runtime: RT,
    database: Database<RT>,
    runner: Arc<ApplicationFunctionRunner<RT>>,
}

struct PendingSchemaMigration {
    namespace: TableNamespace,
    id: ResolvedDocumentId,
    table_mapping: NamespacedTableMapping,
    db_schema: Arc<DatabaseSchema>,
    ts: common::types::RepeatableTimestamp,
    component_path: String,
    by_id_indexes: BTreeMap<TabletId, common::types::IndexId>,
}

impl<RT: Runtime> SchemaMigrationWorker<RT> {
    pub fn start(
        runtime: RT,
        database: Database<RT>,
        runner: Arc<ApplicationFunctionRunner<RT>>,
    ) -> impl Future<Output = ()> + Send {
        let worker = Self {
            runtime,
            database,
            runner,
        };
        async move {
            tracing::info!("Starting SchemaMigrationWorker");
            let mut backoff = Backoff::new(INITIAL_BACKOFF, MAX_BACKOFF);
            loop {
                let result: anyhow::Result<()> = async {
                    let token = Box::pin(worker.run()).await?;
                    worker
                        .database
                        .subscribe_and_wait_for_invalidation(token)
                        .await?;
                    Ok(())
                }
                .await;
                if let Err(e) = result {
                    let delay = backoff.fail(&mut worker.runtime.rng());
                    report_error(&mut e.context("SchemaMigrationWorker died")).await;
                    tracing::error!("Migration worker failed, sleeping {delay:?}");
                    worker.runtime.wait(delay).await;
                } else {
                    backoff.reset();
                }
            }
        }
    }

    pub async fn run(&self) -> anyhow::Result<Token> {
        let status = log_worker_starting("SchemaMigrationWorker");
        self.run_pending_migrations().await;

        drop(status);
        tracing::debug!("SchemaMigrationWorker waiting...");
        let mut tx: Transaction<RT> = self.database.begin(Identity::system()).await?;
        Ok(tx.into_token()?)
    }

    /// Run any outstanding schema migrations. Called by the background worker
    /// and proactively from `wait_for_schema` so migrations make progress
    /// during pushes even if the worker misses an invalidation.
    pub async fn run_pending_migrations_from(
        database: Database<RT>,
        runner: Arc<ApplicationFunctionRunner<RT>>,
        runtime: RT,
    ) {
        let worker = Self {
            runtime,
            database,
            runner,
        };
        worker.run_pending_migrations().await;
    }

    async fn run_pending_migrations(&self) {
        let Ok(mut tx) = self.database.begin(Identity::system()).await else {
            return;
        };
        let Ok(pending_work) = Self::pending_schema_migrations(&mut tx).await else {
            return;
        };
        drop(tx);

        for pending in pending_work {
            if let Err(e) = self.run_migrations_for_schema(pending).await {
                report_error(&mut e.context("Failed to run schema migrations")).await;
            }
        }
    }

    async fn pending_schema_migrations(
        tx: &mut Transaction<RT>,
    ) -> anyhow::Result<Vec<PendingSchemaMigration>> {
        let mut pending_work = Vec::new();
        let namespaces: Vec<_> = tx.table_mapping().namespaces_for_name(&SCHEMAS_TABLE);
        for namespace in namespaces {
            if let Some((id, db_schema)) = SchemaModel::new(tx, namespace)
                .get_by_state(SchemaState::Pending)
                .await?
            {
                let component_path = tx
                    .get_component_path(namespace.into())
                    .map(|p| p.to_string())
                    .unwrap_or_default();
                let table_mapping = tx.table_mapping().namespace(namespace);
                let ts = tx.begin_timestamp();
                let by_id_indexes = IndexModel::new(tx).by_id_indexes().await?;
                pending_work.push(PendingSchemaMigration {
                    namespace,
                    id,
                    table_mapping,
                    db_schema,
                    ts,
                    component_path,
                    by_id_indexes,
                });
            }
        }
        Ok(pending_work)
    }

    async fn run_migrations_for_schema(
        &self,
        pending: PendingSchemaMigration,
    ) -> anyhow::Result<()> {
        let pending_migrations =
            Self::pending_migrations_for_schema(&self.database, &pending).await?;
        if pending_migrations.is_empty() {
            return Ok(());
        }

        tracing::info!(
            "SchemaMigrationWorker running {} migrations for schema {:?}",
            pending_migrations.len(),
            pending.id
        );

        for migration in pending_migrations {
            if let Err(e) = self.run_single_migration(&pending, migration).await {
                report_error(&mut e.context("Failed to run schema migration")).await;
            }
        }
        Ok(())
    }

    async fn pending_migrations_for_schema(
        database: &Database<RT>,
        pending: &PendingSchemaMigration,
    ) -> anyhow::Result<Vec<SchemaMigration>> {
        let mut tx = database.begin(Identity::system()).await?;
        let mut model = SchemaMigrationsModel::new(&mut tx, pending.namespace);
        let mut result = vec![];
        for migration in &pending.db_schema.migrations {
            if let Some(existing) = model
                .get_completed_migration(&migration.id, &pending.component_path)
                .await?
            {
                if existing.handler_hash != migration.handler_hash {
                    anyhow::bail!(ErrorMetadata::bad_request(
                        "MigrationHandlerChanged",
                        format!(
                            "Migration \"{}\" has already run with different code. Use a new \
                             migration id.",
                            migration.id
                        )
                    ));
                }
                if matches!(existing.state, SchemaMigrationState::Completed) {
                    continue;
                }
            }
            result.push(migration.clone());
        }
        Ok(result)
    }

    async fn run_single_migration(
        &self,
        pending: &PendingSchemaMigration,
        migration: SchemaMigration,
    ) -> anyhow::Result<()> {
        let tablet_id = pending
            .table_mapping
            .name_to_tablet()(migration.table_name.clone())?;
        let index_id = *pending
            .by_id_indexes
            .get(&tablet_id)
            .context("Missing by_id index")?;

        let mut docs_processed = 0u64;
        let mut table_iterator = self
            .database
            .table_iterator(pending.ts, 1000)
            .multi(vec![tablet_id]);
        {
            let stream = table_iterator.stream_documents_in_table(tablet_id, index_id, None);
            pin_mut!(stream);

            while let Some(LatestDocument { value: doc, .. }) = stream.try_next().await? {
                let id = doc.id();
                let doc_object: ConvexObject = doc.into_value().0;
                let patch = self
                    .run_migration_on_document(pending, &migration, &doc_object)
                    .await?;
                if let Some(patch) = patch {
                    let mut tx = self.database.begin(Identity::system()).await?;
                    let mut user_model = UserFacingModel::new(&mut tx, pending.namespace);
                    user_model
                        .patch_for_schema_migration(id.developer_id, patch)
                        .await?;
                    self.database
                        .commit_with_write_source(tx, "schema_migration_patch")
                        .await?;
                }
                docs_processed += 1;
            }
        }
        table_iterator.unregister_table(tablet_id)?;

        let mut tx = self.database.begin(Identity::system()).await?;
        let mut model = SchemaMigrationsModel::new(&mut tx, pending.namespace);
        model
            .insert_completed_migration(SchemaMigrationRecordMetadata {
                migration_id: migration.id.clone(),
                component_path: pending.component_path.clone(),
                table_name: migration.table_name.clone(),
                field_path: migration.field_path.clone(),
                handler_hash: migration.handler_hash.clone(),
                state: SchemaMigrationState::Completed,
                completed_at: self.runtime.unix_timestamp().as_secs() as i64,
                docs_processed,
            })
            .await?;
        self.database
            .commit_with_write_source(tx, "schema_migration_completed")
            .await?;
        tracing::info!(
            "Completed schema migration \"{}\" on {}.{} ({} documents)",
            migration.id,
            migration.table_name,
            migration
                .field_path
                .as_deref()
                .unwrap_or("<table>"),
            docs_processed,
        );
        Ok(())
    }

    async fn run_migration_on_document(
        &self,
        pending: &PendingSchemaMigration,
        migration: &SchemaMigration,
        doc: &ConvexObject,
    ) -> anyhow::Result<Option<PatchValue>> {
        let doc_json = convex_object_to_json(doc)?;
        let ctx = match migration.scope {
            SchemaMigrationScope::Field => {
                let field_path = migration
                    .field_path
                    .as_ref()
                    .context("Field migration missing field path")?;
                let (old_value, is_missing) = get_field_value(doc, field_path)?;
                json!({
                    "oldValue": old_value,
                    "doc": doc_json,
                    "fieldPath": field_path,
                    "tableName": migration.table_name.to_string(),
                    "isMissing": is_missing,
                })
            },
            SchemaMigrationScope::Table => json!({
                "doc": doc_json,
                "tableName": migration.table_name.to_string(),
            }),
        };

        let rng_seed = self.runtime.rng().random();
        let result = self
            .runner
            .run_migration_handler(
                vec![migration.handler_source.clone()],
                0,
                ctx,
                rng_seed,
                self.runtime.unix_timestamp(),
            )
            .await?;

        let Some(result) = result else {
            return Ok(None);
        };

        let patch_json = match migration.scope {
            SchemaMigrationScope::Field => {
                let field_path = migration.field_path.as_ref().unwrap();
                field_patch_json(field_path, result)
            },
            SchemaMigrationScope::Table => result,
        };
        Ok(Some(patch_json.try_into()?))
    }

    pub async fn migrations_complete_for_schema(
        database: &Database<RT>,
        namespace: TableNamespace,
        component_path: &str,
        db_schema: &DatabaseSchema,
    ) -> anyhow::Result<bool> {
        if db_schema.migrations.is_empty() {
            return Ok(true);
        }
        let mut tx = database.begin(Identity::system()).await?;
        let mut model = SchemaMigrationsModel::new(&mut tx, namespace);
        for migration in &db_schema.migrations {
            let Some(existing) = model
                .get_completed_migration(&migration.id, component_path)
                .await?
            else {
                return Ok(false);
            };
            if !matches!(existing.state, SchemaMigrationState::Completed) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

fn field_patch_json(field_path: &str, value: JsonValue) -> JsonValue {
    let parts: Vec<_> = field_path.split('.').collect();
    let mut current = value;
    for part in parts.into_iter().rev() {
        current = json!({ part: current });
    }
    current
}

fn convex_object_to_json(doc: &ConvexObject) -> anyhow::Result<JsonValue> {
    let mut map = serde_json::Map::new();
    for (field, value) in doc.clone().into_iter() {
        map.insert(field.to_string(), convex_value_to_json(&value)?);
    }
    Ok(JsonValue::Object(map))
}

fn convex_value_to_json(value: &ConvexValue) -> anyhow::Result<JsonValue> {
    Ok(serde_json::from_str(&value.json_serialize()?)?)
}

fn get_field_value(doc: &ConvexObject, field_path: &str) -> anyhow::Result<(JsonValue, bool)> {
    let path: FieldPath = field_path.parse()?;
    let mut current = doc.clone();
    let segments = path.fields();
    for (i, segment) in segments.iter().enumerate() {
        let field_name: FieldName = segment.clone().try_into()?;
        let Some(value) = current.get(&field_name) else {
            return Ok((JsonValue::Null, true));
        };
        if i == segments.len() - 1 {
            return Ok((convex_value_to_json(value)?, false));
        }
        current = match value {
            ConvexValue::Object(obj) => obj.clone(),
            _ => return Ok((JsonValue::Null, true)),
        };
    }
    Ok((JsonValue::Null, true))
}
