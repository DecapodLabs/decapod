import assert from "node:assert/strict";
import { resolve } from "node:path";
import test from "node:test";
import { DecapodClient } from "../../../bindings/typescript/src/index.ts";
import { createPiDecapodExtension } from "../src/index.ts";

function createMockClient(
  evalStatus = "allow",
  rpcOk = true,
  contextResult: Record<string, unknown> = { status: "ok", conflicts: [], decision_gates: [] },
): DecapodClient {
  const client = new DecapodClient({
    binaryPath: process.execPath,
    cwd: process.cwd(),
    agentId: "pi-test-agent",
  });
  const stubbed = Object.create(client) as DecapodClient;
  stubbed.evalPrompt = async () => ({
    allowed: evalStatus === "allow",
    status: evalStatus,
    raw: JSON.stringify({ status: evalStatus }),
    transport: client.transportInfo(),
  });
  stubbed.rpc = async <T = unknown>(op: string) => ({
    ok: rpcOk,
    op,
    result: (rpcOk ? (op === "context.resolve" ? contextResult : { can_work: true }) : undefined) as T,
    error: rpcOk ? undefined : "WORKTREE_REQUIRED",
    rawStdout: "",
    rawStderr: "",
    exitCode: rpcOk ? 0 : 1,
    transport: client.transportInfo(),
  });
  stubbed.resolveContext = async () => ({
    ok: rpcOk,
    op: "context.resolve",
    result: rpcOk ? contextResult : undefined,
    error: rpcOk ? undefined : "CONTEXT_UNAVAILABLE",
    rawStdout: "",
    rawStderr: "",
    exitCode: rpcOk ? 0 : 1,
    transport: client.transportInfo(),
  });
  return stubbed;
}

test("Pi extension preserves original and follow-up prompts across model/harness replacement", async () => {
  const ext = createPiDecapodExtension({
    sessionId: "sess-1",
    modelId: "model-a",
    harnessId: "pi-v1",
    client: createMockClient("allow", true),
  });

  const turn1 = await ext.onPromptReceived({
    turnId: "t1",
    rawHumanPrompt: "Implement mooring contracts with fail-closed expiry.",
    agentInterpretation: "Add src/decapod/core/mooring.rs",
  });
  assert.equal(turn1.evalAllowed, true);
  assert.equal(turn1.provenance.humanApprovalAuthenticated, false);
  assert.equal(turn1.transport.kind, "cli_process");
  assert.equal(turn1.transport.fallbackActive, true);

  const resumed = ext.onResume({
    nextModelId: "model-b",
    nextHarnessId: "pi-v2",
    expectedPromptSha256: turn1.provenance.rawPromptSha256,
  });
  assert.equal(resumed.safeToResume, true);
  assert.equal(resumed.state.modelId, "model-b");
  assert.equal(resumed.state.harnessId, "pi-v2");
  assert.equal(
    resumed.state.originalPrompt?.rawHumanPrompt,
    "Implement mooring contracts with fail-closed expiry.",
  );

  await ext.onPromptReceived({
    turnId: "t2",
    rawHumanPrompt: "Also verify wrapper tampering cannot bypass verification.",
  });
  assert.equal(ext.state().promptHistory.length, 2);
});

test("Pi extension surfaces Decision Gates, refuses settlement without proof, and blocks protected branches", async () => {
  const ext = createPiDecapodExtension({
    sessionId: "sess-2",
    modelId: "model-a",
    harnessId: "pi-v1",
    client: createMockClient("allow", true, {
      status: "unresolved_conflict",
      conflicts: [["K_1", "K_2"]],
      decision_gates: [{ id: "gate-1", prompt: "Choose option A or B" }],
    }),
  });

  assert.equal(ext.classifyDirectHostWrite(false), "outside_native_enforcement");
  assert.equal(ext.classifyDirectHostWrite(true), "cooperative_hook");

  const bypass = await ext.beforeMutation({
    targetPaths: ["src/lib.rs"],
    planConverged: true,
    invokedThroughDecapodHook: false,
  });
  assert.equal(bypass.allowed, false);
  assert.equal(bypass.boundary, "outside_native_enforcement");

  await ext.onPromptReceived({
    turnId: "t1",
    rawHumanPrompt: "Add budget tests",
  });

  const ctx = await ext.onContextResolve({ query: "budget" });
  assert.equal(ctx.decisionGateRequired, true);
  assert.equal(ctx.unresolvedConflicts.length, 1);

  // Pending Decision Gate blocks autonomous mutation and resume.
  const blockedByGate = await ext.beforeMutation({
    targetPaths: ["tests/houseboat_engine.rs"],
    planConverged: true,
    invokedThroughDecapodHook: true,
  });
  assert.equal(blockedByGate.allowed, false);
  assert.equal(blockedByGate.decisionGateRequired, true);

  const refusedResume = ext.onResume({
    nextModelId: "model-b",
    nextHarnessId: "pi-v2",
  });
  assert.equal(refusedResume.safeToResume, false);

  const toolGate = ext.onToolResult({
    toolName: "decapod.preflight",
    ok: true,
    outputSummary: "Human sign-off required",
    decisionGateRequired: true,
  });
  assert.equal(toolGate.allowed, false);
  assert.equal(toolGate.decisionGateRequired, true);

  const settledUnproven = await ext.onSettlement({
    branch: "agent/swarm-w7-storage/refa_01m82r48qybtzak5-storage-rbac",
    proofPassed: false,
  });
  assert.equal(settledUnproven.completionClaimAllowed, false);
});

test("Pi extension allows settlement only on non-protected branch with clean proof and no open Decision Gate", async () => {
  const ext = createPiDecapodExtension({
    sessionId: "sess-3",
    modelId: "model-a",
    harnessId: "pi-v1",
    client: createMockClient("allow", true),
  });

  await ext.onPromptReceived({
    turnId: "t1",
    rawHumanPrompt: "Run focused tests",
  });

  const allowedMutation = await ext.beforeMutation({
    targetPaths: ["tests/single_datastore.rs"],
    planConverged: true,
    invokedThroughDecapodHook: true,
  });
  assert.equal(allowedMutation.allowed, true);

  const protectedBranch = await ext.beforeValidationAndAdmission("master");
  assert.equal(protectedBranch.allowed, false);
  assert.equal(protectedBranch.boundary, "kernel_enforced");

  const settled = await ext.onSettlement({
    branch: "agent/swarm-w7-storage/refa_01m82r48qybtzak5-storage-rbac",
    proofPassed: true,
  });
  assert.equal(settled.completionClaimAllowed, true);
  assert.equal(settled.transport.fallbackActive, true);
});

test("Pi extension runs real-binary prompt safety eval when DECAPOD_TEST_BIN is set", async () => {
  const bin = process.env.DECAPOD_TEST_BIN;
  if (!bin) {
    return;
  }
  const repoRoot = resolve(process.cwd(), "../..");
  const client = new DecapodClient({
    binaryPath: bin,
    cwd: repoRoot,
    agentId: "pi-real-bin-test",
  });
  const ext = createPiDecapodExtension({
    sessionId: "sess-real-bin",
    modelId: "real-model",
    harnessId: "pi-real",
    client,
  });
  const turn = await ext.onPromptReceived({
    turnId: "real-1",
    rawHumanPrompt: "Verify storage compaction invariants cleanly.",
  });
  assert.equal(turn.evalAllowed, true);
  assert.equal(turn.transport.kind, "cli_process");
});
