import assert from "node:assert/strict";
import test from "node:test";
import { DecapodClient } from "../../../bindings/typescript/src/index.ts";
import { createPiDecapodExtension } from "../src/index.ts";

function createMockClient(evalStatus = "allow", rpcOk = true): DecapodClient {
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
  });
  stubbed.rpc = async <T = unknown>(op: string) => ({
    ok: rpcOk,
    op,
    result: (rpcOk ? { can_work: true } : undefined) as T,
    error: rpcOk ? undefined : "WORKTREE_REQUIRED",
    rawStdout: "",
    rawStderr: "",
    exitCode: rpcOk ? 0 : 1,
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

  const afterSwap = ext.switchModelOrHarness("model-b", "pi-v2");
  assert.equal(afterSwap.modelId, "model-b");
  assert.equal(afterSwap.harnessId, "pi-v2");
  assert.equal(
    afterSwap.originalPrompt?.rawHumanPrompt,
    "Implement mooring contracts with fail-closed expiry.",
  );

  await ext.onPromptReceived({
    turnId: "t2",
    rawHumanPrompt: "Also verify wrapper tampering cannot bypass verification.",
  });
  assert.equal(ext.state().promptHistory.length, 2);
  assert.equal(
    ext.state().originalPrompt?.rawHumanPrompt,
    "Implement mooring contracts with fail-closed expiry.",
  );
});

test("Pi extension refuses direct host bypass and unconverged plans, and blocks protected branches", async () => {
  const ext = createPiDecapodExtension({
    sessionId: "sess-2",
    modelId: "model-a",
    harnessId: "pi-v1",
    client: createMockClient("allow", true),
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

  const unconverged = await ext.beforeMutation({
    targetPaths: ["tests/houseboat_engine.rs"],
    planConverged: false,
    invokedThroughDecapodHook: true,
  });
  assert.equal(unconverged.allowed, false);

  const allowedMutation = await ext.beforeMutation({
    targetPaths: ["tests/houseboat_engine.rs"],
    planConverged: true,
    invokedThroughDecapodHook: true,
  });
  assert.equal(allowedMutation.allowed, true);

  const protectedBranch = await ext.beforeValidationAndAdmission("master");
  assert.equal(protectedBranch.allowed, false);
  assert.equal(protectedBranch.boundary, "kernel_enforced");

  const workBranch = await ext.beforeValidationAndAdmission(
    "agent/swarm-w6-houseboat/code_01m81me586f0pmvj-houseboat",
  );
  assert.equal(workBranch.allowed, true);
});
