<div class="decapod-docs-hero">
  <div class="decapod-docs-hero-copy">
    <div class="decapod-docs-kicker">DECAPOD DOCUMENTATION</div>
    <h1>HARNESS AI <em class="decapod-docs-metacognition">METACOGNITION.</em></h1>
    <p>Decapod is an <strong>agent-agnostic, daemonless governance kernel</strong>. Codex, Claude Code, Cursor, Antigravity, and custom harnesses call it via CLI or structured RPC to keep work bounded to explicit intent, isolated in Git worktrees, and gated by exact-commit proof.</p>
    <div class="decapod-docs-actions">
      <a class="decapod-docs-btn primary" href="quickstart.html">Start in your repo <span aria-hidden="true">→</span></a>
      <a class="decapod-docs-btn secondary" href="introduction.html">Architecture overview <span aria-hidden="true">→</span></a>
    </div>
  </div>
  <img class="decapod-docs-mascot" src="https://decapodlabs.ai/assets/mascot.svg" width="520" height="440" alt="Decapod crab mascot" loading="eager">
</div>

## Choose your path

<div class="decapod-docs-grid">
  <a class="decapod-docs-tile tile-orange" href="quickstart.html">
    <span class="tile-mark" aria-hidden="true">01</span>
    <strong>Start locally with your agent</strong>
    <span>Install the CLI, run <code>decapod init</code> once, and let your coding harness claim tasks, isolate worktrees, and validate commits.</span>
  </a>
  <a class="decapod-docs-tile tile-cream" href="reference/propodus.html">
    <span class="tile-mark" aria-hidden="true">02</span>
    <strong>Coordinate a team with Cloud</strong>
    <span>Share live task custody, repository-bound sessions, and organization seat controls across contributors on separate machines.</span>
  </a>
  <a class="decapod-docs-tile tile-ink" href="https://decapodlabs.ai/proof.html">
    <span class="tile-mark" aria-hidden="true">03</span>
    <strong>Inspect recorded proof</strong>
    <span>Review observed autonomous validation runs, stale-evidence invalidation, and exact-commit gate receipts before publication.</span>
  </a>
</div>

<section class="decapod-docs-offerings" aria-labelledby="decapod-offerings-heading">
  <div class="decapod-offerings-head">
    <div>
      <div class="decapod-offerings-kicker">DEPLOYMENT OFFERINGS</div>
      <h2 id="decapod-offerings-heading">Decapod Local &amp; Decapod Cloud</h2>
    </div>
    <a class="decapod-offerings-compare" href="https://decapodlabs.ai/pricing">Compare Local &amp; Cloud <span aria-hidden="true">↗</span></a>
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
        <li>Embedded Constitution, governed plans, isolated worktrees, and proof gates</li>
      </ul>
      <a class="decapod-offering-action" href="quickstart.html">Start locally <span aria-hidden="true">↗</span></a>
    </article>
    <article class="decapod-offering-card offering-cloud">
      <div class="decapod-offering-meta">
        <h3>Decapod Cloud</h3>
        <span class="decapod-offering-badge badge-cloud">$50/contributor/repo/mo · $0 surprise overages</span>
      </div>
      <p>Hosted team task coordination, repository-bound sessions, and organization seat management with predictable per-repo pricing and multi-repo reservations.</p>
      <ul class="decapod-offering-points">
        <li>Shared repository custody (<code>backend = "cloud"</code>)</li>
        <li>Manage seats, repo enrollment, and billing in <a href="https://decapodlabs.ai/admin">Organization Admin</a></li>
      </ul>
      <div class="decapod-offering-actions">
        <a class="decapod-offering-action" href="https://decapodlabs.ai/pricing">View Cloud pricing <span aria-hidden="true">↗</span></a>
        <a class="decapod-offering-action secondary" href="https://decapodlabs.ai/signup">Sign up <span aria-hidden="true">↗</span></a>
        <a class="decapod-offering-action secondary" href="https://decapodlabs.ai/admin">Organization Admin <span aria-hidden="true">↗</span></a>
      </div>
    </article>
  </div>
</section>

## Quick install

Install once with `cargo-binstall` (or build from source via `cargo install decapod --locked`) and initialize your repository:

```sh
cargo binstall decapod && decapod init
```

Then open Codex, Claude Code, Cursor, or Antigravity and describe your task. Generated entrypoints (`AGENTS.md`, `CLAUDE.md`, `CODEX.md`, `GEMINI.md`) direct your agent to invoke Decapod at decision, workspace, and validation boundaries. See [Quickstart](quickstart.md) for prerequisites, agent boundary details, and your first governed result walkthrough.

## Core documentation

- **[Quickstart & First Governed Result](quickstart.md)** — Install, initialize once, and follow a complete candidate commit from claim through validation.
- **[Mental Model](mental-model.md)** — How the Human → Agent → Kernel → Repository boundary works and what Decapod does not guarantee.
- **[Proof & Verification Gates](concepts/proof.md)** — Exact-commit receipts, stale-evidence invalidation, and failure recovery.
- **[Single-Agent & Handoff Workflow](workflows/single-agent.md)** — Switch coding harnesses mid-task without stranding context.
- **[Concurrent Multi-Agent Workflow](workflows/multi-agent.md)** — Coordinate parallel work with leases, `todo fleet`, and isolated worktrees.
- **[CLI Reference](reference/cli.md)** & **[Configuration](configuration.md)** — Command contracts, `.decapod/config.toml`, and legacy upgrade behavior.
