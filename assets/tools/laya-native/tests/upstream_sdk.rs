//! Maintained Rust SDK over a pinned public ONNX conversion; not readiness proof.
mod support;
use laya::{
    HfTokenizer, LayaConfig, LayaEngine, LayaOptions, Question, QuestionSet, SequenceBuilder,
};
use serde_json::Value;

use std::{path::PathBuf, time::Instant};

fn questions(reference: &Value) -> Result<QuestionSet, Box<dyn std::error::Error>> {
    let mut questions = QuestionSet::new();
    for (id, item) in reference["questions"].as_object().unwrap() {
        let instructions = item["instructions"].clone();
        let criteria = &item["criteria"];
        let question = match item["type"].as_str().unwrap() {
            "choice" => Question::choice(
                instructions,
                criteria
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone())),
            )?,
            "score" => Question::score(instructions, criteria.as_array().unwrap().clone())?,
            "noul" => Question::noul(
                instructions,
                criteria["false"].clone(),
                criteria["true"].clone(),
            ),
            _ => unreachable!(),
        };
        questions.insert(id.clone(), question)?;
    }
    Ok(questions)
}

#[test]
#[ignore = "requires the pinned external tokenizer and configuration"]
fn tokenization_and_markers_match_published_rows() -> Result<(), Box<dyn std::error::Error>> {
    let directory = PathBuf::from(std::env::var("DECAPOD_LAYA_REFERENCE_MODEL")?).join("onnx");
    let tokenizer = HfTokenizer::from_file_with_config(directory.join("tokenizer.json"), None)?;
    let config = LayaConfig::load(directory.join("rl_agent_config.json"))?;
    let reference = support::profile().reference;
    let questions = questions(&reference)?;
    for row in reference["per_question"].as_array().unwrap() {
        let question = questions.get(row["id"].as_str().unwrap()).unwrap();
        let (ids, markers) = SequenceBuilder::build(
            &tokenizer,
            &reference["state"],
            question,
            config.max_len,
            config.head_max_len,
            false,
        )?;
        assert_eq!(serde_json::to_value(ids)?, row["input_ids"]);
        assert_eq!(serde_json::to_value(markers)?, row["markers"]);
    }
    Ok(())
}

#[test]
#[ignore = "requires externally installed, independently hashed public ONNX graphs"]
fn rust_sdk_matches_published_answers_without_python() -> Result<(), Box<dyn std::error::Error>> {
    let directory = PathBuf::from(std::env::var("DECAPOD_LAYA_REFERENCE_MODEL")?).join("onnx");
    support::profile().verify(directory.parent().unwrap())?;
    let reference = support::profile().reference;
    let start = Instant::now();
    let engine = LayaEngine::create(LayaOptions {
        model_directory: Some(directory),
        allow_download: false,
        intra_op_threads: Some(8),
        inter_op_threads: Some(1),
        ..LayaOptions::default()
    })?;
    let load_ms = start.elapsed().as_secs_f64() * 1000.0;
    let questions = questions(&reference)?;
    let start = Instant::now();
    let result = engine.predict(reference["state"].clone(), &questions)?;
    let inference_ms = start.elapsed().as_secs_f64() * 1000.0;
    let expected = &reference["result"]["answers"];
    for (id, actual) in result.iter() {
        let row = &expected[id];
        match row["type"].as_str().unwrap() {
            "choice" => {
                let answer = actual.as_choice().unwrap();
                assert_eq!(answer.choice, row["choice"].as_str().unwrap());
                for (label, value) in row["probabilities"].as_object().unwrap() {
                    assert!(
                        (answer.probability(label).unwrap() - value.as_f64().unwrap()).abs()
                            <= 1e-4
                    );
                }
            }
            "score" => assert!(
                (actual.as_score().unwrap().score - row["score"].as_f64().unwrap()).abs() <= 1e-4
            ),
            "noul" => assert!(
                (actual.as_noul().unwrap().probability - row["noul"].as_f64().unwrap()).abs()
                    <= 1e-4
            ),
            _ => unreachable!(),
        }
        assert!(
            (actual.action().act_probability - row["action"]["act_probability"].as_f64().unwrap())
                .abs()
                <= 1e-4
        );
    }
    assert_eq!(
        result.usage().input_tokens,
        reference["result"]["usage"]["input_tokens"]
            .as_u64()
            .unwrap() as u32
    );
    eprintln!(
        "backend=onnxruntime-cpu load_ms={load_ms:.3} inference_ms={inference_ms:.3}; four published questions, independent rounded answers <=1e-4"
    );
    Ok(())
}
