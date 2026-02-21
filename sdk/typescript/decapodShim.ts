import { spawnSync } from "node:child_process";
import { randomUUID } from "node:crypto";

export interface RpcEnvelope {
  id: string;
  success: boolean;
  [key: string]: unknown;
}

export function runRpc(op: string, params: Record<string, unknown> = {}, requestId?: string): RpcEnvelope {
  const payload = {
    id: requestId ?? randomUUID(),
    op,
    params,
  };
  const proc = spawnSync("decapod", ["rpc", "--stdin"], {
    input: JSON.stringify(payload),
    encoding: "utf-8",
  });
  if (proc.status !== 0) {
    throw new Error(proc.stderr || `decapod rpc failed with status ${proc.status}`);
  }
  return JSON.parse(proc.stdout) as RpcEnvelope;
}

export function runValidateEnvelope(): Record<string, unknown> {
  const proc = spawnSync("decapod", ["validate"], { encoding: "utf-8" });
  return {
    envelope_version: "1.0.0",
    cmd: "validate",
    status: proc.status === 0 ? "ok" : "error",
    exit_code: proc.status ?? 1,
    stdout: proc.stdout,
    stderr: proc.stderr,
  };
}
