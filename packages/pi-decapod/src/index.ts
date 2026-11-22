import {
  DecapodClient,
  createPromptProvenance,
  type EnforcementBoundary,
  type GovernanceGateVerdict,
  type PromptProvenance,
} from "../../../bindings/typescript/src/index.ts";

export interface PiSessionState {
  readonly sessionId: string;
  readonly modelId: string;
  readonly harnessId: string;
  readonly originalPrompt?: PromptProvenance;
  readonly promptHistory: readonly PromptProvenance[];
  readonly lastGateVerdict?: GovernanceGateVerdict;
}

export interface PiMutationContext {
  readonly targetPaths: readonly string[];
  readonly planConverged: boolean;
  readonly invokedThroughDecapodHook: boolean;
}

export interface PiDecapodExtension {
  readonly state: () => PiSessionState;
  onPromptReceived(input: {
    readonly turnId: string;
    readonly rawHumanPrompt: string;
    readonly agentInterpretation?: string;
  }): Promise<{ readonly provenance: PromptProvenance; readonly evalAllowed: boolean }>;
  switchModelOrHarness(nextModelId: string, nextHarnessId: string): PiSessionState;
  beforeMutation(ctx: PiMutationContext): Promise<GovernanceGateVerdict>;
  beforeValidationAndAdmission(branch: string): Promise<GovernanceGateVerdict>;
  classifyDirectHostWrite(invokedThroughDecapodHook: boolean): EnforcementBoundary;
}

/**
 * First-class Pi harness lifecycle extension (#1277).
 *
 * Design invariants:
 * - Preserves genuinely observed original + follow-up raw prompts separately
 *   from agent interpretation across model/harness replacement.
 * - Invokes the Decapod kernel contract at preparation, pre-mutation, and
 *   validation/admission without creating a second prompt ledger.
 * - Never translates automatic plan acceptance into authenticated human consent.
 * - Classifies unobserved direct host writes honestly as `outside_native_enforcement`.
 */
export function createPiDecapodExtension(options: {
  readonly sessionId: string;
  readonly modelId: string;
  readonly harnessId: string;
  readonly client: DecapodClient;
}): PiDecapodExtension {
  let modelId = options.modelId;
  let harnessId = options.harnessId;
  let originalPrompt: PromptProvenance | undefined;
  const promptHistory: PromptProvenance[] = [];
  let lastGateVerdict: GovernanceGateVerdict | undefined;

  const snapshotState = (): PiSessionState =>
    Object.freeze({
      sessionId: options.sessionId,
      modelId,
      harnessId,
      originalPrompt,
      promptHistory: Object.freeze([...promptHistory]),
      lastGateVerdict,
    });

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
      return { provenance, evalAllowed: evalResult.allowed };
    },

    switchModelOrHarness(nextModelId, nextHarnessId) {
      modelId = nextModelId;
      harnessId = nextHarnessId;
      return snapshotState();
    },

    classifyDirectHostWrite(invokedThroughDecapodHook) {
      return invokedThroughDecapodHook ? "cooperative_hook" : "outside_native_enforcement";
    },

    async beforeMutation(ctx) {
      const latest = promptHistory.at(-1);
      const provenanceDigest = latest?.rawPromptSha256 ?? "sha256:missing";
      if (!ctx.invokedThroughDecapodHook) {
        lastGateVerdict = {
          allowed: false,
          stage: "pre_mutation",
          boundary: "outside_native_enforcement",
          reason:
            "Direct host write bypassed Pi cooperative hook; kernel cannot claim host-side write blocking without tested permissions.",
          typedRemediation: "Route edits through governed Pi workspace tool with active Decapod claim.",
          provenanceDigest,
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
        };
        return lastGateVerdict;
      }
      lastGateVerdict = {
        allowed: true,
        stage: "pre_mutation",
        boundary: "cooperative_hook",
        reason: `Pre-mutation hook passed for ${ctx.targetPaths.length} target path(s); kernel validation remains required for admission.`,
        provenanceDigest,
      };
      return lastGateVerdict;
    },

    async beforeValidationAndAdmission(branch) {
      const latest = promptHistory.at(-1);
      const provenanceDigest = latest?.rawPromptSha256 ?? "sha256:missing";
      if (!branch || branch === "main" || branch === "master") {
        lastGateVerdict = {
          allowed: false,
          stage: "validation_admission",
          boundary: "kernel_enforced",
          reason: `Protected branch '${branch}' cannot be admitted directly.`,
          typedRemediation: "Use an isolated todo-scoped workspace branch and run decapod validate.",
          provenanceDigest,
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
      };
      return lastGateVerdict;
    },
  };
}
