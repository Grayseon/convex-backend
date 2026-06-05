# Migrations Proposal
Test with `just convex dev`

```javascript
export default defineSchema({
  numbers: defineTable({
    value: v.number()
  }),
})```

|
V

```javascript
export default defineSchema({
  numbers: defineTable({
    key: v.string().migrate("add key field", (ctx) => {
      if (!ctx.isMissing) return ctx.oldValue as string;
      return "item";
    }),
    value: v.string()
  }),
})```

|
V

```javascript
export default defineSchema({
  numbers: defineTable({
    key: v.string().migrate("add key field", (ctx) => {
      if (!ctx.isMissing) return ctx.oldValue as string;
      return "item";
    }),
    value: v.string().migrate("migrate: number to string and add key", (ctx) => {
      const oldValue = ctx.oldValue;
      const key = (ctx.doc.key as string | undefined) ?? "item";

      if (typeof oldValue === "number") return `${key}: ${oldValue.toString()}`;
      return oldValue as string;
    }),
  }),
})```