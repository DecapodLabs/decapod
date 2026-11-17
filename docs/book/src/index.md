<div class="decapod-docs-brand">
  <a href="https://decapodlabs.github.io/" aria-label="Decapod Labs company homepage"><img class="decapod-docs-wordmark logo-a" src="https://decapodlabs.github.io/assets/logo-a.png" alt="Decapod Labs" width="420" height="140" loading="eager"><img class="decapod-docs-wordmark logo-c" src="https://decapodlabs.github.io/assets/logo-c.png" alt="" width="420" height="140" loading="eager"></a>
  <span class="decapod-docs-label">DOCUMENTATION</span>
</div>

<div class="decapod-docs-hero">
  <div class="decapod-docs-hero-copy">
    <div class="decapod-docs-kicker">CLOSED-LOOP METACOGNITIVE AGENT GOVERNANCE</div>
    <h1>HARNESS AI <em class="decapod-docs-metacognition">METACOGNITION.</em></h1>
    <p>A capable model needs a dependable execution loop. Agents invoke Decapod to query a shared Constitution of engineering golden paths, research, and software-business knowledge; retain recorded intent, ownership, and proof obligations across sessions; repair governed failures; and validate the actual work.</p>
    <div class="decapod-docs-actions">
      <a class="decapod-docs-btn primary" href="quickstart.html">Get started <span aria-hidden="true">↗</span></a>
      <a class="decapod-docs-btn secondary" href="introduction.html">Understand Decapod <span aria-hidden="true">↗</span></a>
    </div>
  </div>
  <img class="decapod-docs-mascot" src="https://decapodlabs.github.io/assets/mascot.svg" width="520" height="440" alt="The original coral-orange kawaii Decapod crab mascot" loading="eager">
</div>

<div class="decapod-docs-video">
  <video controls autoplay muted loop playsinline preload="metadata" poster="https://decapodlabs.github.io/assets/decapod-poster.png" aria-label="Decapod product introduction, including persistent state, enforced boundaries, and commit-bound proof">
    <source src="https://decapodlabs.github.io/assets/decapod-showcase.mp4" type="video/mp4">
    <p><a href="https://decapodlabs.github.io/assets/decapod-showcase.mp4">View the Decapod introduction.</a></p>
  </video>
</div>

<div class="decapod-docs-loop" aria-label="Durable state, agent feedback, and independent validation">
  <div class="decapod-docs-loop-stage">
    <span class="decapod-docs-loop-label">01 / DURABLE STATE</span>
    <strong>Keep the governing thread.</strong>
    <p>Recorded intent, scoped tasks, decisions, attempted corrections, and proof obligations remain inspectable when an agent session ends.</p>
  </div>
  <div class="decapod-docs-loop-stage decapod-docs-loop-feedback">
    <span class="decapod-docs-loop-label">02 / CORRECTIVE LOOP</span>
    <strong>Make failures actionable.</strong>
    <p>At supported governed boundaries, rejections expose the unsatisfied condition and available remediation. The agent revises, retries, and preserves what it learned.</p>
  </div>
  <div class="decapod-docs-loop-stage decapod-docs-loop-verify">
    <span class="decapod-docs-loop-label">03 / VALIDATION</span>
    <strong>Prove the candidate.</strong>
    <p>Required gates execute on the actual repository state. Evidence determines which obligations passed, failed, became stale, or remain unproven.</p>
  </div>
</div>

[Explore the closed-loop architecture](concepts/convergence.md) · [Understand proof-backed validation](concepts/proof.md)

<div class="decapod-docs-grid">
  <a class="decapod-docs-tile tile-orange" href="mental-model.html"><span class="tile-mark">✳</span><strong>Persistent state</strong><span>Keep context across sessions.</span></a>
  <a class="decapod-docs-tile tile-cream" href="workflows/workspace-isolation.html"><span class="tile-mark">⬡</span><strong>Enforced boundaries</strong><span>Work within accepted scope.</span></a>
  <a class="decapod-docs-tile tile-ink" href="concepts/proof.html"><span class="tile-mark">✓</span><strong>Commit-bound proof</strong><span>Know what was actually checked.</span></a>
</div>

## Start with your existing agents

Install and initialize in your project:

```sh
cargo binstall decapod && decapod init
```

Then keep using Codex, Claude Code, Cursor, or your preferred coding agent. Decapod operates through the agent, retaining context, custody and evidence in the repository.

[Quickstart](quickstart.md) · [Agent-first architecture](concepts/agent-first.md) · [Configuration](configuration.md) · [CLI reference](reference/cli.md) · [Project source](https://github.com/DecapodLabs/decapod)

## Explore

- **[Intent and decisions](concepts/intent.md)** — Understand how a human request becomes reviewable agent understanding.
- **[Agent workflows](workflows/single-agent.md)** — Follow an accepted change from planning to validation.
- **[Concurrent work](workflows/multi-agent.md)** — Keep ownership explicit when agents work in parallel.
- **[Proof and recovery](concepts/proof.md)** — Distinguish verified work from unsupported completion claims.
- **[Cold-context continuation](concepts/convergence.md#cold-context-continuation)** — What the next agent needs to resume work safely.
- **[Global Constitution and project overrides](concepts/constitution.md)** — Query shared engineering doctrine and research, then apply explicit repository-specific policy.
