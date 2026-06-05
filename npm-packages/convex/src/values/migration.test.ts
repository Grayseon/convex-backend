import { describe, expect, test } from "vitest";
import { v } from "./validator.js";
import {
  collectFieldMigrationsFromValidator,
  serializeHandler,
} from "./migration.js";
import type { ExportedMigration } from "./migration.js";
import { defineSchema, defineTable } from "../server/schema.js";

describe("VMigrated", () => {
  test("json returns target validator only", () => {
    const validator = v.string().migrate("test", () => "x");
    expect(validator.json).toEqual({ type: "string" });
  });

  test("empty migration id throws", () => {
    expect(() => v.string().migrate("", () => "x")).toThrow(
      "Migration id must be a non-empty string",
    );
  });
});

describe("collectFieldMigrationsFromValidator", () => {
  test("collects nested field migrations", () => {
    const migrations: ExportedMigration[] = [];
    const handlers: string[] = [];
    const validator = v.object({
      value: v.string().migrate("number to string", (ctx) => {
        if (typeof ctx.oldValue === "number") return ctx.oldValue.toString();
      }),
      nested: v.object({
        city: v.string().migrate("add prefix", () => "unknown"),
      }),
    });
    collectFieldMigrationsFromValidator(
      validator,
      "users",
      "",
      migrations,
      handlers,
    );
    expect(migrations).toHaveLength(2);
    expect(migrations[0]).toMatchObject({
      id: "number to string",
      tableName: "users",
      scope: "field",
      fieldPath: "value",
      handlerIndex: 0,
    });
    expect(migrations[1]).toMatchObject({
      id: "add prefix",
      fieldPath: "nested.city",
      handlerIndex: 1,
    });
    expect(handlers).toHaveLength(2);
    expect(handlers[0]).toContain("oldValue");
  });
});

describe("serializeHandler", () => {
  test("serializes inline arrow functions", () => {
    const source = serializeHandler((ctx: { oldValue: number }) => {
      return ctx.oldValue.toString();
    });
    expect(source).toContain("oldValue");
  });
});

describe("SchemaDefinition.export", () => {
  test("exports migrations and handlers", () => {
    const schema = defineSchema({
      numbers: defineTable({
        key: v.string(),
        value: v.string().migrate("number to string", (ctx) => {
          if (typeof ctx.oldValue === "number") return ctx.oldValue.toString();
        }),
      }),
      users: defineTable({
        first: v.optional(v.string()),
        last: v.optional(v.string()),
        name: v.optional(v.string()),
      }).migrate("combine names", (ctx) => {
        if (ctx.doc.first && ctx.doc.last && !ctx.doc.name) {
          return { name: `${ctx.doc.first} ${ctx.doc.last}` };
        }
      }),
    });
    const exported = JSON.parse(schema.export());
    expect(exported.migrations).toHaveLength(2);
    expect(exported.migrationHandlers).toHaveLength(2);
    expect(exported.migrations[0]).toMatchObject({
      id: "number to string",
      tableName: "numbers",
      scope: "field",
      fieldPath: "value",
    });
    expect(exported.migrations[1]).toMatchObject({
      id: "combine names",
      tableName: "users",
      scope: "table",
    });
    expect(exported.tables[0].documentType).toMatchObject({
      type: "object",
      value: {
        key: { fieldType: { type: "string" } },
        value: { fieldType: { type: "string" } },
      },
    });
  });
});
