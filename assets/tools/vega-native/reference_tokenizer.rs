//! Independent byte-level BPE implementation restricted to the pinned Qwen
//! tokenizer schema. Does not call tokenizers or the candidate prompt builder.
//! JSON/regex/Unicode-normalization dependencies are shared infrastructure.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use unicode_normalization::UnicodeNormalization;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub struct ReferenceTokenizer {
    vocab: BTreeMap<String, u32>,
    merges: BTreeMap<(String, String), usize>,
    special: Vec<(String, u32)>,
    split: fancy_regex::Regex,
    alphabet: [char; 256],
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_byte_bpe_merges_by_declared_rank() {
        let json = json!({"normalizer":{"type":"NFC"},"model":{"type":"BPE","ignore_merges":false,
            "byte_fallback":false,"vocab":{"h":1,"i":2,"hi":3},"merges":["h i"]},
            "added_tokens":[],"pre_tokenizer":{"pretokenizers":[{"pattern":{"Regex":".+"}}]}});
        assert_eq!(
            ReferenceTokenizer::new(&json)
                .unwrap()
                .encode("hi")
                .unwrap(),
            [3]
        );
        let mut unmerged = json;
        unmerged["model"]["merges"] = json!([]);
        assert_eq!(
            ReferenceTokenizer::new(&unmerged)
                .unwrap()
                .encode("hi")
                .unwrap(),
            [1, 2]
        );
        assert!(ReferenceTokenizer::new(&json!({"model":{"type":"WordPiece"}})).is_err());
    }
}
impl ReferenceTokenizer {
    pub fn new(value: &Value) -> Result<Self> {
        if value["normalizer"]["type"] != "NFC"
            || value["model"]["type"] != "BPE"
            || value["model"]["ignore_merges"] != false
            || value["model"]["byte_fallback"] != false
        {
            return Err("unsupported reference tokenizer schema".into());
        }
        let mut alphabet = ['\0'; 256];
        let mut next = 256_u32;
        for b in 0..=255_u8 {
            let printable = matches!(b,33..=126|161..=172|174..=255);
            alphabet[b as usize] = char::from_u32(if printable {
                u32::from(b)
            } else {
                let n = next;
                next += 1;
                n
            })
            .unwrap();
        }
        let vocab = value["model"]["vocab"]
            .as_object()
            .ok_or("missing reference vocabulary")?
            .iter()
            .map(|(key, id)| {
                Ok((
                    key.clone(),
                    u32::try_from(id.as_u64().ok_or("invalid token ID")?)?,
                ))
            })
            .collect::<Result<_>>()?;
        let merges = value["model"]["merges"]
            .as_array()
            .ok_or("missing merges")?
            .iter()
            .enumerate()
            .map(|(rank, merge)| {
                let (a, b) = merge
                    .as_str()
                    .ok_or("invalid merge")?
                    .split_once(' ')
                    .ok_or("invalid merge pair")?;
                Ok(((a.to_owned(), b.to_owned()), rank))
            })
            .collect::<Result<_>>()?;
        let special = value["added_tokens"]
            .as_array()
            .ok_or("missing special tokens")?
            .iter()
            .map(|token| {
                if token["lstrip"] != false
                    || token["rstrip"] != false
                    || token["single_word"] == true
                    || token["normalized"] != false
                {
                    return Err("unsupported special-token matching semantics".into());
                }
                Ok((
                    token["content"]
                        .as_str()
                        .ok_or("missing token content")?
                        .to_owned(),
                    u32::try_from(token["id"].as_u64().ok_or("missing token ID")?)?,
                ))
            })
            .collect::<Result<_>>()?;
        let regex = value["pre_tokenizer"]["pretokenizers"][0]["pattern"]["Regex"]
            .as_str()
            .ok_or("missing split regex")?;
        Ok(Self {
            vocab,
            merges,
            special,
            split: fancy_regex::Regex::new(regex)?,
            alphabet,
        })
    }
    fn plain(&self, text: &str, out: &mut Vec<u32>) -> Result<()> {
        let normalized = text.nfc().collect::<String>();
        let mut end = 0;
        for found in self.split.find_iter(&normalized) {
            let found = found?;
            if found.start() != end {
                return Err("reference regex left unmatched bytes".into());
            }
            end = found.end();
            let mut pieces = found
                .as_str()
                .bytes()
                .map(|b| self.alphabet[b as usize].to_string())
                .collect::<Vec<_>>();
            while pieces.len() > 1 {
                let next = pieces
                    .windows(2)
                    .enumerate()
                    .filter_map(|(i, p)| {
                        self.merges
                            .get(&(p[0].clone(), p[1].clone()))
                            .map(|rank| (*rank, i))
                    })
                    .min();
                let Some((_, index)) = next else { break };
                let right = pieces.remove(index + 1);
                pieces[index].push_str(&right);
            }
            for piece in pieces {
                out.push(
                    *self
                        .vocab
                        .get(&piece)
                        .ok_or("BPE piece missing from vocabulary")?,
                );
            }
        }
        if end != normalized.len() {
            return Err("reference regex did not consume input".into());
        }
        Ok(())
    }
    pub fn encode(&self, text: &str) -> Result<Vec<u32>> {
        if text.len() > 32 * 1024 {
            return Err("reference input byte limit".into());
        }
        let mut out = Vec::new();
        let mut remaining = text;
        while !remaining.is_empty() {
            let next = self
                .special
                .iter()
                .filter_map(|(token, id)| remaining.find(token).map(|offset| (offset, token, id)))
                .min_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.len().cmp(&a.1.len())));
            let Some((offset, token, id)) = next else {
                self.plain(remaining, &mut out)?;
                break;
            };
            self.plain(&remaining[..offset], &mut out)?;
            out.push(*id);
            remaining = &remaining[offset + token.len()..];
        }
        Ok(out)
    }
    pub fn prompt(&self, state: &str, question: &str, bos: Option<u32>) -> Result<Value> {
        let st = self.encode(state)?;
        let mut ids = bos.into_iter().collect::<Vec<_>>();
        let bos_len = ids.len();
        if st.len() < 4096 {
            ids.extend(self.encode(&format!(
                "Question to keep in mind while reading (boolean): {question}\n\n"
            ))?);
        }
        ids.extend(self.encode("Observation:\n")?);
        let start = ids.len();
        ids.extend(st);
        let end = ids.len();
        let state = if end == start {
            json!({"start":bos_len,"end":start})
        } else {
            json!({"start":start,"end":end})
        };
        let qstart = ids.len();
        ids.extend(self.encode(&format!(
            "\n\nMeasurement (boolean): {question}\nPossible outcomes:"
        ))?);
        let qend = ids.len();
        let mut opts = Vec::new();
        for text in [
            "\n* no - the statement does not hold",
            "\n* yes - the statement holds",
        ] {
            let start = ids.len();
            ids.extend(self.encode(text)?);
            opts.push(json!({"start":start,"end":ids.len()}));
        }
        ids.extend(self.encode("\nOutcome:")?);
        if ids.len() > 4096 {
            return Err("reference refuses truncation beyond 4096".into());
        }
        Ok(
            json!({"tokens":ids,"spans":{"state":state,"question":{"start":qstart,"end":qend},"options":opts,"last":ids.len()-1}}),
        )
    }
}
