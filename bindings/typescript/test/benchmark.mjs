import { performance } from "node:perf_hooks";
import { resolve } from "node:path";
import {
  DecapodClient,
  createPromptProvenance,
  resolveDefaultNativeAddonPath,
} from "../src/index.ts";

function percentile(sorted, pct) {
  if (sorted.length === 0) return 0;
  const idx = Math.min(sorted.length - 1, Math.max(0, Math.ceil((pct / 100) * sorted.length) - 1));
  return sorted[idx];
}

async function bench(label, iterations, fn) {
  const samples = [];
  for (let i = 0; i < iterations; i += 1) {
    const start = performance.now();
    await fn();
    samples.push(performance.now() - start);
  }
  samples.sort((a, b) => a - b);
  return {
    label,
    iterations,
    p50_ms: Number(percentile(samples, 50).toFixed(3)),
    p95_ms: Number(percentile(samples, 95).toFixed(3)),
    max_ms: Number((samples[samples.length - 1] ?? 0).toFixed(3)),
  };
}

const addonPath = resolveDefaultNativeAddonPath();
if (!addonPath) {
  throw new Error("Native N-API addon not built. Run `npm run build:native` first.");
}

const repoRoot = resolve(process.cwd(), "../..");
const cliBin = process.env.DECAPOD_TEST_BIN ?? "/Users/arx/.cargo/bin/decapod";
const prov = createPromptProvenance({
  turnId: "bench-1",
  rawHumanPrompt: "Inspect repository governance status safely.",
  sourceChannel: "harness_observed",
});

const nativeClient = new DecapodClient({
  cwd: repoRoot,
  agentId: "bench-native",
  nativeAddonPath: addonPath,
});
const cliClient = new DecapodClient({
  binaryPath: cliBin,
  cwd: repoRoot,
  agentId: "bench-cli",
});

const nativeEval = await bench("native_napi:evalPrompt", 50, async () => {
  const r = await nativeClient.evalPrompt(prov);
  if (!r.allowed) throw new Error("evalPrompt failed");
});
const cliEval = await bench("cli_process:evalPrompt", 15, async () => {
  const r = await cliClient.evalPrompt(prov);
  if (!r.allowed) throw new Error("cli evalPrompt failed");
});

const nativeRpc = await bench("native_napi:rpc(schema.get)", 25, async () => {
  const r = await nativeClient.rpc("schema.get", { entity: "todo" });
  if (!r.ok) throw new Error(`native rpc failed: ${r.error}`);
});
const cliRpc = await bench("cli_process:rpc(schema.get)", 10, async () => {
  const r = await cliClient.rpc("schema.get", { entity: "todo" });
  if (!r.ok) throw new Error(`cli rpc failed: ${r.error}`);
});

console.log(
  JSON.stringify(
    {
      schema_version: "1.0.0",
      addonPath,
      cliBin,
      results: [nativeEval, cliEval, nativeRpc, cliRpc],
      speedup: {
        eval_p50_x: Number((cliEval.p50_ms / Math.max(nativeEval.p50_ms, 0.001)).toFixed(1)),
        rpc_p50_x: Number((cliRpc.p50_ms / Math.max(nativeRpc.p50_ms, 0.001)).toFixed(1)),
      },
    },
    null,
    2,
  ),
);
