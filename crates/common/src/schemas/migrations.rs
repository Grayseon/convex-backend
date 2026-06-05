use value::{
    sha256::Sha256,
    TableName,
};

use super::validator::Validator;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchemaMigrationScope {
    Field,
    Table,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaMigration {
    pub id: String,
    pub table_name: TableName,
    pub scope: SchemaMigrationScope,
    pub field_path: Option<String>,
    pub target_validator: Option<Validator>,
    pub handler_index: usize,
    pub handler_source: String,
    pub handler_hash: String,
}

impl SchemaMigration {
    pub fn new(
        id: String,
        table_name: TableName,
        scope: SchemaMigrationScope,
        field_path: Option<String>,
        target_validator: Option<Validator>,
        handler_index: usize,
        handler_source: String,
    ) -> Self {
        let handler_hash = Sha256::hash(handler_source.as_bytes()).as_hex();
        Self {
            id,
            table_name,
            scope,
            field_path,
            target_validator,
            handler_index,
            handler_source,
            handler_hash,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handler_hash_is_stable() {
        let migration = SchemaMigration::new(
            "test".to_string(),
            "numbers".parse().unwrap(),
            SchemaMigrationScope::Field,
            Some("value".to_string()),
            None,
            0,
            "(ctx) => ctx.oldValue".to_string(),
        );
        let migration2 = SchemaMigration::new(
            "test".to_string(),
            "numbers".parse().unwrap(),
            SchemaMigrationScope::Field,
            Some("value".to_string()),
            None,
            0,
            "(ctx) => ctx.oldValue".to_string(),
        );
        assert_eq!(migration.handler_hash, migration2.handler_hash);
        assert_ne!(
            migration.handler_hash,
            SchemaMigration::new(
                "test".to_string(),
                "numbers".parse().unwrap(),
                SchemaMigrationScope::Field,
                Some("value".to_string()),
                None,
                0,
                "(ctx) => ctx.newValue".to_string(),
            )
            .handler_hash
        );
    }
}
