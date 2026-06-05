import { chalkStderr } from "chalk";
import { Context } from "../../bundler/context.js";
import {
  changeSpinner,
  logFinishedStep,
  logMessage,
  stopSpinner,
} from "../../bundler/log.js";
import { promptYesNo } from "./utils/prompts.js";
import { Span } from "./tracing.js";
import { StartPushRequest } from "./deployApi/startPush.js";
import { evaluatePush } from "./deploy2.js";

export async function checkForPendingMigrations({
  ctx,
  span,
  request,
  options,
  askForConfirmation,
}: {
  ctx: Context;
  span: Span;
  request: StartPushRequest;
  options: {
    url: string;
    deploymentName: string | null;
    adminKey: string;
  };
  askForConfirmation: boolean;
}): Promise<{ migrationsApproved: boolean }> {
  changeSpinner("Checking for pending schema migrations...");

  const { pendingMigrations } = await evaluatePush(ctx, span, request, options);
  const migrations = pendingMigrations ?? [];

  if (migrations.length === 0) {
    logFinishedStep("No schema migrations to run");
    return { migrationsApproved: false };
  }

  const lines = migrations.map((migration) => {
    const location =
      migration.scope === "field"
        ? `${migration.tableName}.${migration.fieldPath ?? ""}`
        : `${migration.tableName} (table)`;
    return `  ${location}  "${migration.id}"`;
  });

  if (!askForConfirmation) {
    logFinishedStep(
      `This push will run ${migrations.length} schema migration(s) automatically`,
    );
    return { migrationsApproved: true };
  }

  if (!process.stdin.isTTY) {
    return await ctx.crash({
      exitCode: 1,
      errorType: "invalid filesystem data",
      printedMessage:
        `This push will run ${migrations.length} schema migration(s):\n` +
        lines.join("\n") +
        "\n\nRe-run in an interactive terminal to confirm, or use --yes to approve migrations.",
    });
  }

  stopSpinner();
  logMessage(
    chalkStderr.yellow(
      `This push will run ${migrations.length} schema migration(s):\n` +
        lines.join("\n") +
        "\n\nThese migrations run automatically in production deploys.",
    ),
  );

  const approved = await promptYesNo(ctx, {
    message: "Run migrations and continue push?",
    default: true,
  });
  if (!approved) {
    return await ctx.crash({
      exitCode: 1,
      errorType: "invalid filesystem data",
      printedMessage: "Push aborted: schema migrations were not approved.",
    });
  }

  return { migrationsApproved: true };
}
