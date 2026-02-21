const { spawnSync } = require("node:child_process");
const { randomUUID } = require("node:crypto");

function runRpc(op, params = {}, requestId) {
  const payload = {
    id: requestId || randomUUID(),
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
  return JSON.parse(proc.stdout);
}

function runValidateEnvelope() {
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

module.exports = {
  runRpc,
  runValidateEnvelope,
};
