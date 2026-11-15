# Decapod — six-second brand film

![Decapod animated showcase](generated/decapod-showcase.gif)

[Watch MP4 with original music](generated/decapod-showcase.mp4) · [Poster](generated/decapod-showcase-poster.png)

A 198-frame, 30-fps, **6.600-second**, **1920 × 1080** feature film for Decapod.

The editorial direction is deliberately sparse: white canvas, high-contrast black editorial typography, coral crab, individual single-message scenes, no timeline or persistent UI. The crab hops between cuts. Original stereo synthesizer music is synchronized to scene arrivals. The GIF is silent because GitHub Markdown has no native autoplay video/sound support. Click the animated GIF in the root README to view the MP4 with music.

The scenes introduce the shipped capabilities of Decapod: persistent task ownership, parallel isolated work, bounded evidence-grounded reflection, proof tied to source commits, and history that survives recovery and handoff. These are motion-graphics explanations, **not fabricated CLI recordings or attestations of successful hosted-cloud validation**.

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
