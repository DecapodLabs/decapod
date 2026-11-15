# Decapod — six-second brand film

![Decapod animated showcase](generated/decapod-showcase.gif)

[Watch MP4 with original music](generated/decapod-showcase.mp4) · [Poster](generated/decapod-showcase-poster.png)

A 198-frame, 30-fps, **6.600-second**, **1920 × 1080** brand film for an AI-native governance kernel. Eight standalone cuts; no persistent UI or timeline.

The editorial direction is deliberately sparse: white canvas, high-contrast black editorial typography, coral crab, individual single-message scenes, no timeline or persistent UI. The crab hops between cuts. Original stereo synthesizer music is synchronized to scene arrivals. The GIF is silent because GitHub Markdown has no native autoplay video/sound support. Click the animated GIF in the root README to view the MP4 with music.

The opening shows the intended operating model: **a human has an idea, tells their coding agent, and the agent calls Decapod**. The viewer operates through the agent; Decapod runs behind the scenes. The remaining cuts introduce the agent's governance substrate: context and boundaries, parallel coordination, proof for the exact commit, and history that survives interruptions and handoffs. The closing card says **Built for AI agents**. This is motion-graphics explanation, not an execution recording or an attestation of hosted-cloud readiness.

## Render

Dependencies: Python 3, Pillow 11.3.0, NumPy 2.3.4, FFmpeg (libx264 + AAC + ffprobe) and Ubuntu **Lato** font package.

On Ubuntu:

```sh
sudo apt-get update -q && sudo apt-get install -y ffmpeg fonts-lato
python3 -m pip install 'Pillow==11.3.0' 'numpy==2.3.4'
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
