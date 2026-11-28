import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";

export type EnforcementBoundary =
  | "kernel_enforced"
  | "cooperative_hook"
  | "outside_native_enforcement";

export type TransportKind = "native_napi" | "cli_process";

export interface TransportMetadata {
  readonly kind: TransportKind;
  readonly fallbackActive: boolean;
  readonly platform: string;
  readonly arch: string;
  readonly reason: string;
  readonly binaryPath: string;
}

export type DecapodErrorCode =
  | "PROMPT_BLOCKED"
  | "DECISION_GATE"
  | "INVALID_SESSION"
  | "PROTECTED_WORKSPACE"
  | "PROOF_UNAVAILABLE"
  | "VALIDATION_FAILED"
  | "TIMEOUT"
  | "CANCELLED"
  | "INFRASTRUCTURE";

export interface DecapodError {
  readonly code: DecapodErrorCode;
  readonly message: string;
  readonly exitCode?: number;
}

export interface DecapodResult<T> {
  readonly ok: boolean;
  readonly value?: T;
  readonly error?: DecapodError;
  readonly transport: TransportMetadata;
}

export interface PromptProvenance {
  readonly turnId: string;
  readonly observedAtIso: string;
  readonly rawHumanPrompt: string;
  readonly rawPromptSha256: string;
  readonly sourceChannel: "harness_observed" | "issue_event" | "caller_unverified";
  readonly agentInterpretation?: string;
  /**
   * Automatic plan acceptance or model inference MUST NEVER be promoted into
   * authenticated human consent.
   */
  readonly humanApprovalAuthenticated: boolean;
}

export interface DecapodClientOptions {
  readonly binaryPath?: string;
  readonly nativeAddonPath?: string;
  readonly preferNative?: boolean;
  readonly cwd: string;
  readonly agentId: string;
  readonly sessionPassword?: string;
  readonly extraEnv?: Readonly<Record<string, string>>;
  readonly defaultTimeoutMs?: number;
}

export interface NativeAddonBinding {
  kernelVersion(): string;
  evalPrompt(prompt: string): string;
  validateDarkmatterSnapshotJson(snapshotJson: string): string;
  harnessCompatibilityMatrixJson(): string;
  generateHarnessPackageJson(
    harness: string,
    executablePath: string,
    repositoryRoot: string,
    minVersion: string,
  ): string;
  executeRpcSync(
    cwd: string,
    agentId: string,
    sessionPassword: string | null | undefined,
    op: string,
    paramsJson: string,
    requestId?: string | null,
  ): string;
  executeRpcAsync(
    cwd: string,
    agentId: string,
    sessionPassword: string | null | undefined,
    op: string,
    paramsJson: string,
    requestId?: string | null,
    signal?: AbortSignal | null,
  ): Promise<string>;
  cancelFlagSmokeTest(): boolean;
}

export interface InvocationOptions {
  readonly signal?: AbortSignal;
  readonly timeoutMs?: number;
}

export interface DecapodCommandOutput {
  readonly exitCode: number;
  readonly stdout: string;
  readonly stderr: string;
  readonly boundary: EnforcementBoundary;
  readonly transport: TransportMetadata;
}

export interface RpcEnvelope<T = unknown> {
  readonly ok: boolean;
  readonly op: string;
  readonly result?: T;
  readonly error?: string;
  readonly rawStdout: string;
  readonly rawStderr: string;
  readonly exitCode: number;
  readonly transport?: TransportMetadata;
}

export interface EvalPromptResult {
  readonly allowed: boolean;
  readonly status: string;
  readonly promptSha256?: string;
  readonly findings?: readonly unknown[];
  readonly action?: string;
  readonly raw: string;
  readonly transport?: TransportMetadata;
}

export interface GovernanceGateVerdict {
  readonly allowed: boolean;
  readonly stage: "preparation" | "pre_mutation" | "validation_admission";
  readonly boundary: EnforcementBoundary;
  readonly reason: string;
  readonly typedRemediation?: string;
  readonly provenanceDigest: string;
  readonly decisionGateRequired?: boolean;
  readonly transport?: TransportMetadata;
}

export interface DarkmatterEvidenceRef {
  readonly evidence_id: string;
  readonly kind: string;
  readonly status: string;
  readonly digest: string;
  readonly path: string;
}

export interface DarkmatterObligation {
  readonly obligation_id: string;
  readonly description: string;
  readonly status: string;
}

export interface DarkmatterReadonlySnapshot {
  readonly schema_version: string;
  readonly read_only: boolean;
  readonly mutation_endpoints_exposed: boolean;
  readonly ui_generated_proof_allowed: boolean;
  readonly repository: string;
  readonly head_commit: string;
  readonly human_requested_intent: string;
  readonly current_agent_objective: string;
  readonly task_id: string;
  readonly task_scope: readonly string[];
  readonly dependencies: readonly string[];
  readonly relevant_files: readonly string[];
  readonly governance_json_path: string;
  readonly completion_evidence: readonly DarkmatterEvidenceRef[];
  readonly outstanding_obligations: readonly DarkmatterObligation[];
}

export interface DecapodTransport {
  transportInfo(): TransportMetadata;
  evalPrompt(
    provenance: PromptProvenance,
    options?: InvocationOptions,
  ): Promise<EvalPromptResult>;
  orient(
    input: { readonly intent: string; readonly taskId?: string },
    options?: InvocationOptions,
  ): Promise<DecapodResult<Record<string, unknown>>>;
  resolveContext(
    input?: {
      readonly query?: string;
      readonly taskId?: string;
      readonly touchedPaths?: readonly string[];
    },
    options?: InvocationOptions,
  ): Promise<RpcEnvelope<Record<string, unknown>>>;
  preflight(
    input: { readonly operation: string },
    options?: InvocationOptions,
  ): Promise<DecapodResult<Record<string, unknown>>>;
  validateInference(
    input: { readonly intent: string; readonly result: string },
    options?: InvocationOptions,
  ): Promise<DecapodResult<Record<string, unknown>>>;
  validate(
    input?: { readonly store?: "user" | "repo" },
    options?: InvocationOptions,
  ): Promise<DecapodResult<Record<string, unknown>>>;
  rpc<T = unknown>(
    op: string,
    params?: Readonly<Record<string, unknown>>,
    options?: InvocationOptions,
  ): Promise<RpcEnvelope<T>>;
}

export const DARKMATTER_SNAPSHOT_SCHEMA_VERSION = "decapod.darkmatter-readonly-snapshot.v1";

export function sha256Digest(text: string): string {
  return `sha256:${createHash("sha256").update(text, "utf8").digest("hex")}`;
}

export function createPromptProvenance(input: {
  readonly turnId: string;
  readonly rawHumanPrompt: string;
  readonly sourceChannel: PromptProvenance["sourceChannel"];
  readonly agentInterpretation?: string;
  readonly observedAtIso?: string;
}): PromptProvenance {
  return Object.freeze({
    turnId: input.turnId,
    observedAtIso: input.observedAtIso ?? new Date().toISOString(),
    rawHumanPrompt: input.rawHumanPrompt,
    rawPromptSha256: sha256Digest(input.rawHumanPrompt),
    sourceChannel: input.sourceChannel,
    agentInterpretation: input.agentInterpretation,
    humanApprovalAuthenticated: false,
  });
}

function isRepoBoundedRelativePath(raw: string): boolean {
  if (!raw || raw.trim() === "" || raw.startsWith("/") || raw.startsWith("\\")) {
    return false;
  }
  return !raw.split("/").some((seg) => seg === "..");
}

/**
 * Pure read-only consumer contract validator for Darkmatter snapshots (#1711).
 * Mirrors `decapod::core::houseboat::validate_darkmatter_snapshot` so an
 * independent TypeScript reader fails closed on boundary or path escapes.
 */
export function validateDarkmatterSnapshot(
  snapshot: DarkmatterReadonlySnapshot,
): { readonly ok: boolean; readonly errors: readonly string[] } {
  const errors: string[] = [];
  if (snapshot.schema_version !== DARKMATTER_SNAPSHOT_SCHEMA_VERSION) {
    errors.push(
      `unsupported schema_version '${snapshot.schema_version}', expected '${DARKMATTER_SNAPSHOT_SCHEMA_VERSION}'`,
    );
  }
  if (!snapshot.read_only) {
    errors.push("DARKMATTER_BOUNDARY_VIOLATION: read_only must be true");
  }
  if (snapshot.mutation_endpoints_exposed) {
    errors.push("DARKMATTER_BOUNDARY_VIOLATION: mutation_endpoints_exposed must be false");
  }
  if (snapshot.ui_generated_proof_allowed) {
    errors.push("DARKMATTER_BOUNDARY_VIOLATION: ui_generated_proof_allowed must be false");
  }
  if (!snapshot.human_requested_intent || snapshot.human_requested_intent.trim() === "") {
    errors.push("human_requested_intent must be present and non-empty");
  }
  if (!snapshot.current_agent_objective || snapshot.current_agent_objective.trim() === "") {
    errors.push("current_agent_objective must be present and non-empty");
  }
  if (!snapshot.task_id || snapshot.task_id.trim() === "") {
    errors.push("task_id must be present and non-empty");
  }
  if (!snapshot.relevant_files || snapshot.relevant_files.length === 0) {
    errors.push("relevant_files must not be empty");
  }
  if (!isRepoBoundedRelativePath(snapshot.governance_json_path)) {
    errors.push(`PATH_ESCAPE: governance_json_path '${snapshot.governance_json_path}'`);
  }
  for (const path of snapshot.relevant_files ?? []) {
    if (!isRepoBoundedRelativePath(path)) {
      errors.push(`PATH_ESCAPE: relevant_file '${path}'`);
    }
  }
  if (!snapshot.completion_evidence || snapshot.completion_evidence.length === 0) {
    errors.push("completion_evidence must include at least one traceable record");
  }
  for (const ev of snapshot.completion_evidence ?? []) {
    if (!isRepoBoundedRelativePath(ev.path)) {
      errors.push(`PATH_ESCAPE: evidence path '${ev.path}'`);
    }
    if (!ev.digest || !ev.digest.startsWith("sha256:")) {
      errors.push(`EVIDENCE_DIGEST_INVALID: evidence '${ev.evidence_id}' must carry sha256:`);
    }
  }
  return Object.freeze({ ok: errors.length === 0, errors: Object.freeze(errors) });
}

export function classifyDecapodError(exitCode: number, stderr: string, stdout: string): DecapodError {
  const combined = `${stderr}\n${stdout}`.trim();
  const upper = combined.toUpperCase();
  let code: DecapodErrorCode = "INFRASTRUCTURE";
  if (upper.includes("DECISION_GATE") || upper.includes("DECISION GATE")) {
    code = "DECISION_GATE";
  } else if (upper.includes("SESSION") && (upper.includes("INVALID") || upper.includes("REQUIRED"))) {
    code = "INVALID_SESSION";
  } else if (upper.includes("PROTECTED_BRANCH") || upper.includes("WORKTREE")) {
    code = "PROTECTED_WORKSPACE";
  } else if (upper.includes("PROMPT") && upper.includes("BLOCK")) {
    code = "PROMPT_BLOCKED";
  } else if (upper.includes("PROOF") || upper.includes("UNAVAILABLE")) {
    code = "PROOF_UNAVAILABLE";
  } else if (exitCode !== 0) {
    code = "VALIDATION_FAILED";
  }
  return Object.freeze({
    code,
    message: combined || `Decapod exited with code ${exitCode}`,
    exitCode,
  });
}

export function resolveDefaultNativeAddonPath(): string | undefined {
  const filename = `decapod.${process.platform}-${process.arch}.node`;
  const candidates = [
    resolve(dirname(new URL(import.meta.url).pathname), "../native", filename),
    resolve(dirname(new URL(import.meta.url).pathname), "../../native", filename),
  ];
  return candidates.find((candidate) => existsSync(candidate));
}

function loadNativeAddon(nativeAddonPath?: string): NativeAddonBinding | undefined {
  if (!nativeAddonPath || !existsSync(nativeAddonPath)) {
    return undefined;
  }
  const req = createRequire(import.meta.url);
  return req(nativeAddonPath) as NativeAddonBinding;
}

/**
 * Probe whether an optional N-API prebuild exists; always reports explicit
 * `cli_process` fallback metadata when absent so hosts never silently downgrade.
 */
export function probeTransport(binaryPath: string, nativeAddonPath?: string): TransportMetadata {
  const hasNative = nativeAddonPath !== undefined && existsSync(nativeAddonPath);
  return Object.freeze({
    kind: hasNative ? "native_napi" : "cli_process",
    fallbackActive: !hasNative,
    platform: process.platform,
    arch: process.arch,
    reason: hasNative
      ? `native N-API addon detected at ${nativeAddonPath}`
      : "prebuilt N-API addon not installed; using verified Decapod CLI/RPC process carrier",
    binaryPath,
  });
}

/**
 * Library-safe Decapod client implementing `DecapodTransport`.
 *
 * Guarantees:
 * - Zero `process.exit` calls.
 * - Zero mutable global session state (every client instance carries its own
 *   explicit `cwd`, `agentId`, and `sessionPassword`).
 * - Full concurrency safety and `AbortSignal` cancellation per invocation.
 * - Explicit transport/fallback visibility (`transportInfo()`).
 */
export class DecapodClient implements DecapodTransport {
  private readonly binaryPath: string;
  private readonly cwd: string;
  private readonly agentId: string;
  private readonly sessionPassword?: string;
  private readonly extraEnv: Readonly<Record<string, string>>;
  private readonly defaultTimeoutMs: number;
  private readonly transportMeta: TransportMetadata;
  private readonly nativeAddon?: NativeAddonBinding;

  constructor(options: DecapodClientOptions) {
    if (!options.cwd || options.cwd.trim() === "") {
      throw new Error("DecapodClient requires a non-empty cwd");
    }
    if (!options.agentId || options.agentId.trim() === "") {
      throw new Error("DecapodClient requires a non-empty agentId");
    }
    this.binaryPath = options.binaryPath ?? "decapod";
    this.cwd = options.cwd;
    this.agentId = options.agentId;
    this.sessionPassword = options.sessionPassword;
    this.extraEnv = Object.freeze({ ...(options.extraEnv ?? {}) });
    this.defaultTimeoutMs = options.defaultTimeoutMs ?? 30_000;
    const resolvedAddon =
      options.nativeAddonPath ??
      (options.preferNative ? resolveDefaultNativeAddonPath() : undefined);
    this.nativeAddon = loadNativeAddon(resolvedAddon);
    this.transportMeta = probeTransport(
      this.binaryPath,
      this.nativeAddon ? resolvedAddon : options.nativeAddonPath,
    );
  }

  transportInfo(): TransportMetadata {
    return this.transportMeta;
  }

  private buildEnv(): Record<string, string | undefined> {
    const env: Record<string, string | undefined> = {
      ...process.env,
      ...this.extraEnv,
      DECAPOD_AGENT_ID: this.agentId,
    };
    if (this.sessionPassword !== undefined) {
      env.DECAPOD_SESSION_PASSWORD = this.sessionPassword;
    }
    return env;
  }

  exec(
    args: readonly string[],
    options: InvocationOptions & { readonly stdin?: string } = {},
  ): Promise<DecapodCommandOutput> {
    return new Promise((resolve, reject) => {
      if (options.signal?.aborted) {
        reject(new DOMException("Decapod invocation aborted before spawn", "AbortError"));
        return;
      }

      const child = spawn(this.binaryPath, [...args], {
        cwd: this.cwd,
        env: this.buildEnv(),
        stdio: ["pipe", "pipe", "pipe"],
      });

      const stdoutChunks: Buffer[] = [];
      const stderrChunks: Buffer[] = [];
      let settled = false;

      const finish = (fn: () => void) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        options.signal?.removeEventListener("abort", onAbort);
        fn();
      };

      const onAbort = () => {
        child.kill("SIGTERM");
        finish(() => reject(new DOMException("Decapod invocation cancelled", "AbortError")));
      };

      options.signal?.addEventListener("abort", onAbort, { once: true });

      const timeoutMs = options.timeoutMs ?? this.defaultTimeoutMs;
      const timer = setTimeout(() => {
        child.kill("SIGTERM");
        finish(() =>
          reject(new Error(`Decapod command timed out after ${timeoutMs}ms: ${args.join(" ")}`)),
        );
      }, timeoutMs);

      child.stdout.on("data", (chunk: Buffer) => stdoutChunks.push(chunk));
      child.stderr.on("data", (chunk: Buffer) => stderrChunks.push(chunk));

      child.on("error", (err) => {
        finish(() => reject(err));
      });

      child.on("close", (code) => {
        finish(() =>
          resolve({
            exitCode: code ?? 1,
            stdout: Buffer.concat(stdoutChunks).toString("utf8"),
            stderr: Buffer.concat(stderrChunks).toString("utf8"),
            boundary: "kernel_enforced",
            transport: this.transportMeta,
          }),
        );
      });

      if (options.stdin !== undefined) {
        child.stdin.write(options.stdin, "utf8");
      }
      child.stdin.end();
    });
  }

  private parseJsonResult(
    out: DecapodCommandOutput,
  ): DecapodResult<Record<string, unknown>> {
    let parsed: Record<string, unknown> | undefined;
    try {
      const candidate = JSON.parse(out.stdout) as unknown;
      if (candidate && typeof candidate === "object") {
        parsed = candidate as Record<string, unknown>;
      }
    } catch {
      parsed = undefined;
    }

    if (out.exitCode === 0 && parsed !== undefined) {
      return { ok: true, value: parsed, transport: this.transportMeta };
    }
    return {
      ok: false,
      value: parsed,
      error: classifyDecapodError(out.exitCode, out.stderr, out.stdout),
      transport: this.transportMeta,
    };
  }

  async rpc<T = unknown>(
    op: string,
    params: Readonly<Record<string, unknown>> = {},
    options: InvocationOptions = {},
  ): Promise<RpcEnvelope<T>> {
    if (this.nativeAddon) {
      if (options.signal?.aborted) {
        throw new DOMException("Decapod native invocation aborted before execution", "AbortError");
      }
      try {
        const rawStdout = await this.nativeAddon.executeRpcAsync(
          this.cwd,
          this.agentId,
          this.sessionPassword ?? null,
          op,
          JSON.stringify(params),
          null,
          options.signal ?? null,
        );
        const obj = JSON.parse(rawStdout) as Record<string, unknown>;
        const ok = obj.success !== false;
        return {
          ok,
          op,
          result: ("result" in obj ? obj.result : obj) as T,
          error: ok ? undefined : String(obj.error ?? rawStdout),
          rawStdout,
          rawStderr: "",
          exitCode: ok ? 0 : 1,
          transport: this.transportMeta,
        };
      } catch (err) {
        if (options.signal?.aborted) {
          throw new DOMException("Decapod native invocation cancelled", "AbortError");
        }
        const msg = err instanceof Error ? err.message : String(err);
        return {
          ok: false,
          op,
          error: msg,
          rawStdout: "",
          rawStderr: msg,
          exitCode: 1,
          transport: this.transportMeta,
        };
      }
    }
    const out = await this.exec(
      ["rpc", "--op", op, "--params", JSON.stringify(params)],
      options,
    );
    let parsed: unknown;
    try {
      parsed = JSON.parse(out.stdout);
    } catch {
      parsed = undefined;
    }

    if (out.exitCode === 0 && parsed !== undefined) {
      const obj = parsed as Record<string, unknown>;
      return {
        ok: true,
        op,
        result: ("result" in obj ? obj.result : parsed) as T,
        rawStdout: out.stdout,
        rawStderr: out.stderr,
        exitCode: out.exitCode,
        transport: this.transportMeta,
      };
    }

    return {
      ok: false,
      op,
      error: out.stderr.trim() || out.stdout.trim() || `RPC '${op}' failed with exit ${out.exitCode}`,
      rawStdout: out.stdout,
      rawStderr: out.stderr,
      exitCode: out.exitCode,
      transport: this.transportMeta,
    };
  }

  async evalPrompt(
    provenance: PromptProvenance,
    options: InvocationOptions = {},
  ): Promise<EvalPromptResult> {
    if (this.nativeAddon) {
      if (options.signal?.aborted) {
        throw new DOMException("Decapod native eval aborted before execution", "AbortError");
      }
      const raw = this.nativeAddon.evalPrompt(provenance.rawHumanPrompt);
      const parsed = JSON.parse(raw) as {
        status?: string;
        prompt_sha256?: string;
        findings?: readonly unknown[];
        action?: string;
      };
      return {
        allowed: parsed.status === "allow",
        status: parsed.status ?? "unknown",
        promptSha256: parsed.prompt_sha256,
        findings: parsed.findings,
        action: parsed.action,
        raw,
        transport: this.transportMeta,
      };
    }
    const out = await this.exec(["eval", "--stdin", "--format", "json"], {
      ...options,
      stdin: provenance.rawHumanPrompt,
    });
    if (out.stdout.trim() !== "") {
      try {
        const parsed = JSON.parse(out.stdout) as {
          status?: string;
          prompt_sha256?: string;
          findings?: readonly unknown[];
          action?: string;
        };
        if (typeof parsed.status === "string") {
          return {
            allowed: out.exitCode === 0 && parsed.status === "allow",
            status: parsed.status,
            promptSha256: parsed.prompt_sha256,
            findings: parsed.findings,
            action: parsed.action,
            raw: out.stdout,
            transport: this.transportMeta,
          };
        }
      } catch {
        // Fall through to exitCode / invalid_json handling below.
      }
    }
    if (out.exitCode !== 0) {
      return {
        allowed: false,
        status: "error",
        raw: out.stderr || out.stdout,
        transport: this.transportMeta,
      };
    }
    return {
      allowed: false,
      status: "invalid_json",
      raw: out.stdout,
      transport: this.transportMeta,
    };
  }

  async orient(
    input: { readonly intent: string; readonly taskId?: string },
    options: InvocationOptions = {},
  ): Promise<DecapodResult<Record<string, unknown>>> {
    const args = ["infer", "orientation", "--intent", input.intent, "--format", "json"];
    if (input.taskId) {
      args.push("--task-id", input.taskId);
    }
    const out = await this.exec(args, options);
    return this.parseJsonResult(out);
  }

  async resolveContext(
    input: {
      readonly query?: string;
      readonly taskId?: string;
      readonly touchedPaths?: readonly string[];
    } = {},
    options: InvocationOptions = {},
  ): Promise<RpcEnvelope<Record<string, unknown>>> {
    const params: Record<string, unknown> = {};
    if (input.query !== undefined) params.query = input.query;
    if (input.taskId !== undefined) params.task_id = input.taskId;
    if (input.touchedPaths !== undefined) params.touched_paths = [...input.touchedPaths];
    return this.rpc<Record<string, unknown>>("context.resolve", params, options);
  }

  async preflight(
    input: { readonly operation: string },
    options: InvocationOptions = {},
  ): Promise<DecapodResult<Record<string, unknown>>> {
    const out = await this.exec(
      ["context", "preflight", "--op", input.operation, "--format", "json"],
      options,
    );
    return this.parseJsonResult(out);
  }

  async validateInference(
    input: { readonly intent: string; readonly result: string },
    options: InvocationOptions = {},
  ): Promise<DecapodResult<Record<string, unknown>>> {
    const out = await this.exec(
      ["infer", "validate", "--intent", input.intent, "--result", input.result, "--format", "json"],
      options,
    );
    return this.parseJsonResult(out);
  }

  async validate(
    input: { readonly store?: "user" | "repo" } = {},
    options: InvocationOptions = {},
  ): Promise<DecapodResult<Record<string, unknown>>> {
    const args = ["validate", "--format", "json"];
    if (input.store) {
      args.push("--store", input.store);
    }
    const out = await this.exec(args, options);
    return this.parseJsonResult(out);
  }

  async dataCompact(
    input: { readonly dryRun?: boolean; readonly reclaimPages?: boolean } = {},
    options: InvocationOptions = {},
  ): Promise<DecapodResult<Record<string, unknown>>> {
    const args = ["data", "compact", "--format", "json"];
    if (input.dryRun) args.push("--dry-run");
    if (input.reclaimPages) args.push("--reclaim-pages");
    const out = await this.exec(args, options);
    return this.parseJsonResult(out);
  }

  async dbMaintenanceCapabilities(
    options: InvocationOptions = {},
  ): Promise<DecapodResult<Record<string, unknown>>> {
    const out = await this.exec(["data", "db", "capabilities"], options);
    return this.parseJsonResult(out);
  }

  async houseboatStatus(options: InvocationOptions = {}): Promise<Record<string, unknown>> {
    const out = await this.exec(["houseboat", "status"], options);
    if (out.exitCode !== 0) {
      throw new Error(`houseboat status failed (${out.exitCode}): ${out.stderr}`);
    }
    return JSON.parse(out.stdout) as Record<string, unknown>;
  }

  async verifyDarkmatterSnapshot(
    filePath: string,
    options: InvocationOptions = {},
  ): Promise<DecapodResult<Record<string, unknown>>> {
    const out = await this.exec(
      ["houseboat", "verify-darkmatter-snapshot", "--file", filePath],
      options,
    );
    return this.parseJsonResult(out);
  }
}
