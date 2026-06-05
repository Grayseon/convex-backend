use std::{
    sync::Arc,
    time::Duration,
};

use anyhow::anyhow;
use common::runtime::{
    Runtime,
    UnixTimestamp,
};
use deno_core::{
    v8::{
        self,
        scope,
        scope_with_context,
    },
    ModuleSpecifier,
};
use errors::{
    ErrorMetadata,
    ErrorMetadataAnyhowExt,
};
use model::modules::module_versions::{
    FullModuleSource,
    ModuleSource,
    SourceMap,
};
use rand::SeedableRng;
use rand_chacha::ChaCha12Rng;
use serde_json::Value as JsonValue;

use super::{
    AsyncOpRequest,
    IsolateEnvironment,
    ModuleCodeCacheResult,
};
use crate::{
    environment::helpers::syscall_error::{
        syscall_description_for_error,
        syscall_name_for_error,
    },
    helpers::{
        self,
        to_rust_string,
    },
    isolate::{
        Isolate,
        CONVEX_SCHEME,
    },
    request_scope::RequestScope,
    strings,
    timeout::Timeout,
};

pub struct MigrationEnvironment {
    migration_bundle: ModuleSource,
    source_map: Option<SourceMap>,
    rng: ChaCha12Rng,
    unix_timestamp: UnixTimestamp,
}

impl<RT: Runtime> IsolateEnvironment<RT> for MigrationEnvironment {
    fn trace(
        &mut self,
        _level: common::log_lines::LogLevel,
        messages: Vec<String>,
    ) -> anyhow::Result<()> {
        tracing::warn!(
            "Unexpected Console access at migration evaluation time: {}",
            messages.join(" ")
        );
        Ok(())
    }

    fn rng(&mut self) -> anyhow::Result<&mut ChaCha12Rng> {
        Ok(&mut self.rng)
    }

    fn crypto_rng(&mut self) -> anyhow::Result<super::crypto_rng::CryptoRng> {
        anyhow::bail!(ErrorMetadata::bad_request(
            "NoCryptoRngInMigration",
            "Cannot use cryptographic randomness when running migrations"
        ))
    }

    fn unix_timestamp(&mut self) -> anyhow::Result<UnixTimestamp> {
        Ok(self.unix_timestamp)
    }

    fn performance_now(&mut self) -> anyhow::Result<Duration> {
        anyhow::bail!(ErrorMetadata::bad_request(
            "NoPerformanceInMigration",
            "The Performance API is not supported when running migrations"
        ))
    }

    fn performance_time_origin(&mut self) -> anyhow::Result<UnixTimestamp> {
        anyhow::bail!(ErrorMetadata::bad_request(
            "NoPerformanceInMigration",
            "The Performance API is not supported when running migrations"
        ))
    }

    fn get_environment_variable(
        &mut self,
        _name: model::environment_variables::types::EnvVarName,
    ) -> anyhow::Result<Option<model::environment_variables::types::EnvVarValue>> {
        anyhow::bail!(ErrorMetadata::bad_request(
            "NoEnvironmentVariablesInMigration",
            "Environment variables unsupported when running migrations"
        ))
    }

    fn get_all_table_mappings(&mut self) -> anyhow::Result<value::NamespacedTableMapping> {
        anyhow::bail!(ErrorMetadata::bad_request(
            "NoTableMappingFetchInMigration",
            "Getting the table mapping unsupported when running migrations"
        ))
    }

    async fn lookup_source(
        &mut self,
        path: &str,
        _timeout: &mut Timeout<RT>,
    ) -> anyhow::Result<Option<(Arc<FullModuleSource>, ModuleCodeCacheResult)>> {
        if path != "migration.js" {
            anyhow::bail!(ErrorMetadata::bad_request(
                "NoImportModuleInMigration",
                format!("Can't import {path} while running migrations")
            ))
        }
        Ok(Some((
            Arc::new(FullModuleSource {
                source: self.migration_bundle.clone(),
                source_map: self.source_map.clone(),
            }),
            ModuleCodeCacheResult::noop(),
        )))
    }

    fn syscall(&mut self, name: &str, _args: JsonValue) -> anyhow::Result<JsonValue> {
        anyhow::bail!(ErrorMetadata::bad_request(
            "NoSyscallInMigration",
            format!("Syscall {name} unsupported when running migrations")
        ))
    }

    fn start_async_syscall(
        &mut self,
        name: String,
        _args: JsonValue,
        _resolver: v8::Global<v8::PromiseResolver>,
    ) -> anyhow::Result<()> {
        anyhow::bail!(ErrorMetadata::bad_request(
            format!("No{}InMigration", syscall_name_for_error(&name)),
            format!(
                "{} unsupported while running migrations",
                syscall_description_for_error(&name),
            ),
        ))
    }

    fn start_async_op(
        &mut self,
        request: AsyncOpRequest,
        _resolver: v8::Global<v8::PromiseResolver>,
    ) -> anyhow::Result<()> {
        anyhow::bail!(ErrorMetadata::bad_request(
            format!("No{}InMigration", request.name_for_error()),
            format!(
                "{} unsupported while running migrations",
                request.description_for_error()
            ),
        ))
    }

    fn user_timeout(&self) -> std::time::Duration {
        *common::knobs::DATABASE_UDF_USER_TIMEOUT
    }

    fn system_timeout(&self) -> std::time::Duration {
        *common::knobs::DATABASE_UDF_SYSTEM_TIMEOUT
    }
}

impl MigrationEnvironment {
    pub fn build_migration_module(handlers: &[String]) -> String {
        let handler_entries = handlers
            .iter()
            .map(|handler| format!("({handler})"))
            .collect::<Vec<_>>()
            .join(",\n");
        format!(
            r#"const handlers = [
{handler_entries}
];
export function runMigration(handlerIndex, ctxJson) {{
  const ctx = JSON.parse(ctxJson);
  const handler = handlers[handlerIndex];
  if (handler === undefined) {{
    throw new Error("Missing migration handler at index " + handlerIndex);
  }}
  const result = handler(ctx);
  if (result === undefined) {{
    return null;
  }}
  return JSON.stringify(result);
}}
"#
        )
    }

    pub async fn run_migration_handler<RT: Runtime>(
        client_id: String,
        isolate: &mut Isolate<RT>,
        v8_context: v8::Global<v8::Context>,
        handlers: &[String],
        handler_index: usize,
        ctx: JsonValue,
        rng_seed: [u8; 32],
        unix_timestamp: UnixTimestamp,
    ) -> anyhow::Result<Option<JsonValue>> {
        let migration_source = Self::build_migration_module(handlers);
        let migration_bundle = ModuleSource::new(&migration_source);
        let rng = ChaCha12Rng::from_seed(rng_seed);
        let environment = Self {
            migration_bundle,
            source_map: None,
            rng,
            unix_timestamp,
        };
        let client_id = Arc::new(client_id);
        let (handle, state, mut timeout) = isolate.start_request(client_id, environment).await?;
        scope_with_context!(let context_scope, isolate.isolate(), v8_context);
        let mut isolate_context =
            RequestScope::new(context_scope, handle.clone(), state, false).await?;
        let handle = isolate_context.handle();
        let result = Self::run_handler(
            &mut isolate_context,
            &mut timeout,
            handler_index,
            ctx,
        )
        .await;

        isolate_context.checkpoint();
        drop(isolate_context);
        drop(timeout);
        handle.take_termination_error(None, "migration")??;
        result
    }

    async fn run_handler<RT: Runtime>(
        isolate: &mut RequestScope<'_, '_, '_, RT, Self>,
        timeout: &mut Timeout<RT>,
        handler_index: usize,
        ctx: JsonValue,
    ) -> anyhow::Result<Option<JsonValue>> {
        scope!(let v8_scope, isolate.scope());
        let mut scope = RequestScope::<RT, Self>::enter(v8_scope);

        let migration_url = ModuleSpecifier::parse(&format!("{CONVEX_SCHEME}:/migration.js"))?;
        let module = scope.eval_module(&migration_url, timeout).await?;
        let namespace = module
            .get_module_namespace()
            .to_object(&scope)
            .ok_or_else(|| anyhow!("Module namespace wasn't an object?"))?;
        let export_str = strings::runMigration.create(&scope)?;
        let run_migration_fn: v8::Local<v8::Function> = namespace
            .get(&scope, export_str.into())
            .ok_or_else(|| anyhow!("Couldn't find runMigration export"))?
            .try_into()?;

        let ctx_str = serde_json::to_string(&ctx)?;
        let ctx_v8 = v8::String::new(&mut scope, &ctx_str)
            .ok_or_else(|| anyhow!("Failed to create ctx string"))?;
        let index_v8 = v8::Integer::new(&mut scope, handler_index as i32);

        let result: Option<v8::Local<v8::Value>> = {
            let args = [index_v8.into(), ctx_v8.into()];
            scope.with_try_catch(|s| run_migration_fn.call(s, namespace.into(), &args))??
        };

        let Some(result) = result else {
            return Ok(None);
        };
        if result.is_undefined() || result.is_null() {
            return Ok(None);
        }
        let result_v8: v8::Local<v8::String> = result.try_into()?;
        let result_str = to_rust_string(&scope, &result_v8)?;
        let parsed: JsonValue = serde_json::from_str(&result_str)?;
        Ok(Some(parsed))
    }
}
