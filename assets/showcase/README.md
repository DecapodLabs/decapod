# Decapod — six-second brand film

![Decapod animated showcase](generated/decapod-showcase.gif)

[Watch MP4 with original music](generated/decapod-showcase.mp4) · [Poster](generated/decapod-showcase-poster.png)

A 198-frame, 30-fps, **6.600-second**, **1920 × 1080** brand film for an AI-native governance kernel. Seven standalone cuts; no persistent UI or timeline.

The editorial direction is deliberately sparse: white canvas, high-contrast typography, original kawaii coral crab, individual single-message scenes, and no persistent UI. Coral marks the semantic anchor of each screen. The crab clatters in, bounces between cuts, and exits after the URL. Original stereo synthesizer music is synchronized to the seven scene arrivals. The GIF is silent because GitHub Markdown has no native autoplay video/sound support. Click the animated GIF in the root README to view the MP4 with music.

The film answers a familiar problem directly: **“Tired of babysitting your agents? Give it a metacognitive governance kernel.”** The human initializes once per project and continues using their existing coding agents. The agent invokes Decapod behind the scenes for context, governed execution, durable state and proof.

The seven independent screens are: **Tired of babysitting your agents? → Give it a metacognitive governance kernel → Persistent state → Enforced boundaries → Commit-bound proof → Init once per project. Keep using your agents. → Decapod: Built for AI agents.**

The four practical outcomes are durable context and continuity, enforced limits on agent authority, proof tied to the actual commit, and unchanged human coding workflow after one-time project initialization. Metacognition is established in the opening and intentionally not repeated in the closing title. **Color grammar:** black carries the message; coral marks one chosen semantic anchor per screen and the final brand. The standalone kawaii crab clatters in from offscreen right, lands between independent scenes, and exits off the nearest edge after the URL is visible. All movements are deterministic and contained in the renderer.

**Metacognition** means Decapod helps the agent inspect project context, challenge assumptions and investigate missing information through bounded evidence-grounded reflection before proceeding or requiring a human decision. Decapod does not provide a model's intelligence. **Persistent state** means repository-owned intent, living specifications, plans, task custody, unresolved obligations, event history and validation evidence survive process, agent and harness changes. Local state uses SQLite and tracked governance material; optional cloud shared authority has its own acceptance contract. **Proof** refers to specific source candidates, required checks and currently applicable policy, never generalized correctness by assertion.

This film depicts the product's released and documented operating model. It is designed as original motion graphics, not fabricated CLI output or an attestation of unexecuted hosted-cloud, OS-isolation or independent-verifier guarantees.

## Render

Dependencies: Python 3, Pillow 11.3.0, NumPy 2.3.4, CairoSVG 2.7.1, FFmpeg (libx264 + AAC + ffprobe), Cairo and Ubuntu **Lato** fonts.

On Ubuntu:

```sh
sudo apt-get update -q && sudo apt-get install -y ffmpeg fonts-lato libcairo2
python3 -m pip install 'Pillow==11.3.0' 'numpy==2.3.4' 'CairoSVG==2.7.1'
python3 assets/showcase/render.py
```

The renderer emits these tracked assets:

- `generated/decapod-showcase.mp4`: 1080p H.264 + AAC stereo, 30 fps, 6.600 seconds
- `generated/decapod-showcase.gif`: 960 × 540, ~15 fps, silent, loops in GitHub README
- `generated/decapod-showcase-poster.png`: static 1920 × 1080 poster

The duration comes from exactly **198 frames at 30 fps**. The script checks the encoded output with ffprobe; GitHub Actions checks duration, audio/video streams, GIF frames, and artifacts. Source and generated files live together in this repository.

## Repeatable repository automation

[Showcase render workflow](../../.github/workflows/showcase-render.yml) renders and validates generated media on the dedicated showcase branch and commits outputs with GitHub Actions. The workflow does not update protected `master`. Contributors can change scenes, motion, lettering, colors, and score in the source script, review visual output, and produce an identical scripted sequence.

The renderer uses Pillow, NumPy and FFmpeg for frame-accurate motion and soundtrack synthesis. The GIF uses FFmpeg at 15fps from the same rendered video; this avoids a Pillow shared-palette bug on Ubuntu that collapsed the animation into one frame and prevented the workflow from committing new assets. [VHS](https://github.com/charmbracelet/vhs) is useful for authentic terminal capture; this opening film is designed as motion graphics.

Current source and release details: [README](../../README.md) and [CHANGELOG](../../CHANGELOG.md). Historical receipts are bound to the exact subjects and assurance scopes they were produced for; the film does not imply universal forge or hosted independent-verifier enforcement.
