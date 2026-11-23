import {
  DecapodClient,
  createPromptProvenance,
  type DecapodTransport,
  type EnforcementBoundary,
  type GovernanceGateVerdict,
  type PromptProvenance,
  type TransportMetadata,
} from "../../../bindings/typescript/src/index.ts";

export interface PiToolOutcome {
  readonly toolName: string;
  readonly ok: boolean;
  readonly decisionGateRequired: boolean;
  readonly summary: string;
  readonly recordedAtIso: string;
}

export interface PiSessionState {
  readonly sessionId: string;
  readonly modelId: string;
  readonly harnessId: string;
  readonly transport: TransportMetadata;
  readonly originalPrompt?: PromptProvenance;
  readonly promptHistory: readonly PromptProvenance[];
  readonly toolHistory: readonly PiToolOutcome[];
  readonly unresolvedConflicts: readonly string[];
  readonly decisionGatePending: boolean;
  readonly lastGateVerdict?: GovernanceGateVerdict;
}

export interface PiMutationContext {
  readonly targetPaths: readonly string[];
  readonly planConverged: boolean;
  readonly invokedThroughDecapodHook: boolean;
}

export interface PiContextResolveResult {
  readonly ok: boolean;
  readonly transport: TransportMetadata;
  readonly unresolvedConflicts: readonly string[];
  readonly decisionGateRequired: boolean;
  readonly context?: Record<string, unknown>;
  readonly error?: string;
}

export interface PiSettlementResult {
  readonly completionClaimAllowed: boolean;
  readonly verdict: GovernanceGateVerdict;
  readonly transport: TransportMetadata;
}

export interface PiDecapodExtension {
  readonly state: () => PiSessionState;
  onPromptReceived(input: {
    readonly turnId: string;
    readonly rawHumanPrompt: string;
    readonly agentInterpretation?: string;
  }): Promise<{
    readonly provenance: PromptProvenance;
    readonly evalAllowed: boolean;
    readonly transport: TransportMetadata;
  }>;
  onContextResolve(input?: {
    readonly query?: string;
    readonly taskId?: string;
    readonly touchedPaths?: readonly string[];
  }): Promise<PiContextResolveResult>;
  onToolResult(input: {
    readonly toolName: string;
    readonly ok: boolean;
    readonly outputSummary: string;
    readonly decisionGateRequired?: boolean;
  }): GovernanceGateVerdict;
  switchModelOrHarness(nextModelId: string, nextHarnessId: string): PiSessionState;
  onResume(input: {
    readonly nextModelId: string;
    readonly nextHarnessId: string;
    readonly expectedPromptSha256?: string;
  }): { readonly safeToResume: boolean; readonly reason: string; readonly state: PiSessionState };
  beforeMutation(ctx: PiMutationContext): Promise<GovernanceGateVerdict>;
  beforeValidationAndAdmission(branch: string): Promise<GovernanceGateVerdict>;
  onSettlement(input: {
    readonly branch: string;
    readonly proofPassed: boolean;
  }): Promise<PiSettlementResult>;
  classifyDirectHostWrite(invokedThroughDecapodHook: boolean): EnforcementBoundary;
}

/**
 * First-class Pi harness lifecycle extension (#1277).
 *
 * Design invariants:
 * - Preserves genuinely observed original + follow-up raw prompts separately
 *   from agent interpretation across model/harness replacement.
 * - Invokes the Decapod kernel contract at preparation, context resolution,
 *   tool result, pre-mutation, settlement, and resume without creating a
 *   second prompt ledger.
 * - Never translates automatic plan acceptance or tool output into
 *   authenticated human consent; Decision Gates remain human-owned stops.
 * - Classifies unobserved direct host writes honestly as `outside_native_enforcement`.
 * - Reports active CLI fallback transport explicitly rather than silently.
 */
export function createPiDecapodExtension(options: {
  readonly sessionId: string;
  readonly modelId: string;
  readonly harnessId: string;
  readonly client: DecapodClient | DecapodTransport;
}): PiDecapodExtension {
  let modelId = options.modelId;
  let harnessId = options.harnessId;
  let originalPrompt: PromptProvenance | undefined;
  const promptHistory: PromptProvenance[] = [];
  const toolHistory: PiToolOutcome[] = [];
  let unresolvedConflicts: string[] = [];
  let decisionGatePending = false;
  let lastGateVerdict: GovernanceGateVerdict | undefined;

  const snapshotState = (): PiSessionState =>
    Object.freeze({
      sessionId: options.sessionId,
      modelId,
      harnessId,
      transport: options.client.transportInfo(),
      originalPrompt,
      promptHistory: Object.freeze([...promptHistory]),
      toolHistory: Object.freeze([...toolHistory]),
      unresolvedConflicts: Object.freeze([...unresolvedConflicts]),
      decisionGatePending,
      lastGateVerdict,
    });

  const beforeValidationAndAdmission = async (branch: string): Promise<GovernanceGateVerdict> => {
    const latest = promptHistory.at(-1);
    const provenanceDigest = latest?.rawPromptSha256 ?? "sha256:missing";
    const transport = options.client.transportInfo();
    if (!branch || branch === "main" || branch === "master") {
      lastGateVerdict = {
        allowed: false,
        stage: "validation_admission",
        boundary: "kernel_enforced",
        reason: `Protected branch '${branch}' cannot be admitted directly.`,
        typedRemediation: "Use an isolated todo-scoped workspace branch and run decapod validate.",
        provenanceDigest,
        transport,
      };
      return lastGateVerdict;
    }
    const rpc = await options.client.rpc("workspace.status", {});
    lastGateVerdict = {
      allowed: rpc.ok,
      stage: "validation_admission",
      boundary: "kernel_enforced",
      reason: rpc.ok
        ? `Workspace gate checked for '${branch}'; run decapod validate for full proof.`
        : `Workspace gate refused '${branch}': ${rpc.error ?? "unknown error"}`,
      typedRemediation: rpc.ok ? undefined : "Resolve reported workspace blockers and rerun validate.",
      provenanceDigest,
      transport,
    };
    return lastGateVerdict;
  };

  return {
    state: snapshotState,

    async onPromptReceived(input) {
      const provenance = createPromptProvenance({
        turnId: input.turnId,
        rawHumanPrompt: input.rawHumanPrompt,
        sourceChannel: "harness_observed",
        agentInterpretation: input.agentInterpretation,
      });
      if (!originalPrompt) {
        originalPrompt = provenance;
      }
      promptHistory.push(provenance);
      const evalResult = await options.client.evalPrompt(provenance);
      return {
        provenance,
        evalAllowed: evalResult.allowed,
        transport: options.client.transportInfo(),
      };
    },

    async onContextResolve(input = {}) {
      const transport = options.client.transportInfo();
      const env = await options.client.resolveContext(input);
      const conflicts: string[] = [];
      let gateRequired = false;
      if (env.result && typeof env.result === "object") {
        const rawConflicts = env.result.conflicts;
        if (Array.isArray(rawConflicts)) {
          for (const entry of rawConflicts) {
            conflicts.push(typeof entry === "string" ? entry : JSON.stringify(entry));
          }
        }
        const gates = env.result.decision_gates;
        if (Array.isArray(gates) && gates.length > 0) {
          gateRequired = true;
        }
      }
      unresolvedConflicts = conflicts;
      decisionGatePending = decisionGatePending || gateRequired || conflicts.length > 0;
      return Object.freeze({
        ok: env.ok,
        transport,
        unresolvedConflicts: Object.freeze(conflicts),
        decisionGateRequired: gateRequired || conflicts.length > 0,
        context: env.result,
        error: env.error,
      });
    },

    onToolResult(input) {
      const latest = promptHistory.at(-1);
      const provenanceDigest = latest?.rawPromptSha256 ?? "sha256:missing";
      const gate = Boolean(input.decisionGateRequired);
      toolHistory.push(
        Object.freeze({
          toolName: input.toolName,
          ok: input.ok,
          decisionGateRequired: gate,
          summary: input.outputSummary,
          recordedAtIso: new Date().toISOString(),
        }),
      );
      if (gate) {
        decisionGatePending = true;
      }
      lastGateVerdict = {
        allowed: input.ok && !gate,
        stage: "pre_mutation",
        boundary: "cooperative_hook",
        reason: gate
          ? `Tool '${input.toolName}' surfaced a Decision Gate; stop for explicit human judgment.`
          : input.ok
            ? `Tool '${input.toolName}' recorded without promoting to human approval.`
            : `Tool '${input.toolName}' failed: ${input.outputSummary}`,
        typedRemediation: gate
          ? "Pause autonomous execution and request human decision before mutating."
          : undefined,
        provenanceDigest,
        decisionGateRequired: gate,
        transport: options.client.transportInfo(),
      };
      return lastGateVerdict;
    },

    switchModelOrHarness(nextModelId, nextHarnessId) {
      modelId = nextModelId;
      harnessId = nextHarnessId;
      return snapshotState();
    },

    onResume(input) {
      modelId = input.nextModelId;
      harnessId = input.nextHarnessId;
      if (!originalPrompt) {
        return {
          safeToResume: false,
          reason: "No recorded prompt provenance to resume from; capture human prompt first.",
          state: snapshotState(),
        };
      }
      if (
        input.expectedPromptSha256 &&
        input.expectedPromptSha256 !== originalPrompt.rawPromptSha256
      ) {
        return {
          safeToResume: false,
          reason: "Original prompt digest mismatch; refuse stale continuation.",
          state: snapshotState(),
        };
      }
      if (decisionGatePending) {
        return {
          safeToResume: false,
          reason: "Pending Decision Gate or unresolved conflict requires human resolution before resuming.",
          state: snapshotState(),
        };
      }
      return {
        safeToResume: true,
        reason: "Prompt custody reconstructed cleanly across model/harness switch.",
        state: snapshotState(),
      };
    },

    classifyDirectHostWrite(invokedThroughDecapodHook) {
      return invokedThroughDecapodHook ? "cooperative_hook" : "outside_native_enforcement";
    },

    async beforeMutation(ctx) {
      const latest = promptHistory.at(-1);
      const provenanceDigest = latest?.rawPromptSha256 ?? "sha256:missing";
      const transport = options.client.transportInfo();
      if (!ctx.invokedThroughDecapodHook) {
        lastGateVerdict = {
          allowed: false,
          stage: "pre_mutation",
          boundary: "outside_native_enforcement",
          reason:
            "Direct host write bypassed Pi cooperative hook; kernel cannot claim host-side write blocking without tested permissions.",
          typedRemediation: "Route edits through governed Pi workspace tool with active Decapod claim.",
          provenanceDigest,
          transport,
        };
        return lastGateVerdict;
      }
      if (!latest) {
        lastGateVerdict = {
          allowed: false,
          stage: "pre_mutation",
          boundary: "cooperative_hook",
          reason: "No harness-observed prompt provenance recorded before mutation.",
          typedRemediation: "Capture raw human prompt via onPromptReceived before mutating files.",
          provenanceDigest,
          transport,
        };
        return lastGateVerdict;
      }
      if (decisionGatePending) {
        lastGateVerdict = {
          allowed: false,
          stage: "pre_mutation",
          boundary: "cooperative_hook",
          reason: "Pending Decision Gate or unresolved conflict blocks autonomous mutation.",
          typedRemediation: "Obtain explicit human decision before continuing.",
          provenanceDigest,
          decisionGateRequired: true,
          transport,
        };
        return lastGateVerdict;
      }
      if (!ctx.planConverged) {
        lastGateVerdict = {
          allowed: false,
          stage: "pre_mutation",
          boundary: "cooperative_hook",
          reason:
            "Plan is not converged; model-only plan drafting does not constitute human approval.",
          typedRemediation: "Converge plan in .decapod/governance.json before editing source files.",
          provenanceDigest,
          transport,
        };
        return lastGateVerdict;
      }
      lastGateVerdict = {
        allowed: true,
        stage: "pre_mutation",
        boundary: "cooperative_hook",
        reason: `Pre-mutation hook passed for ${ctx.targetPaths.length} target path(s); kernel validation remains required for admission.`,
        provenanceDigest,
        transport,
      };
      return lastGateVerdict;
    },

    beforeValidationAndAdmission,

    async onSettlement(input) {
      const verdict = await beforeValidationAndAdmission(input.branch);
      const completionClaimAllowed = verdict.allowed && input.proofPassed && !decisionGatePending;
      return Object.freeze({
        completionClaimAllowed,
        verdict: completionClaimAllowed
          ? verdict
          : {
              ...verdict,
              allowed: false,
              reason: !input.proofPassed
                ? "Completion claim refused: independent repository proof has not passed."
                : decisionGatePending
                  ? "Completion claim refused: unresolved Decision Gate remains open."
                  : verdict.reason,
            },
        transport: options.client.transportInfo(),
      });
    },
  };
}
