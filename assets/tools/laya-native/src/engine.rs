//! Bounded readiness question through Laya's maintained Rust front end.
//! No state truncation, long-window answer selection, router or fitting.
use crate::{Model, backbone_config};
use candle::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use laya::{
    Calibration, HfTokenizer, LayaConfig, LayaEngine, LayaOptions, LayaTokenizer, Question,
    QuestionSet, SequenceBuilder,
};
use serde_json::Value;
use std::{path::Path, time::Instant};

pub use crate::protocol::{Backend, Judgment, QUESTION, RUBRIC};
use crate::protocol::{IF_FALSE, IF_TRUE};
const QUESTION_ID: &str = "ready_for_pull_request";
type Error = Box<dyn std::error::Error + Send + Sync>;
type Result<T> = std::result::Result<T, Error>;

fn invalid(reason: &'static str) -> Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, reason).into()
}

pub struct Frontend {
    tokenizer: HfTokenizer,
    config: LayaConfig,
}

pub struct Prepared {
    state: String,
    question: Question,
    ids: Vec<u32>,
    markers: Vec<usize>,
}

impl Prepared {
    pub fn input_tokens(&self) -> usize {
        self.ids.len()
    }
}

impl Frontend {
    pub fn open(directory: &Path) -> Result<Self> {
        let directory = directory.join("onnx");
        Ok(Self {
            tokenizer: HfTokenizer::from_file_with_config(directory.join("tokenizer.json"), None)?,
            config: LayaConfig::load(directory.join("rl_agent_config.json"))?,
        })
    }

    pub fn prepare(&self, state: &str) -> Result<Prepared> {
        if state.is_empty() || state.len() > 32 * 1024 {
            return Err(invalid("laya_input_byte_limit"));
        }
        let mask = &self.tokenizer.special().mask_token;
        if !mask.is_empty() && state.contains(mask) {
            return Err(invalid("laya_literal_mask_token_is_unsupported"));
        }
        let question = Question::noul(QUESTION, IF_FALSE, IF_TRUE);
        let (empty, markers) = SequenceBuilder::build(
            &self.tokenizer,
            &Value::String(String::new()),
            &question,
            self.config.max_len,
            self.config.head_max_len,
            false,
        )?;
        // Also verify the fixed question/options: the SDK is permitted to
        // truncate its head independently of the state. Our reviewed rubric
        // must reach the model in full, including both explicit alternatives.
        let special = self.tokenizer.special();
        let mut complete = vec![special.cls_id];
        complete.extend(self.tokenizer.encode(&format!(
            "noul question: {}",
            SequenceBuilder::render_instructions(&question)
        ))?);
        complete.push(special.sep_id);
        let mut complete_markers = Vec::new();
        for option in SequenceBuilder::render_options(&question) {
            complete_markers.push(complete.len());
            complete.push(special.mask_id);
            complete.extend(self.tokenizer.encode(&format!(" {option}"))?);
        }
        complete.extend([special.sep_id, special.sep_id]);
        if empty != complete || markers != complete_markers {
            return Err(invalid("laya_rubric_or_options_were_truncated"));
        }
        let state_tokens = self.tokenizer.encode(state)?;
        if state_tokens.iter().any(|id| {
            [
                special.cls_id,
                special.sep_id,
                special.pad_id,
                special.mask_id,
            ]
            .contains(id)
        }) {
            return Err(invalid("laya_reserved_token_in_state"));
        }
        // Empty-state layout includes the final SEP. Every state token must
        // fit: the SDK's truncating/most-confident-window routes are not used.
        if empty
            .len()
            .checked_add(state_tokens.len())
            .is_none_or(|len| len > self.config.max_len)
        {
            return Err(invalid("laya_input_token_limit_without_truncation"));
        }
        let (ids, actual_markers) = SequenceBuilder::build(
            &self.tokenizer,
            &Value::String(state.into()),
            &question,
            self.config.max_len,
            self.config.head_max_len,
            false,
        )?;
        let mut expected = empty;
        let end = expected
            .pop()
            .ok_or_else(|| invalid("laya_empty_question_layout"))?;
        expected.extend(state_tokens);
        expected.push(end);
        if ids != expected || actual_markers != markers || markers.len() != 2 {
            return Err(invalid("laya_question_or_state_was_truncated"));
        }
        Ok(Prepared {
            state: state.into(),
            question,
            ids,
            markers,
        })
    }
}

pub enum Engine {
    Cpu(LayaEngine),
    Native {
        model: Model,
        device: Device,
        config: LayaConfig,
        backend: Backend,
    },
}

pub struct ScorerCapture {
    pub false_features: Vec<f32>,
    pub true_features: Vec<f32>,
    pub original_weights: Vec<f32>,
    pub original_bias: f32,
    pub logits: [f32; 2],
    pub temperature: f64,
    pub input_tokens: usize,
    pub backend: Backend,
}

impl Engine {
    pub fn replace_scorer(&mut self, weights: &[f32]) -> Result<()> {
        match self {
            Self::Native { model, .. } => Ok(model.replace_scorer(weights)?),
            Self::Cpu(_) => Err(invalid("fitted_scorer_requires_explicit_native_engine")),
        }
    }

    /// Captures the same production forward path without another input builder
    /// or training the encoder. The fitting tool consumes fitting rows only.
    pub fn capture_scorer(&self, prepared: &Prepared) -> Result<ScorerCapture> {
        let Self::Native {
            model,
            device,
            config,
            backend,
        } = self
        else {
            return Err(invalid("scorer_capture_requires_native_engine"));
        };
        let output = native_forward(model, device, prepared)?;
        let raw = output.logits.to_vec2::<f32>()?;
        let mut features = output.scoring_features.to_vec3::<f32>()?;
        if features.len() != 1 || features[0].len() != 2 {
            return Err(invalid("scorer_capture_expected_false_true_markers"));
        }
        let mut row = features.remove(0);
        let true_features = row.pop().unwrap();
        let false_features = row.pop().unwrap();
        Ok(ScorerCapture {
            false_features,
            true_features,
            original_weights: model.scorer_weights()?,
            original_bias: model.scorer_bias()?,
            logits: [raw[0][0], raw[0][1]],
            temperature: Calibration::resolve_temperature(
                config,
                prepared.question.question_type(),
                2,
            ),
            input_tokens: prepared.input_tokens(),
            backend: *backend,
        })
    }
    /// Caller verifies and freezes external manifest/runtime/files before load
    /// and rechecks custody after inference. This method is not installation.
    pub fn open(directory: &Path, backend: Backend) -> Result<Self> {
        let model_directory = directory.join("onnx");
        match backend {
            Backend::Cpu => Ok(Self::Cpu(LayaEngine::create(LayaOptions {
                model_directory: Some(model_directory),
                allow_download: false,
                intra_op_threads: Some(8),
                inter_op_threads: Some(1),
                ..LayaOptions::default()
            })?)),
            Backend::Metal | Backend::Cuda => Self::open_native(directory, backend),
        }
    }

    /// Same frozen native path also supports CPU when a reviewed fitted scorer
    /// is installed. It never discovers an overlay from the filesystem.
    pub fn open_native(directory: &Path, backend: Backend) -> Result<Self> {
        let model_directory = directory.join("onnx");
        let device = match backend {
            Backend::Metal => crate::device(true)?,
            Backend::Cuda => Device::new_cuda(0)?,
            Backend::Cpu => Device::Cpu,
        };
        let config = LayaConfig::load(model_directory.join("rl_agent_config.json"))?;
        let backbone = backbone_config(&std::fs::read(directory.join("encoder-config.json"))?)?;
        let raw: Value = serde_json::from_slice(&std::fs::read(
            model_directory.join("rl_agent_config.json"),
        )?)?;
        let layers = raw["head_layers"]
            .as_u64()
            .ok_or_else(|| invalid("laya_head_layers_missing"))?;
        let layers = usize::try_from(layers)?;
        // Independently verified immutable external checkpoint. No
        // generation API, model download or training runs here.
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(
                &[directory.join("model.safetensors")],
                DType::F32,
                &device,
            )?
        };
        let model = Model::load(vb, &backbone, layers, config.max_len)?;
        device.synchronize()?;
        Ok(Self::Native {
            model,
            device,
            config,
            backend,
        })
    }

    pub fn predict(&self, prepared: &Prepared) -> Result<Judgment> {
        let start = Instant::now();
        let (probability, action, backend, runtime) = match self {
            Self::Cpu(engine) => {
                let questions = QuestionSet::new().with(QUESTION_ID, prepared.question.clone())?;
                let result = engine.predict(prepared.state.clone(), &questions)?;
                let answer = result
                    .get(QUESTION_ID)
                    .ok_or_else(|| invalid("laya_missing_typed_answer"))?;
                let probability = answer
                    .as_noul()
                    .ok_or_else(|| invalid("laya_wrong_answer_type"))?
                    .probability;
                if result.usage().input_tokens as usize != prepared.ids.len() {
                    return Err(invalid("laya_executed_input_differs_from_preflight"));
                }
                (
                    probability,
                    answer.action().act_probability,
                    Backend::Cpu,
                    "onnxruntime-cpu",
                )
            }
            Self::Native {
                model,
                device,
                config,
                backend,
            } => {
                let output = native_forward(model, device, prepared)?;
                let logits = output.logits.to_vec2::<f32>()?;
                let actions = output.action_logits.to_vec2::<f32>()?;
                let temperature =
                    Calibration::resolve_temperature(config, prepared.question.question_type(), 2);
                let probabilities = Calibration::probabilities(&logits[0], 2, temperature);
                let actions = Calibration::softmax(&actions[0]);
                (
                    Calibration::round4(probabilities[1]),
                    Calibration::round4(actions[0]),
                    *backend,
                    match backend {
                        Backend::Metal => "candle-metal",
                        Backend::Cuda => "candle-cuda",
                        Backend::Cpu => "candle-cpu",
                    },
                )
            }
        };
        if ![probability, action]
            .iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
        {
            return Err(invalid("laya_invalid_probability"));
        }
        Ok(Judgment {
            true_probability: probability,
            action_probability: action,
            probability_quantum: 1e-4,
            input_tokens: prepared.ids.len(),
            backend,
            runtime: runtime.into(),
            inference_ms: start.elapsed().as_secs_f64() * 1000.0,
        })
    }
}

fn native_forward(model: &Model, device: &Device, prepared: &Prepared) -> Result<crate::Output> {
    let ids = Tensor::from_slice(&prepared.ids, (1, prepared.ids.len()), device)?;
    let mask = Tensor::ones(ids.shape(), DType::U32, device)?;
    let markers = prepared
        .markers
        .iter()
        .map(|index| u32::try_from(*index))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let positions = Tensor::from_slice(&markers, (1, markers.len()), device)?;
    let marker_mask = Tensor::ones(positions.shape(), DType::U32, device)?;
    let types = Tensor::new(&[2_u32], device)?;
    let output = model.forward(&ids, &mask, &positions, &marker_mask, &types)?;
    device.synchronize()?;
    Ok(output)
}
