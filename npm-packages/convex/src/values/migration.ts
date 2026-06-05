import { GenericValidator } from "./validator.js";
import {
  VArray,
  VMigrated,
  VObject,
  ValidatorJSON,
  VUnion,
} from "./validators.js";

/**
 * Context passed to a field-level migration handler.
 *
 * @public
 */
export type MigrationContext<
  TOld,
  TNew,
  TDoc extends Record<string, unknown>,
> = {
  /** Current stored value for this field (may be an older type). */
  oldValue: TOld;
  /** Full document as stored in the database (sibling fields are accessible). */
  doc: TDoc;
  /** Dot-separated path, e.g. `"value"` or `"address.city"`. */
  fieldPath: string;
  tableName: string;
  /** True if the field is absent on the stored document. */
  isMissing: boolean;
};

/**
 * Context passed to a table-level migration handler.
 *
 * @public
 */
export type TableMigrationContext<TDoc extends Record<string, unknown>> = {
  /** Full document as stored in the database. */
  doc: TDoc;
  tableName: string;
};

/**
 * Handler for migrating a single field on a document.
 *
 * Return the new field value to patch, or `undefined` to skip this document.
 *
 * @public
 */
export type FieldMigrationHandler<
  TOld,
  TNew,
  TDoc extends Record<string, unknown>,
> = (ctx: MigrationContext<TOld, TNew, TDoc>) => TNew | undefined | void;

/**
 * Handler for migrating an entire document.
 *
 * Return a partial patch object, or `undefined` to skip this document.
 *
 * @public
 */
export type TableMigrationHandler<TDoc extends Record<string, unknown>> = (
  ctx: TableMigrationContext<TDoc>,
) => Partial<TDoc> | undefined | void;

/** @internal */
export type MigrationScope = "field" | "table";

/** @internal */
export type ExportedFieldMigration = {
  id: string;
  tableName: string;
  scope: "field";
  fieldPath: string;
  targetValidator: ValidatorJSON;
  handlerIndex: number;
  handlerSource: string;
};

/** @internal */
export type ExportedTableMigration = {
  id: string;
  tableName: string;
  scope: "table";
  handlerIndex: number;
  handlerSource: string;
};

/** @internal */
export type ExportedMigration =
  | ExportedFieldMigration
  | ExportedTableMigration;

/** @internal */
export function serializeHandler(handler: Function): string {
  const source = handler.toString();
  if (source.length === 0) {
    throw new Error(
      "Migration handler could not be serialized. Use an inline function or arrow function.",
    );
  }
  return source;
}

/** @internal */
export function isVMigrated(
  validator: GenericValidator,
): validator is VMigrated<any, any, any> {
  return validator.kind === "migrated";
}

/** @internal */
export function collectFieldMigrationsFromValidator(
  validator: GenericValidator,
  tableName: string,
  fieldPathPrefix: string,
  migrations: ExportedMigration[],
  handlers: string[],
): GenericValidator {
  if (isVMigrated(validator)) {
    const fieldPath = fieldPathPrefix;
    const handlerSource = serializeHandler(validator.handler);
    const handlerIndex = handlers.length;
    handlers.push(handlerSource);
    migrations.push({
      id: validator.migrationId,
      tableName,
      scope: "field",
      fieldPath,
      targetValidator: validator.inner.json,
      handlerIndex,
      handlerSource,
    });
    return validator.inner;
  }

  if (validator.kind === "object") {
    const objectValidator = validator as VObject<
      any,
      Record<string, GenericValidator>
    >;
    const newFields: Record<string, GenericValidator> = {};
    for (const [fieldName, fieldValidator] of Object.entries(
      objectValidator.fields,
    )) {
      const fieldPath = fieldPathPrefix
        ? `${fieldPathPrefix}.${fieldName}`
        : fieldName;
      newFields[fieldName] = collectFieldMigrationsFromValidator(
        fieldValidator as GenericValidator,
        tableName,
        fieldPath,
        migrations,
        handlers,
      );
    }
    return new VObject({
      isOptional: objectValidator.isOptional,
      fields: newFields,
    });
  }

  if (validator.kind === "union") {
    const unionValidator = validator as VUnion<any, GenericValidator[]>;
    const newMembers = unionValidator.members.map((member) =>
      collectFieldMigrationsFromValidator(
        member as GenericValidator,
        tableName,
        fieldPathPrefix,
        migrations,
        handlers,
      ),
    );
    return new VUnion({
      isOptional: unionValidator.isOptional,
      members: newMembers as [GenericValidator, ...GenericValidator[]],
    });
  }

  if (validator.kind === "array") {
    const arrayValidator = validator as VArray<any, GenericValidator>;
    const newElement = collectFieldMigrationsFromValidator(
      arrayValidator.element as GenericValidator,
      tableName,
      fieldPathPrefix,
      migrations,
      handlers,
    );
    return new VArray({
      isOptional: arrayValidator.isOptional,
      element: newElement as GenericValidator,
    });
  }

  return validator;
}
