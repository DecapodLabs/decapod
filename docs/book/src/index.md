<div class="decapod-docs-brand">
  <a href="https://decapodlabs.github.io/" aria-label="Decapod Labs company homepage"><img class="decapod-docs-wordmark logo-a" src="https://decapodlabs.github.io/assets/logo-a.png" alt="Decapod Labs" width="420" height="140" loading="eager"><img class="decapod-docs-wordmark logo-c" src="https://decapodlabs.github.io/assets/logo-c.png" alt="" width="420" height="140" loading="eager"></a>
  <span class="decapod-docs-label">DOCUMENTATION</span>
</div>

<div class="decapod-docs-hero">
  <div class="decapod-docs-hero-copy">
    <div class="decapod-docs-kicker">BRING YOUR OWN CODING AGENT · REPO-NATIVE GOVERNANCE KERNEL</div>
    <h1>HARNESS AI <em class="decapod-docs-metacognition">METACOGNITION.</em></h1>
    <p><strong>Coding agents invoke Decapod</strong> — across <strong>Codex, Claude Code, Cursor, Antigravity, and other harnesses</strong> — to bind human intent, coordinate shared custody and isolated workspaces, run decision-time context and reflection checks, and require exact-commit validation proof before completion.</p>
    <div class="decapod-docs-actions">
      <a class="decapod-docs-btn primary" href="quickstart.html">Get started <span aria-hidden="true">↗</span></a>
      <a class="decapod-docs-btn secondary" href="introduction.html">Understand the kernel <span aria-hidden="true">↗</span></a>
    </div>
  </div>
  <img class="decapod-docs-mascot" src="https://decapodlabs.github.io/assets/mascot.svg" width="520" height="440" alt="The original coral-orange kawaii Decapod crab mascot" loading="eager">
</div>

## Who invokes Decapod — and how agents coexist

Decapod is **not** a proprietary chat UI, model router, or background orchestrator. Humans talk to their preferred coding agents; **coding agents invoke Decapod** through its CLI and structured RPC boundary at decision and execution boundaries:

```text
Human (intent & judgment)
  └─▶ Coding Agent (Codex / Claude Code / Cursor / Antigravity / custom harness)
        └─▶ Decapod Kernel (CLI / `decapod rpc`: orientation, claims, workspaces, validation, proof)
              └─▶ Repository (`.decapod/`: plans, specs, custody, trajectory, commit-bound evidence)
```

- **Any compatible agent on the same repo:** One engineer can switch between two agents mid-task; multiple engineers can run different agents in parallel against the same repository and governance substrate.
- **No manual command choreography:** You run `decapod init` once per repository (local `.decapod/data/decapod.db` or `--backend cloud` for shared team task custody). Your coding agent handles `eval`, `session`, `todo`, `workspace`, `infer`, `govern`, and `validate`.
- **Decision-time metacognition, not magic:** Before editing, agents orient against recorded intent, policy, and bounded reflection challenges. When validation fails, they use kernel diagnostics to repair the branch and recheck the exact revised commit.

<div class="decapod-docs-video">
  <video controls autoplay muted loop playsinline preload="metadata" poster="https://decapodlabs.github.io/assets/decapod-poster.png" aria-label="Decapod product introduction, including persistent state, enforced boundaries, and commit-bound proof">
    <source src="https://decapodlabs.github.io/assets/decapod-showcase.mp4" type="video/mp4">
    <p><a href="https://decapodlabs.github.io/assets/decapod-showcase.mp4">View the Decapod introduction.</a></p>
  </video>
</div>

<div class="decapod-docs-grid">
  <a class="decapod-docs-tile tile-orange" href="mental-model.html"><span class="tile-mark">✳</span><strong>Decision-time custody</strong><span>Retain intent, claims, and unresolved proof across agents.</span></a>
  <a class="decapod-docs-tile tile-cream" href="workflows/multi-agent.html"><span class="tile-mark">⬡</span><strong>Isolated parallel work</strong><span>Coordinate leases and worktrees without hiding merge limits.</span></a>
  <a class="decapod-docs-tile tile-ink" href="concepts/proof.html"><span class="tile-mark">✓</span><strong>Exact-commit proof</strong><span>Gate completion on checks run against the actual candidate.</span></a>
</div>

<section class="decapod-docs-offerings" aria-labelledby="decapod-offerings-heading">
  <div class="decapod-offerings-head">
    <div>
      <div class="decapod-offerings-kicker">DEPLOYMENT OFFERINGS</div>
      <h2 id="decapod-offerings-heading">Decapod Local &amp; Decapod Cloud</h2>
    </div>
    <a class="decapod-offerings-compare" href="https://decapodlabs.github.io/offerings.html">Compare Local &amp; Cloud <span aria-hidden="true">↗</span></a>
  </div>
  <div class="decapod-offerings-grid">
    <article class="decapod-offering-card offering-local">
      <div class="decapod-offering-meta">
        <h3>Decapod Local</h3>
        <span class="decapod-offering-badge badge-free">Free · Offline · No signup</span>
      </div>
      <p>Run the full governance kernel in your repository using local SQLite and Git state. No account, hosted service, or network sign-in required.</p>
      <ul class="decapod-offering-points">
        <li><code>decapod init</code> (<code>backend = "local"</code>, default)</li>
        <li>Embedded Constitution, governed plans, isolated workspaces, and proof gates</li>
      </ul>
      <a class="decapod-offering-action" href="quickstart.html">Start locally <span aria-hidden="true">↗</span></a>
    </article>
    <article class="decapod-offering-card offering-cloud">
      <div class="decapod-offering-meta">
        <h3>Decapod Cloud</h3>
        <span class="decapod-offering-badge badge-soon">Coming soon</span>
      </div>
      <p>Planned hosted coordination for shared team task custody, repository-scoped access control, and cross-session multi-agent continuity.</p>
      <ul class="decapod-offering-points">
        <li>Repository-bound sessions and shared coordination (<code>backend = "cloud"</code>)</li>
        <li>Planned offerings overview only — no public pricing or active purchase route</li>
      </ul>
      <a class="decapod-offering-action secondary" href="https://decapodlabs.github.io/offerings.html">Explore planned Cloud <span aria-hidden="true">↗</span></a>
    </article>
  </div>
</section>

## Start with your existing agents

Install Decapod and initialize it once in your repository:

```sh
cargo install decapod && decapod init
```

Then open Codex, Claude Code, Cursor, Antigravity, or another coding harness and describe your engineering goal. Generated entrypoints (`AGENTS.md`, `CLAUDE.md`, `CODEX.md`, `GEMINI.md`) direct your agent to invoke Decapod while keeping custody and evidence in `.decapod/`.

[Quickstart](quickstart.md) · [BYOA & Interoperability](concepts/interoperability.md) · [Agent-First Architecture](concepts/agent-first.md) · [Configuration](configuration.md) · [CLI Reference](reference/cli.md) · [Project source](https://github.com/DecapodLabs/decapod)

## Explore

- **[Introduction & Kernel Model](introduction.md)** — See the Human → Agent → Kernel → Repository boundary and Local/Cloud parity.
- **[Mental Model](mental-model.md)** — Follow the metacognitive decision-time loop and what Decapod does not guarantee.
- **[BYOA & Interoperability](concepts/interoperability.md)** — Understand capability discovery, agent handoffs, and supported integration contracts.
- **[Single-Agent & Handoff Workflow](workflows/single-agent.md)** — Switch agents mid-repo and recover from failed verification on exact commits.
- **[Concurrent Multi-Agent Workflow](workflows/multi-agent.md)** — Coordinate parallel engineers with claims, leases, `todo fleet`, and worktrees.
- **[Embedded Constitution (Reference Layer)](concepts/constitution.md)** — Consult the global, PR-contributable engineering canon inside the governed loop.
