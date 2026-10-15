//! Independently reviewed native proof inputs, never resolved from floating refs.
//! These pins describe the base-text profile; adapters and numerical parity with
//! upstream golden activations remain outside this capability proof.
pub const MODEL_ID: &str = "nandakishorm/vega-08b-public-intents";
pub const MODEL_REVISION: &str = "42ac8d4fa9efb61f96e53d80962cf28f6dfe667e";
pub const BACKBONE_ID: &str = "Qwen/Qwen3.5-0.8B";
pub const BACKBONE_REVISION: &str = "2fc06364715b967f1860aea9cf38778875588b17";
pub const SOURCE_ID: &str = "NandhaKishorM/vegaml";
pub const SOURCE_REVISION: &str = "21a17c90624aa85cec60b1092fa72cb8b720f71a";
pub const PROFILE: &str = "vega-0.8b-base-text-v1";
pub const CONFIG_BYTES: &[u8] = include_bytes!("source_ci/contracts/vega-config.json.fixture");
pub const CANDLE_REVISION: &str = "b60efcf926dcfbdcad9e7c82e8a83679c526df20";
pub const BACKBONE_RUNTIME_REVISION: &str = "2c2c5293d21961d29dd73c2ff7b246b77d3eaf3f";
pub const ATTENTION_REVISION: &str = "5b4d645ea3e9b19fcc09ba1e4e0b8767b15df961";
pub const MODEL_FILES: &[(&str, u64, &str)] = &[
    (
        "vega_config.json",
        11236,
        "sha256:12f4f73784d58532ef244288c425df24944a8bda9c8414aa7c0981dfd2d88494",
    ),
    (
        "engine.safetensors",
        57433776,
        "sha256:09c4c5fe203cf166d419dfaa72d75b26da00f47f66bafbfd750b2263ee1ca66b",
    ),
    (
        "backbone/config.json",
        2907,
        "sha256:b90b86f35c8e6925ef74ee04d0e758f0a845c83a42089ad82bbaa948de9b4204",
    ),
    (
        "backbone/model.safetensors.index.json",
        50900,
        "sha256:d8a08838a613b025eb7952ed9db11696213e57e76a375661ef5c12f9dd5dcf4e",
    ),
    (
        "backbone/model.safetensors-00001-of-00001.safetensors",
        1746942600,
        "sha256:04b1c301231dd422b8860db31311ab2721511346a32cb1e079c4c4e5f1fe4696",
    ),
    (
        "backbone/tokenizer.json",
        12807982,
        "sha256:5f9e4d4901a92b997e463c1f46055088b6cca5ca61a6522d1b9f64c4bb81cb42",
    ),
    (
        "backbone/tokenizer_config.json",
        16709,
        "sha256:49e2b6e395f959f077f1e992b338919c0d4a9732fc6e613995e06557f843500c",
    ),
    (
        "backbone/LICENSE",
        11544,
        "sha256:bbedc3fda3305820b977265f01b8619d87570a6739de3a5582c3464840f1e57a",
    ),
    (
        "licenses/vega.LICENSE",
        10759,
        "sha256:5660238d37f7ae816966cc09d704bf1d2ed08dd16daf54c52a373d8ef38eef1c",
    ),
];
pub const FILE_ROLES: &[&str] = &[
    "engine_configuration_and_calibration",
    "engine_weights",
    "backbone_configuration",
    "backbone_weight_index",
    "backbone_weights",
    "tokenizer",
    "tokenizer_configuration",
    "backbone_license",
    "source_license",
];
