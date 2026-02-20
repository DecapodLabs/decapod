# .decapod - Decapod Project Metadata 🦀✨

Welcome to the control-plane directory for this repo.

[![Ko-fi](https://img.shields.io/badge/Support%20on-Ko--fi-ff5f5f?logo=ko-fi&logoColor=white)](https://ko-fi.com/decapodlabs)

> 💖 If Decapod saves you time, coffee helps keep the chaos contained.

## What This Folder Is 📦

This directory is managed by `decapod` and contains project-scoped control-plane state.

## Contents 🧭

- [`OVERRIDE.md`](OVERRIDE.md): ✅ **Edit this file** to override embedded constitution behavior for your project.
- [`data/`](data/): 🚫 **DO NOT TOUCH**. Persistent state (SQLite + event logs) used by agents and decapod.
- [`generated/`](generated/): 🚫 **DO NOT TOUCH**. Generated artifacts managed by decapod.

## Safety Notes 🔒

- Use [`OVERRIDE.md`](OVERRIDE.md) for customization, not direct DB/file edits.
- Treat [`data/`](data/) and [`generated/`](generated/) as system internals.
