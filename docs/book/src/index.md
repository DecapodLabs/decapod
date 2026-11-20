<div class="decapod-docs-hero">
  <div class="decapod-docs-hero-copy">
    <div class="decapod-docs-kicker">AGENT-AGNOSTIC · DAEMONLESS · REPO-NATIVE</div>
    <h1>HARNESS AI <em class="decapod-docs-metacognition">METACOGNITION.</em></h1>
    <p>Decapod is an <strong>agent-agnostic, daemonless metacognitive governance kernel application</strong>. Codex, Claude Code, Cursor, Antigravity, and other compatible agents call it through CLI or structured RPC to orient decisions, claim tasks, isolate work, and validate exact commits before publication. Each call ends; governance state persists across sessions.</p>
    <div class="decapod-docs-actions">
      <a class="decapod-docs-btn primary" href="quickstart.html">Get started <span aria-hidden="true">↗</span></a>
      <a class="decapod-docs-btn secondary" href="introduction.html">Understand the kernel <span aria-hidden="true">↗</span></a>
    </div>
  </div>
  <img class="decapod-docs-mascot" src="https://decapodlabs.github.io/assets/mascot.svg" width="520" height="440" alt="The original coral-orange kawaii Decapod crab mascot" loading="eager">
</div>

## Who invokes Decapod — and how agents coexist

Engineers work in their preferred coding agent. The agent calls the Decapod application through its CLI or structured RPC interface at decision, workspace, and validation boundaries. Decapod runs, returns a result, and exits:

```text
Human (intent & judgment)
  └─▶ Coding Agent (Codex / Claude Code / Cursor / Antigravity / custom harness)
        └─▶ Decapod application / kernel (CLI / `decapod rpc`: orientation, claims, workspaces, validation, proof)
              └─▶ Repository (`.decapod/`: plans, specs, custody, trajectory, commit-bound evidence)
```

- **Any compatible agent on the same repo:** One engineer can switch between two agents mid-task; multiple engineers can run different agents in parallel against the same repository and governance substrate.
- **No manual command choreography:** You run `decapod init` once per repository (local `.decapod/data/decapod.db` or `--backend cloud` for shared team task custody). Your coding agent handles `eval`, `session`, `todo`, `workspace`, `infer`, `govern`, and `validate`.
- **Orientation and recovery:** Before editing, agents consult recorded intent, policy, and bounded reflection checks. When validation fails, they use diagnostics to repair the branch and revalidate the exact revised commit.

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
