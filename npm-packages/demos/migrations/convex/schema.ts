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
