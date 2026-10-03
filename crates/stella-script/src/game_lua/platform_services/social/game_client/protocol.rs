//! GameClient JSON and ScoreBody wire format recovered from Purple 1.1.6.
//! 1006A1778, 1006A0878, 100561768/10056199C, 10069694C/100697568.

use aes::{
    Aes128,
    cipher::{BlockModeEncrypt, KeyIvInit, block_padding::Pkcs7},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fmt::Write as _};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Score {
    pub(super) account_id: String,
    pub(super) level: String,
    pub(super) points: i64,
    properties: BTreeMap<String, String>,
}

impl Score {
    pub(super) fn new(level: String, points: f32) -> Self {
        Self {
            account_id: String::new(),
            level,
            // FCVTZS X1,S8 at 1000C0384: truncate, saturate, NaN -> zero.
            points: points as i64,
            properties: BTreeMap::new(),
        }
    }

    fn from_cache(text: &str) -> Result<Self, ()> {
        let value: Value = serde_json::from_str(text).map_err(|_| ())?;
        let mut properties = BTreeMap::new();
        for (key, value) in value
            .get("properties")
            .and_then(Value::as_object)
            .ok_or(())?
        {
            properties.insert(key.clone(), value.as_str().ok_or(())?.to_owned());
        }
        Ok(Self {
            account_id: required_string(&value, "accountId")?.to_owned(),
            level: required_string(&value, "level")?.to_owned(),
            points: integer(value.get("points").ok_or(())?)?,
            properties,
        })
    }

    fn cache_string(&self) -> String {
        compact(&json!({
            "accountId": self.account_id, "level": self.level,
            "points": self.points, "properties": self.properties,
        }))
    }

    pub(super) fn submission(&self) -> String {
        let mut score = serde_json::Map::new();
        score.insert("points".to_owned(), self.points.into());
        // Native properties are strings and are assigned after points, even
        // when a restored record contains a property named "points".
        for (key, value) in &self.properties {
            score.insert(key.clone(), value.clone().into());
        }
        compact(&json!({"level": self.level, "score": score}))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingScore {
    pub(super) transaction_id: i64,
    pub(super) score: Score,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CachedScore {
    leaderboard_id: String,
    rank: i64,
    score: Score,
}

#[derive(Debug, Default)]
pub(super) struct Cache {
    pub(super) transaction_id: i64,
    pub(super) pending: Vec<PendingScore>,
    cached: BTreeMap<String, CachedScore>,
}

impl Cache {
    pub(super) fn load(text: &str) -> Self {
        // The unlisted exception region 100694798..100694968 catches JSON
        // errors and resets ALL three members, including partially read data.
        Self::parse(text).unwrap_or_default()
    }

    fn parse(text: &str) -> Result<Self, ()> {
        let root: Value = serde_json::from_str(text).map_err(|_| ())?;
        let transaction_id = integer(root.get("transactionId").ok_or(())?)?;
        let mut pending = Vec::new();
        for row in required_array(&root, "scoresToSend")? {
            pending.push(PendingScore {
                transaction_id: integer(row.get("transactionId").ok_or(())?)? as i32 as i64,
                score: Score::from_cache(required_string(row, "score")?)?,
            });
        }
        let mut cached = BTreeMap::new();
        for row in required_array(&root, "cachedScores")? {
            let score = Score::from_cache(required_string(row, "score")?)?;
            let leaderboard_id = required_string(row, "leaderBoardId")?.to_owned();
            let rank = i64::from(integer(row.get("rank").ok_or(())?)? as u32);
            cached
                .entry(format!("{leaderboard_id}-{}", score.level))
                .or_insert(CachedScore {
                    leaderboard_id,
                    rank,
                    score,
                });
        }
        Ok(Self {
            transaction_id,
            pending,
            cached,
        })
    }

    pub(super) fn save(&self, transaction_id: i64, pending: &[PendingScore]) -> String {
        let pending: Vec<_> = pending
            .iter()
            .map(|row| {
                json!({
                    "transactionId": row.transaction_id, "score": row.score.cache_string(),
                })
            })
            .collect();
        let cached: Vec<_> = self
            .cached
            .values()
            .map(|row| {
                json!({
                    "leaderBoardId": row.leaderboard_id, "rank": row.rank,
                    "score": row.score.cache_string(),
                })
            })
            .collect();
        compact(
            &json!({ "transactionId": transaction_id, "scoresToSend": pending, "cachedScores": cached }),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LeaderboardRow {
    pub(super) account_id: String,
    pub(super) points: i64,
    pub(super) rank: i64,
}

pub(super) fn leaderboard(text: &str) -> Result<Vec<LeaderboardRow>, ()> {
    let root: Value = serde_json::from_str(text).map_err(|_| ())?;
    required_array(&root, "scores")?
        .iter()
        .map(|row| {
            let mut points = -1;
            let mut rank = -1;
            if let Some(score) = row.get("score") {
                let score = score.as_object().ok_or(())?;
                if let Some(value) = score.get("points") {
                    points = integer(value)?;
                }
            }
            if let Some(ranking) = row.get("ranking") {
                rank =
                    integer(ranking.as_object().ok_or(())?.get("rank").ok_or(())?)? as i32 as i64;
            }
            Ok(LeaderboardRow {
                account_id: required_string(row, "accountId")?.to_owned(),
                points,
                rank,
            })
        })
        .collect()
}

fn required_array<'a>(root: &'a Value, key: &str) -> Result<&'a [Value], ()> {
    root.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or(())
}

fn required_string<'a>(root: &'a Value, key: &str) -> Result<&'a str, ()> {
    root.get(key).and_then(Value::as_str).ok_or(())
}

fn integer(value: &Value) -> Result<i64, ()> {
    if let Some(value) = value.as_i64() {
        return Ok(value);
    }
    value.as_f64().map(|number| number as i64).ok_or(())
}

/// ScoreBody captures the current access string BEFORE common POST. A first
/// 401 changes only the headers; it does not rebuild this encrypted body.
pub(in crate::game_lua::platform_services) fn encrypt(access: &str, plaintext: &str) -> String {
    let digest = crate::game_lua::platform::sha1_digest(access.as_bytes());
    let key: &[u8; 16] = digest[..16]
        .try_into()
        .expect("SHA1 contains sixteen key bytes");
    let cipher = cbc::Encryptor::<Aes128>::new(key.into(), (&[0_u8; 16]).into())
        .encrypt_padded_vec::<Pkcs7>(plaintext.as_bytes());
    STANDARD.encode(cipher)
}

/// Native util::JSONWriter: sorted object keys, %.16g numbers, escaped slash,
/// uppercase UTF-16 escapes, and no whitespace or trailing newline.
pub(super) fn compact(value: &Value) -> String {
    let mut output = String::new();
    write_value(&mut output, value);
    output
}

fn write_value(output: &mut String, value: &Value) {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        Value::Number(value) => {
            output.push_str(&general16(value.as_f64().expect("finite JSON number")))
        }
        Value::String(value) => write_string(output, value),
        Value::Array(values) => {
            output.push('[');
            for (i, value) in values.iter().enumerate() {
                if i != 0 {
                    output.push(',');
                }
                write_value(output, value);
            }
            output.push(']');
        }
        Value::Object(values) => {
            output.push('{');
            for (i, (key, value)) in values
                .iter()
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .enumerate()
            {
                if i != 0 {
                    output.push(',');
                }
                write_string(output, key);
                output.push(':');
                write_value(output, value);
            }
            output.push('}');
        }
    }
}

fn write_string(output: &mut String, value: &str) {
    output.push('"');
    for unit in value.encode_utf16() {
        match unit {
            8 => output.push_str("\\b"),
            9 => output.push_str("\\t"),
            10 => output.push_str("\\n"),
            12 => output.push_str("\\f"),
            13 => output.push_str("\\r"),
            34 => output.push_str("\\\""),
            47 => output.push_str("\\/"),
            92 => output.push_str("\\\\"),
            32..=126 => output.push(char::from_u32(u32::from(unit)).unwrap()),
            _ => {
                write!(output, "\\u{unit:04X}").expect("String writes cannot fail");
            }
        }
    }
    output.push('"');
}

fn general16(value: f64) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() { "-0" } else { "0" }.to_owned();
    }
    // Rust's scientific rounding supplies the same sixteen significant
    // digits. Select fixed/scientific notation AFTER rounding, as printf does.
    let scientific = format!("{:.15e}", value.abs());
    let (mantissa, exponent) = scientific.split_once('e').unwrap();
    let exponent: i32 = exponent.parse().unwrap();
    let digits = mantissa.replace('.', "");
    let digits = digits.trim_end_matches('0');
    let mut output = if value.is_sign_negative() { "-" } else { "" }.to_owned();
    if !(-4..16).contains(&exponent) {
        output.push_str(&digits[..1]);
        if digits.len() > 1 {
            output.push('.');
            output.push_str(&digits[1..]);
        }
        write!(output, "e{exponent:+03}").expect("String writes cannot fail");
    } else {
        let point = exponent + 1;
        if point <= 0 {
            output.push_str("0.");
            output.extend(std::iter::repeat_n('0', (-point) as usize));
            output.push_str(digits);
        } else if point as usize >= digits.len() {
            output.push_str(digits);
            output.extend(std::iter::repeat_n('0', point as usize - digits.len()));
        } else {
            let point = point as usize;
            output.push_str(&digits[..point]);
            output.push('.');
            output.push_str(&digits[point..]);
        }
    }
    output
}

#[cfg(test)]
mod tests;
