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

**Important:** Seed data **before** migrations run. If you push a schema with pending
migrations while tables are empty, those migrations mark complete immediately and
will not re-run when you add documents later.

### Scenario 1: Field migrations (`numbers`)

**Step 1 — Start with the old schema**

Temporarily replace `convex/schema.ts` with a schema that matches your existing
data (no migrations yet):

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

**Step 2 — Insert old-format data**

Create `numbers-seed.json`:

```json
[
  { "value": 10 },
  { "value": 42 }
]
```

Import it:

```bash
just convex import --table numbers --append -y numbers-seed.json
```

Or insert rows from the dashboard with only a numeric `value` field.

**Step 3 — Enable migrations**

Restore the full `numbers` schema from `convex/schema.ts` (with both `.migrate()`
calls on `key` and `value`). Save the file; `convex dev` will detect pending
migrations and prompt:

```
This push will run 2 schema migration(s):
  numbers.key  "add key field"
  numbers.value  "number to string and add key"
```

Approve the prompt (or pass `--yes`).

**Step 4 — Verify**

```bash
just convex data numbers
```

Expected result — each document gets a `key` and `value` becomes a string:

```
key    | value
-------|-------------
"item" | "item: 10"
"item" | "item: 42"
```

### Scenario 2: Table migration (`users`)

**Step 1 — Start with unmigrated users schema**

Use a schema with the `users` table but **without** the `.migrate()` call:

```typescript
users: defineTable({
  first: v.optional(v.string()),
  last: v.optional(v.string()),
  name: v.optional(v.string()),
}),
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

```bash
just convex import --table users --append -y users-seed.json
```

**Step 3 — Add the table migration**

Add the `.migrate("combine name fields", ...)` handler to the `users` table
definition and save. Approve the migration prompt when `convex dev` pushes.

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

## Current schema

The checked-in `convex/schema.ts` includes all migrations at once. That is the
target end state. Use the walkthrough above to test incrementally on a fresh local
backend, or reset and seed old-format data before pushing the full schema.

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
