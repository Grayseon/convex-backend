import type { TSESTree } from "@typescript-eslint/types";
import { createRule } from "../util.js";

type MessageIds = "no-migrate-outside-schema";

function isMigrateCall(node: TSESTree.CallExpression): boolean {
  if (node.callee.type !== "MemberExpression") {
    return false;
  }
  if (node.callee.property.type !== "Identifier") {
    return false;
  }
  return node.callee.property.name === "migrate";
}

export const noMigrateOutsideSchema = createRule<[], MessageIds>({
  name: "no-migrate-outside-schema",
  meta: {
    type: "problem",
    docs: {
      description:
        "Disallow `.migrate()` outside schema.ts — migrations only apply in schema definitions",
    },
    schema: [],
    messages: {
      "no-migrate-outside-schema":
        "`.migrate()` is only supported in `convex/schema.ts` for schema-driven data migrations.",
    },
  },
  defaultOptions: [],
  create(context) {
    const filename = context.filename.replace(/\\/g, "/");
    const isSchemaFile = filename.endsWith("/convex/schema.ts")
      || filename.endsWith("/convex/schema.js");

    if (isSchemaFile) {
      return {};
    }

    return {
      CallExpression(node: TSESTree.CallExpression) {
        if (!isMigrateCall(node)) {
          return;
        }
        context.report({
          node,
          messageId: "no-migrate-outside-schema",
        });
      },
    };
  },
});
