# Decapod — 6.58-second feature reel

![Decapod animated showcase](generated/decapod-showcase.gif)

[Watch MP4](generated/decapod-showcase.mp4) · [Poster](generated/decapod-showcase-poster.png)

A 158-frame, 24-fps, silent 1280×720 introduction to Decapod. Every cut is scripted and reproducible; an original small crab travels along the timeline as the message moves through **claim → bounded authority → reflection → proof → recovery → governed work**.

These are **animated product explanations**, not recordings of a particular successful CLI run. The terminal-style labels describe released concepts; they do not assert that an example commit was validated. For exact acceptance evidence, run Decapod's own validation against a specified candidate and its configured checks.

## Render locally

Requires Python 3, [Pillow](https://pypi.org/project/Pillow/), and [FFmpeg](https://ffmpeg.org/) with libx264/ffprobe support.

```sh
python3 -m pip install 'Pillow==11.3.0'
python3 assets/showcase/render.py
```

The renderer produces **three committed outputs** under `assets/showcase/generated/`:

- `decapod-showcase.mp4`: 1280×720 H.264, 24 fps, 6.583 seconds, silent, ready for X/LinkedIn.
- `decapod-showcase.gif`: 760 px-wide / 12 fps optimized loop, embeddable in GitHub Markdown.
- `decapod-showcase-poster.png`: the commit-proof scene for link cards and previews.

Frame count is the source of truth for duration: **158 / 24 = 6.583 seconds**. The render step checks the encoded MP4 via ffprobe and exits nonzero if the duration is outside `[6.0, 7.0)`. A generated asset is valid only if it matches this exact checked-in renderer.

## Repeatable repository automation

[Showcase render workflow](../../.github/workflows/showcase-render.yml) renders and checks the output on the dedicated showcase branch, then commits the generated GIF, MP4, and PNG to the same branch using the built-in GitHub token. After the PR merges, maintainers can run the renderer and commit changed outputs in a reviewed PR. The workflow never writes directly to the protected main branch.

The motion design uses Pillow and FFmpeg because the seven-second hero video requires precise vector/typography transitions and deterministic file rendering. [VHS](https://github.com/charmbracelet/vhs) remains suitable for longer **real terminal session** recordings; its terminal typing delays and capture backend add no value to this animated feature reel. No browser, heavyweight JavaScript video framework, SaaS video service, or proprietary asset is required.

## Product truth

Local Decapod runs offline with SQLite; cloud-backed shared authority is separate work with its own acceptance gates. This reel does not imply hosted-cloud conformance, a signed verifier independent of the operating-system user, or guaranteed publication eligibility for every project. Current source and release details: [README](../../README.md) and [CHANGELOG](../../CHANGELOG.md).
