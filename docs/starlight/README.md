# Decapod public documentation

The canonical **human-facing** content remains in `../book/src/**/*.md`, including
`SUMMARY.md`. Starlight is a static presentation renderer for this content.
The standalone Rust kernel, its agent entrypoints and the **binary-embedded agent
contracts** are not generated from this site and remain authoritative for agents.

## Development

From `docs/starlight`:

```sh
npm install
npm run build
npm run dev
```

The prebuild step copies the canonical Markdown into ignored
`src/content/docs/` and rewrites known local links to
`/decapod/<page>/`. This directory and `content-inventory.json` are
generated; do not edit them. The build validates every canonical page, Pagefind
and the mascot/wordmark shell, and emits legacy `*.html` redirect aliases.

No runtime services, SaaS APIs, payment flows or agent execution contracts are
required. Deployment uses GitHub Pages static assets with the existing
`/decapod/` base path. Release acceptance requires checking the deployed
site and old deep links in addition to the build result.
