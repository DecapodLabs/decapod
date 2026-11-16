# Decapod — six-second brand film

![Decapod animated showcase](generated/decapod-showcase.gif)

[Watch MP4 with original music](generated/decapod-showcase.mp4) · [Poster](generated/decapod-showcase-poster.png)

A 198-frame, 30-fps, **6.600-second**, **1920 × 1080** brand film for an AI-native governance kernel. Seven standalone cuts; no persistent UI or timeline.

The editorial direction is deliberately sparse: white canvas, high-contrast black editorial typography, coral crab, individual single-message scenes, no timeline or persistent UI. The crab hops between cuts. Original stereo synthesizer music is synchronized to the seven scene arrivals. The GIF is silent because GitHub Markdown has no native autoplay video/sound support. Click the animated GIF in the root README to view the MP4 with music.

The film opens with the human problem and immediately answers it: **Give it a metacognitive governance kernel.** This kernel is called by the AI agent, beneath the ordinary coding workflow. The human provides engineering intent and consequential authorization; the agent invokes Decapod as needed to investigate uncertainty, preserve state, coordinate bounded work, and check completion.

The seven full-screen cuts are: **Tired of babysitting your agents? → Give it a metacognitive governance kernel → Reflect. Refine. → Persistent state → Coordinated agents → Prove each commit → Decapod: Metacognition for AI agents.** The brand appears only after its category and capabilities are established.

The original kawaii crab mascot is authored as a standalone, fully transparent vector in [mascot.svg](mascot.svg). The renderer loads it once through CairoSVG, then animates frame-to-frame bouncing without crossing typography. There is no fixed timeline, terminal window, stock footage, licensed sample, or baked-in character background.

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

The renderer uses Pillow, NumPy and FFmpeg for frame-accurate motion and soundtrack synthesis. [VHS](https://github.com/charmbracelet/vhs) is useful for authentic terminal capture; this opening film is designed as motion graphics.

Current source and release details: [README](../../README.md) and [CHANGELOG](../../CHANGELOG.md). Historical receipts are bound to the exact subjects and assurance scopes they were produced for; the film does not imply universal forge or hosted independent-verifier enforcement.
