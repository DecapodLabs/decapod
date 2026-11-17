<div class="decapod-docs-brand">
  <a href="https://decapodlabs.github.io/" aria-label="Decapod Labs company homepage"><img class="decapod-docs-wordmark logo-a" src="https://decapodlabs.github.io/assets/logo-a.png" alt="Decapod Labs" width="420" height="140" loading="eager"><img class="decapod-docs-wordmark logo-c" src="https://decapodlabs.github.io/assets/logo-c.png" alt="" width="420" height="140" loading="eager"></a>
  <span class="decapod-docs-label">DOCUMENTATION</span>
</div>

<div class="decapod-docs-hero">
  <div class="decapod-docs-hero-copy">
    <div class="decapod-docs-kicker">THE METACOGNITION GOVERNANCE KERNEL FOR AI AGENTS</div>
    <h1>HARNESS AI <em class="decapod-docs-metacognition">METACOGNITION.</em></h1>
    <p>Decapod gives coding agents persistent project state, enforceable boundaries, and proof tied to the work they deliver.</p>
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
- **[Constitution and overrides](concepts/constitution.md)** — Establish policy that stays with the project.
