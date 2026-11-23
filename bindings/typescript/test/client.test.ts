import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import test from "node:test";
import {
  DecapodClient,
  classifyDecapodError,
  createPromptProvenance,
  sha256Digest,
  validateDarkmatterSnapshot,
  type DarkmatterReadonlySnapshot,
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

test("DecapodClient exposes explicit cli_process fallback metadata when native addon is absent", () => {
  const client = new DecapodClient({
    binaryPath: process.execPath,
    cwd: process.cwd(),
    agentId: "agent-transport",
  });
  const info = client.transportInfo();
  assert.equal(info.kind, "cli_process");
  assert.equal(info.fallbackActive, true);
  assert.ok(info.reason.includes("process carrier"));
});

test("classifyDecapodError preserves Decision Gate, protected workspace, and session categories", () => {
  assert.equal(classifyDecapodError(1, "DECISION_GATE: human required", "").code, "DECISION_GATE");
  assert.equal(classifyDecapodError(1, "PROTECTED_BRANCH master", "").code, "PROTECTED_WORKSPACE");
  assert.equal(classifyDecapodError(1, "INVALID session password", "").code, "INVALID_SESSION");
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

test("validateDarkmatterSnapshot accepts canonical offline fixture and rejects boundary violations (#1711)", () => {
  const fixturePath = resolve(
    process.cwd(),
    "../../tests/fixtures/darkmatter_readonly_snapshot.v1.json",
  );
  const snapshot = JSON.parse(readFileSync(fixturePath, "utf8")) as DarkmatterReadonlySnapshot;
  const ok = validateDarkmatterSnapshot(snapshot);
  assert.equal(ok.ok, true);
  assert.deepEqual(ok.errors, []);

  const invalid = validateDarkmatterSnapshot({
    ...snapshot,
    read_only: false,
    governance_json_path: "../escape.json",
  });
  assert.equal(invalid.ok, false);
  assert.ok(invalid.errors.some((e) => e.includes("DARKMATTER_BOUNDARY_VIOLATION")));
  assert.ok(invalid.errors.some((e) => e.includes("PATH_ESCAPE")));
});

test("DecapodClient real-binary RPC and CLI contract verification when DECAPOD_TEST_BIN is set", async () => {
  const bin = process.env.DECAPOD_TEST_BIN;
  if (!bin) {
    return;
  }
  const repoRoot = resolve(process.cwd(), "../..");
  const client = new DecapodClient({
    binaryPath: bin,
    cwd: repoRoot,
    agentId: "ts-contract-test",
  });

  const prov = createPromptProvenance({
    turnId: "ts-bin-1",
    rawHumanPrompt: "Inspect storage compaction status safely.",
    sourceChannel: "harness_observed",
  });
  const evalRes = await client.evalPrompt(prov);
  assert.equal(evalRes.allowed, true);
  assert.equal(evalRes.status, "allow");
  assert.equal(evalRes.transport?.kind, "cli_process");

  const status = await client.houseboatStatus();
  assert.equal(status.schema_version, "decapod.houseboat.v1");

  const fixturePath = resolve(repoRoot, "tests/fixtures/darkmatter_readonly_snapshot.v1.json");
  const dm = await client.verifyDarkmatterSnapshot(fixturePath);
  assert.equal(dm.ok, true);

  const schemaRpc = await client.rpc("schema.get", {});
  assert.equal(schemaRpc.ok, true);
});
