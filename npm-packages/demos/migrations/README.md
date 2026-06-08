# Schema migrations demo

This demo shows **inline schema migrations**: data transformations declared directly in
`convex/schema.ts` with `.migrate()`. Migrations run automatically during
`convex dev` and `convex deploy`, **before** schema validation.

The demo covers two patterns:

- **Field migrations** on the `numbers` table — add a field and change a field type
- **Table migration** on the `users` table — patch multiple fields in one pass

## How it works

When you push a schema that declares `.migrate()` handlers, Convex:

1. Detects pending migrations and prompts in dev (production runs them automatically)
2. Scans every document in the affected table
3. Runs your handler per document
4. Patches documents that return a value
5. Records the migration `id` so it never runs again
6. Validates the schema once migrations finish

### Field migrations

Call `.migrate(id, handler)` on a validator. The handler receives the stored field
value plus the full document:

```typescript
value: v.string().migrate("number to string and add key", (ctx) => {
  const oldValue = ctx.oldValue;
  const key = (ctx.doc.key as string | undefined) ?? "item";

  if (typeof oldValue === "number") {
    return `${key}: ${oldValue.toString()}`;
  }
  return oldValue as string;
}),
```

`ctx` includes:

| Field | Description |
| --- | --- |
| `oldValue` | Current stored value (may be an older type) |
| `doc` | Full document as stored in the database |
| `fieldPath` | Dot-separated path, e.g. `"value"` |
| `tableName` | Table name |
| `isMissing` | `true` if the field is absent on the document |

Return the **new field value** to patch that field. Return `undefined` to skip the
document.

### Table migrations

Call `.migrate(id, handler)` on a `defineTable(...)` chain. The handler receives the
full document and returns a **partial patch object**:

```typescript
users: defineTable({
  first: v.optional(v.string()),
  last: v.optional(v.string()),
  name: v.optional(v.string()),
}).migrate("combine name fields", (ctx) => {
  if (ctx.doc.first && ctx.doc.last) {
    return {
      name: `${ctx.doc.first} ${ctx.doc.last}`,
      first: undefined,
      last: undefined,
    };
  }
}),
```

`ctx` includes `doc` and `tableName`. Use `undefined` in the patch to **remove** a
field. Return `undefined` from the handler to skip a document.

### Migration rules

- Each migration needs a **stable string `id`**. Convex records completed ids and
  skips them on future pushes.
- If you change handler code under the same `id`, the push fails. Use a **new `id`**
  instead.
- Migrations only run **once**. If a migration completes while a table is empty,
  documents added later are not migrated automatically.
- Throw from a handler to fail the push with an error.

## Running locally

This demo targets the **local open-source backend** in this monorepo.

### Prerequisites

From the repo root, build JS packages if you have not already:

```bash
just rush build
```

### Terminal setup

Use three terminals (dashboard is optional but helpful for inserting test data):

**Terminal 1 — backend** (repo root):

```bash
just run-local-backend
```

**Terminal 2 — dashboard** (repo root, optional):

```bash
just run-dashboard http://127.0.0.1:3210
```

Open the dashboard (typically `http://localhost:6790`) to view tables and insert
documents.

**Terminal 3 — Convex dev** (this directory):

```bash
cd npm-packages/demos/migrations
just convex dev
```

`just convex` wires the CLI to `http://127.0.0.1:3210` with the local admin key.

### Reset between test runs

Migrations are one-time per deployment. To replay the full walkthrough:

```bash
# Stop convex dev and the local backend first, then from repo root:
just reset-local-backend
```

Restart `just run-local-backend` and run through the scenarios below again.

## Testing walkthrough

Each step below gives a complete `convex/schema.ts` to copy in full.

**Important:** Migrations only run once. If a migration completes while its table is
empty, it will not re-run when you add documents later. Scenario 1 is staged across
two pushes so you add numeric data **between** migrations and can watch the
`number to string and add key` migration transform it.

### Scenario 1: Field migrations (`numbers`)

This scenario uses **two separate pushes** — first the `add key field` migration,
then you insert data, then the `number to string and add key` migration runs on
those documents.

**Step 1 — Old schema (no migrations)**

Create or replace `convex/schema.ts` with:

```typescript
import { defineSchema, defineTable } from "convex/server";
import { v } from "convex/values";

export default defineSchema({
  numbers: defineTable({
    value: v.number(),
  }),
});
```

Push it with `just convex dev`.

**Step 2 — First migration only (`add key field`)**

Replace `convex/schema.ts` with:

```typescript
import { defineSchema, defineTable } from "convex/server";
import { v } from "convex/values";

export default defineSchema({
  numbers: defineTable({
    key: v.string().migrate("add key field", (ctx) => {
      if (!ctx.isMissing) return ctx.oldValue as string;
      return "item";
    }),
    value: v.number(),
  }),
});
```

Save and approve the migration prompt:

```
This push will run 1 schema migration(s):
  numbers.key  "add key field"
```

The table is empty, so this migration completes immediately with nothing to
transform — that is expected.

**Step 3 — Insert data with `key` and numeric `value`**

Create `numbers-seed.json`:

```json
[
  { "key": "item", "value": 10 },
  { "key": "item", "value": 42 }
]
```

Import it **before** pushing the second migration:

```bash
just convex import --table numbers --append -y numbers-seed.json
```

Confirm the data is in place:

```bash
just convex data numbers
```

You should see each document has a `key` and a numeric `value`:

```
key    | value
-------|------
"item" | 10
"item" | 42
```

**Step 4 — Second migration (`number to string and add key`)**

Replace `convex/schema.ts` with:

```typescript
import { defineSchema, defineTable } from "convex/server";
import { v } from "convex/values";

export default defineSchema({
  numbers: defineTable({
    key: v.string(),
    value: v.string().migrate("number to string and add key", (ctx) => {
      const oldValue = ctx.oldValue;
      const key = (ctx.doc.key as string | undefined) ?? "item";

      if (typeof oldValue === "number") return `${key}: ${oldValue.toString()}`;
      return oldValue as string;
    }),
  }),
});
```

The `add key field` migration already ran, so `key` no longer has `.migrate()`.
Save and approve the migration prompt:

```
This push will run 1 schema migration(s):
  numbers.value  "number to string and add key"
```

This migration converts the numeric `value` fields you inserted in Step 3.

**Step 5 — Verify**

```bash
just convex data numbers
```

Expected result — `key` is unchanged and `value` is now a string:

```
key    | value
-------|-------------
"item" | "item: 10"
"item" | "item: 42"
```

### Scenario 2: Table migration (`users`)

Run `just reset-local-backend` and restart the backend before this scenario if you
already completed Scenario 1.

**Step 1 — Users table without migration**

Replace `convex/schema.ts` with:

```typescript
import { defineSchema, defineTable } from "convex/server";
import { v } from "convex/values";

export default defineSchema({
  users: defineTable({
    first: v.optional(v.string()),
    last: v.optional(v.string()),
    name: v.optional(v.string()),
  }),
});
```

Push with `just convex dev`.

**Step 2 — Insert users with `first` and `last` only**

Create `users-seed.json`:

```json
[
  { "first": "Ada", "last": "Lovelace" },
  { "first": "Grace", "last": "Hopper" }
]
```

Import it:

```bash
just convex import --table users --append -y users-seed.json
```

**Step 3 — Schema with table migration**

Replace `convex/schema.ts` with:

```typescript
import { defineSchema, defineTable } from "convex/server";
import { v } from "convex/values";

export default defineSchema({
  users: defineTable({
    first: v.optional(v.string()),
    last: v.optional(v.string()),
    name: v.optional(v.string()),
  }).migrate("combine name fields", (ctx) => {
    if (ctx.doc.first && ctx.doc.last) {
      return {
        name: `${ctx.doc.first} ${ctx.doc.last}`,
        first: undefined,
        last: undefined,
      };
    }
  }),
});
```

Save the file and approve the migration prompt when `convex dev` pushes.

**Step 4 — Verify**

```bash
just convex data users
```

Expected result — `name` is set and `first`/`last` are removed:

```
name              | first | last
------------------|-------|-----
"Ada Lovelace"    |       |
"Grace Hopper"    |       |
```

### Final schema (both tables, all migrations)

This is the combined end state with a **required** `key` field. Follow the staged
Scenario 1 walkthrough above to see each migration in isolation; use this schema
once both migrations have already run, or on a new project where you are not
stepping through the demo.

```typescript
import { defineSchema, defineTable } from "convex/server";
import { v } from "convex/values";

export default defineSchema({
  numbers: defineTable({
    key: v.string().migrate("add key field", (ctx) => {
      if (!ctx.isMissing) return ctx.oldValue as string;
      return "item";
    }),
    value: v.string().migrate("number to string and add key", (ctx) => {
      const oldValue = ctx.oldValue;
      const key = (ctx.doc.key as string | undefined) ?? "item";

      if (typeof oldValue === "number") return `${key}: ${oldValue.toString()}`;
      return oldValue as string;
    }),
  }),

  users: defineTable({
    first: v.optional(v.string()),
    last: v.optional(v.string()),
    name: v.optional(v.string()),
  }).migrate("combine name fields", (ctx) => {
    if (ctx.doc.first && ctx.doc.last) {
      return {
        name: `${ctx.doc.first} ${ctx.doc.last}`,
        first: undefined,
        last: undefined,
      };
    }
  }),
});
```

To run both scenarios on a fresh backend, follow the steps in order:

1. `just reset-local-backend` and restart the backend
2. Scenario 1, Steps 1–5 (two migration pushes with `numbers-seed.json` in between)
3. Scenario 2, Steps 1–4 (users seed before the table migration push)

## Troubleshooting

| Problem | Cause | Fix |
| --- | --- | --- |
| Migration completes but data unchanged | Migration ran on an empty table, or handler returned `undefined` for every doc | `just reset-local-backend`, seed old-format data, then push migrations |
| `MigrationHandlerChanged` error | Handler code changed under an existing migration `id` | Use a new migration `id`, or reset the local backend |
| No migration prompt | All migration ids already recorded as complete | Reset the backend, or add a new migration with a new `id` |
| `first`/`last` not removed | Patch used `null` instead of `undefined` | Use `undefined` to delete fields in table migration patches |

## Further reading

- [Schemas — Schema migrations](https://docs.convex.dev/database/schemas#schema-migrations)
- For large or multi-step migrations, see [`@convex-dev/migrations`](https://www.npmjs.com/package/@convex-dev/migrations)
