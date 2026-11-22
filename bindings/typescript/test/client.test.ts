import assert from "node:assert/strict";
import test from "node:test";
import {
  DecapodClient,
  createPromptProvenance,
  sha256Digest,
} from "../src/index.ts";

test("createPromptProvenance keeps raw prompt distinct from interpretation and never authenticates approval", () => {
  const prov = createPromptProvenance({
    turnId: "turn-1",
    rawHumanPrompt: "Fix storage compaction test without touching CI.",
    sourceChannel: "harness_observed",
    agentInterpretation: "Run local compaction tests only",
  });

  assert.equal(prov.rawHumanPrompt, "Fix storage compaction test without touching CI.");
  assert.equal(prov.agentInterpretation, "Run local compaction tests only");
  assert.equal(prov.rawPromptSha256, sha256Digest(prov.rawHumanPrompt));
  assert.equal(prov.humanApprovalAuthenticated, false);
});

test("DecapodClient supports concurrent invocations with isolated agent env and zero global state", async () => {
  const clientA = new DecapodClient({
    binaryPath: process.execPath,
    cwd: process.cwd(),
    agentId: "agent-alpha",
    sessionPassword: "pw-alpha",
  });
  const clientB = new DecapodClient({
    binaryPath: process.execPath,
    cwd: process.cwd(),
    agentId: "agent-beta",
    sessionPassword: "pw-beta",
  });

  const script =
    "console.log(JSON.stringify({agent: process.env.DECAPOD_AGENT_ID, pw: process.env.DECAPOD_SESSION_PASSWORD}))";
  const [resA, resB] = await Promise.all([
    clientA.exec(["-e", script]),
    clientB.exec(["-e", script]),
  ]);

  assert.equal(resA.exitCode, 0);
  assert.equal(resB.exitCode, 0);
  assert.deepEqual(JSON.parse(resA.stdout), { agent: "agent-alpha", pw: "pw-alpha" });
  assert.deepEqual(JSON.parse(resB.stdout), { agent: "agent-beta", pw: "pw-beta" });
});

test("DecapodClient honours AbortSignal cancellation cleanly without process.exit", async () => {
  const client = new DecapodClient({
    binaryPath: process.execPath,
    cwd: process.cwd(),
    agentId: "agent-cancel",
  });
  const controller = new AbortController();
  const promise = client.exec(["-e", "setTimeout(() => {}, 10_000)"], {
    signal: controller.signal,
  });
  setTimeout(() => controller.abort(), 40);

  await assert.rejects(promise, (err: unknown) => {
    return err instanceof DOMException && err.name === "AbortError";
  });
});
