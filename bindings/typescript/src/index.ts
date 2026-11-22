import { spawn } from "node:child_process";
import { createHash } from "node:crypto";

export type EnforcementBoundary =
  | "kernel_enforced"
  | "cooperative_hook"
  | "outside_native_enforcement";

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
  readonly cwd: string;
  readonly agentId: string;
  readonly sessionPassword?: string;
  readonly extraEnv?: Readonly<Record<string, string>>;
  readonly defaultTimeoutMs?: number;
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
}

export interface RpcEnvelope<T = unknown> {
  readonly ok: boolean;
  readonly op: string;
  readonly result?: T;
  readonly error?: string;
  readonly rawStdout: string;
  readonly rawStderr: string;
  readonly exitCode: number;
}

export interface GovernanceGateVerdict {
  readonly allowed: boolean;
  readonly stage: "preparation" | "pre_mutation" | "validation_admission";
  readonly boundary: EnforcementBoundary;
  readonly reason: string;
  readonly typedRemediation?: string;
  readonly provenanceDigest: string;
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
}

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

/**
 * Library-safe Decapod client.
 *
 * Guarantees:
 * - Zero `process.exit` calls.
 * - Zero mutable global session state (every client instance carries its own
 *   explicit `cwd`, `agentId`, and `sessionPassword`).
 * - Full concurrency safety and `AbortSignal` cancellation per invocation.
 */
export class DecapodClient {
  private readonly binaryPath: string;
  private readonly cwd: string;
  private readonly agentId: string;
  private readonly sessionPassword?: string;
  private readonly extraEnv: Readonly<Record<string, string>>;
  private readonly defaultTimeoutMs: number;

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
          }),
        );
      });

      if (options.stdin !== undefined) {
        child.stdin.write(options.stdin, "utf8");
      }
      child.stdin.end();
    });
  }

  async rpc<T = unknown>(
    op: string,
    params: Readonly<Record<string, unknown>> = {},
    options: InvocationOptions = {},
  ): Promise<RpcEnvelope<T>> {
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
      };
    }

    return {
      ok: false,
      op,
      error: out.stderr.trim() || out.stdout.trim() || `RPC '${op}' failed with exit ${out.exitCode}`,
      rawStdout: out.stdout,
      rawStderr: out.stderr,
      exitCode: out.exitCode,
    };
  }

  async evalPrompt(
    provenance: PromptProvenance,
    options: InvocationOptions = {},
  ): Promise<{ readonly allowed: boolean; readonly status: string; readonly raw: string }> {
    const out = await this.exec(["eval", "--stdin", "--format", "json"], {
      ...options,
      stdin: provenance.rawHumanPrompt,
    });
    if (out.exitCode !== 0) {
      return { allowed: false, status: "error", raw: out.stderr || out.stdout };
    }
    try {
      const parsed = JSON.parse(out.stdout) as { status?: string };
      return {
        allowed: parsed.status === "allow",
        status: parsed.status ?? "unknown",
        raw: out.stdout,
      };
    } catch {
      return { allowed: false, status: "invalid_json", raw: out.stdout };
    }
  }

  async houseboatStatus(options: InvocationOptions = {}): Promise<Record<string, unknown>> {
    const out = await this.exec(["houseboat", "status"], options);
    if (out.exitCode !== 0) {
      throw new Error(`houseboat status failed (${out.exitCode}): ${out.stderr}`);
    }
    return JSON.parse(out.stdout) as Record<string, unknown>;
  }
}
