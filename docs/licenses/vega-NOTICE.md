# vega native port notice

The Rust prompt, dynamics and calibration implementation derives from
NandhaKishorM/vegaml, revision 21a17c90624aa85cec60b1092fa72cb8b720f71a,
licensed under Apache License 2.0. The accompanying vega-APACHE-2.0.txt preserves
that license. The implementation has been modified into Rust with bounded
readiness input, strict tensor validation and fail-closed decision handling.

Source: https://github.com/NandhaKishorM/vegaml/tree/21a17c90624aa85cec60b1092fa72cb8b720f71a

The optional native worker consumes candle-vllm (MIT), its pinned Candle fork
(MIT OR Apache-2.0), and attention.rs (MIT). Their license texts and dependency
closure are retained by the locked source distribution/build. Model weights are
not embedded in this crate. Explicit setup records and retains the model/source
license metadata and Qwen Apache-2.0 license alongside immutable local files.
