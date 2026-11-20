//! Published fixtures are selected explicitly; expected outputs never come
//! from the candidate. Keep both independently pinned profiles reproducible.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    io::{self, Read},
    path::Path,
};

pub struct Profile {
    pub weights_bytes: u64,
    pub weights_sha256: &'static str,
    pub max_len: usize,
    pub reference: Value,
    pub files: [(&'static str, u64, &'static str); 4],
}

impl Profile {
    pub fn verify(&self, directory: &Path) -> io::Result<()> {
        assert_eq!(self.reference["questions"].as_object().unwrap().len(), 4);
        let pins = self
            .files
            .iter()
            .map(|(name, size, digest)| (directory.join("onnx").join(name), *size, *digest))
            .chain(std::iter::once((
                directory.join("model.safetensors"),
                self.weights_bytes,
                self.weights_sha256,
            )));
        for (path, size, expected) in pins {
            let mut file = std::fs::File::open(path)?;
            assert_eq!(file.metadata()?.len(), size);
            let mut digest = Sha256::new();
            let mut buffer = [0; 65536];
            let mut count = 0;
            loop {
                let n = file.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                count += n as u64;
                assert!(count <= size);
                digest.update(&buffer[..n]);
            }
            assert_eq!(count, size);
            assert_eq!(format!("{:x}", digest.finalize()), expected);
        }
        let config: Value =
            serde_json::from_slice(&std::fs::read(directory.join("onnx/rl_agent_config.json"))?)?;
        assert_eq!(config["max_len"].as_u64().unwrap(), self.max_len as u64);
        Ok(())
    }
}

pub fn profile() -> Profile {
    match std::env::var("DECAPOD_LAYA_REFERENCE_PROFILE").as_deref() {
        Ok("typed-decisions") => Profile {
            weights_bytes: 842609220,
            weights_sha256: "4fa56de72383a9d3efa9cfa78955733c81b9fc8067a587ca4beb82c78107a24e",
            max_len: 1024,
            reference: serde_json::from_str(include_str!(
                "../fixtures/typed-decisions-quickstart.json"
            ))
            .unwrap(),
            files: [
                (
                    "encoder.onnx",
                    1582334685,
                    "14e9707c42a300667c5f195befc566d34c8a0fb5b2ea00b53d1c405559f8060e",
                ),
                (
                    "head.onnx",
                    106272385,
                    "212b39e45b99e4a5d866369b0757ea586e7a494ed004563624205a381e17d52a",
                ),
                (
                    "tokenizer.json",
                    3583228,
                    "6c8aaa9a542084f2457eab775d4eeb51f92a70c0fd9de28d5edb0ddec3c08d30",
                ),
                (
                    "rl_agent_config.json",
                    847,
                    "ebf0cd524d92342a6be5e48e9fca3d7c2babfb5a56ccd79d2171ef5d8c7f7be8",
                ),
            ],
        },
        Ok("multilingual") => Profile {
            weights_bytes: 643835514,
            weights_sha256: "9d628fd971b700382ac6f65920a86f149777b2e748e0c955fb3b19695aa8f204",
            max_len: 1024,
            reference: serde_json::from_str(include_str!(
                "../fixtures/multilingual-quickstart.json"
            ))
            .unwrap(),
            files: [
                (
                    "encoder.onnx",
                    1230282662,
                    "4bf4feb8de3ee17e8032cf0c2fc506131d2e3c458dbe3ea82898d4d2d06fb1a6",
                ),
                (
                    "head.onnx",
                    60101236,
                    "03f78ddf0bfaa86f7b24e4298070bf9ac0a03516e64e8441552ecc2db89e1b3c",
                ),
                (
                    "tokenizer.json",
                    34363188,
                    "609d8f4c067cd3950f88594c5a802616cea245823836ef5848ee4fc40aab5b6f",
                ),
                (
                    "rl_agent_config.json",
                    472,
                    "25061739243b617ad88d1219ba6f8a9c86c5881ca28df024fa2d9b3b2fcc30c6",
                ),
            ],
        },
        Ok("english") | Err(std::env::VarError::NotPresent) => Profile {
            weights_bytes: 842609210,
            weights_sha256: "891102d372688fc2a094dac56a384bc537b87c63f21f9f3dac0be2b7cbc8d86c",
            max_len: 512,
            reference: serde_json::from_str(include_str!("../fixtures/english-quickstart.json"))
                .unwrap(),
            files: [
                (
                    "encoder.onnx",
                    1582334685,
                    "a617f183bea68956cdacf80e970eee72865fea32a181a111be292614155e4b9b",
                ),
                (
                    "head.onnx",
                    106272385,
                    "275b74e1494dcb84a8aa4f486ba4b0d414a4143e85b9d9bb5f07af62ca70c6ee",
                ),
                (
                    "tokenizer.json",
                    3583228,
                    "6c8aaa9a542084f2457eab775d4eeb51f92a70c0fd9de28d5edb0ddec3c08d30",
                ),
                (
                    "rl_agent_config.json",
                    745,
                    "ae287b56bbcf5f8c4f4541ae9dfd00c914c4c48b940b8398c3058af37ba92bbd",
                ),
            ],
        },
        other => panic!("unsupported explicit reference profile: {other:?}"),
    }
}
