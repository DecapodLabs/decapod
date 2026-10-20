# Pinned Metal recurrence synchronization repair

Base source: guoqingbao/attention.rs
commit `f22e42cf27dd31efd2d1b838bd9e4cbf85bd2305`,
subdirectory `src/metal-kernels`, Git subtree
`b38deaeb0bc70244804a2467ffca45ba46b1ba3e`.
All 25 upstream source files are retained. Only `src/gdn.metal` is modified:
three token-loop end barriers protect shared-key readers before the next
iteration writes those keys. The public Rust API, dispatch geometry, shader
arithmetic, unrelated kernels and build script are unchanged.

Original GDN source SHA256:
`fc639b0d0fd583e63f1238cc4ab11201693c706a86fc61325fb715f13bac6928`.
Modified GDN source SHA256:
`3ba973f49a14c320b748c256a40ab08190ba9ae611319397d2279e4fdd4b9568`.
These are runtime-source identities. Candidate aa66c8e4 subsequently executed
the repaired primitive and two real-model reference fixtures successfully under
official v0.119.1; full scientific acceptance still failed readiness evaluation.
See `docs/vega-reference-contract.md` for measured scope and failure history.

The active pinned Qwen profile uses equal key/value head counts and calls the
variable-length recurrence with 128 key/value dimensions. Its 64-thread groups
can span multiple SIMD groups. Each token reads shared keys during the final
state/readout update; without an end barrier another SIMD group may start the
next token's writes early. The same repair covers the generic and GQA loops,
without asserting that other model modes have been verified.
See [Apple's synchronization explanation](https://developer.apple.com/videos/play/wwdc2020/10631/).

This subpackage's original Cargo manifest declares `MIT OR Apache-2.0`;
the upstream parent manifest declares MIT. No standalone license/NOTICE file
was present in the pinned source tree. Redistribution here uses the
subpackage's declared Apache-2.0 option with the official standard text from
https://www.apache.org/licenses/LICENSE-2.0.txt in `LICENSE-APACHE` (11,358 bytes,
SHA256 `cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30`).
This corrects the earlier copied vegaml-specific copyright appendix; that
appendix was not an upstream attention-rs notice. Existing source notices and manifest metadata are retained;
no new upstream copyright or independent review is asserted.

The Cargo patch applies only to this separate worker. Candle, candle-vllm and
attention-rs remain pinned; model/tokenizer/engine/calibration assets are
unchanged and external. Do not edit Cargo caches or silently synchronize this
directory to a floating upstream branch. Review an explicit source update and
rerun the native reference/regression suite before changing these identities.
