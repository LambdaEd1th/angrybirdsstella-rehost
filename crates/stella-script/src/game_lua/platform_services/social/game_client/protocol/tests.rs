use super::*;

#[test]
fn native_game_score_cipher_matches_independent_openssl_vectors() {
    let vectors: Value = serde_json::from_str(include_str!("vectors.json")).unwrap();
    for row in vectors["encryption"].as_array().unwrap() {
        assert_eq!(
            encrypt(
                row["access"].as_str().unwrap(),
                row["plaintext"].as_str().unwrap()
            ),
            row["base64"]
        );
    }
    assert_eq!(
        Score::new("S01L01".to_owned(), 50000.9).submission(),
        r#"{"level":"S01L01","score":{"points":50000}}"#
    );
    assert_eq!(
        Score::new("星/🐦".to_owned(), f32::INFINITY).submission(),
        r#"{"level":"\u661F\/\uD83D\uDC26","score":{"points":9.223372036854776e+18}}"#
    );
}

#[test]
fn native_game_json_numbers_match_independent_binary64_general16_vectors() {
    let vectors: Value = serde_json::from_str(include_str!("vectors.json")).unwrap();
    for row in vectors["numbers"].as_array().unwrap() {
        let bits = u64::from_str_radix(row["bits"].as_str().unwrap(), 16).unwrap();
        assert_eq!(
            general16(f64::from_bits(bits)),
            row["text"],
            "binary64 {bits:016x}"
        );
    }
}

#[test]
fn native_game_json_writer_preserves_sorted_utf16_and_control_escapes() {
    let value = json!({"z": [true, null], "a/星": "\0\u{1}\u{7f}\u{8}\t\n\u{c}\r\"\\/é🐦"});
    assert_eq!(
        compact(&value),
        r#"{"a\/\u661F":"\u0000\u0001\u007F\b\t\n\f\r\"\\\/\u00E9\uD83D\uDC26","z":[true,null]}"#
    );
    assert_eq!(
        serde_json::from_str::<Value>(&compact(&value)).unwrap(),
        value
    );
}

#[test]
fn native_game_score_acceptance_uses_the_original_float_to_signed_conversion() {
    for (input, expected) in [
        (f32::NAN, 0),
        (f32::INFINITY, i64::MAX),
        (f32::NEG_INFINITY, i64::MIN),
        (-0.9, 0),
        (-1.9, -1),
        (16777217.0, 16777216),
    ] {
        assert_eq!(Score::new("x".to_owned(), input).points, expected);
    }
}

#[test]
fn native_game_cache_restores_pending_order_attributes_and_distinct_low32_rules() {
    let score = json!({"accountId":"cached-own","level":"S01L01","points":42,"properties":{"points":"reserved", "note":"星"}}).to_string();
    let mut cache = Cache::load(&json!({
        "transactionId": 4294967302_i64,
        "scoresToSend":[{"transactionId":4294967295_i64,"score":score}, {"transactionId":2,"score":score}],
        "cachedScores":[{"leaderBoardId":"level","rank":-1,"score":score},{"leaderBoardId":"level","rank":9,"score":score}],
    }).to_string());
    assert_eq!(cache.transaction_id, 4294967302);
    assert_eq!(
        cache
            .pending
            .iter()
            .map(|p| p.transaction_id)
            .collect::<Vec<_>>(),
        [-1, 2]
    );
    assert_eq!(cache.cached.len(), 1);
    assert_eq!(cache.cached.values().next().unwrap().rank, 4294967295);
    assert_eq!(
        cache.pending[0].score.submission(),
        r#"{"level":"S01L01","score":{"note":"\u661F","points":"reserved"}}"#
    );
    let pending = std::mem::take(&mut cache.pending);
    let saved = cache.save(17, &pending);
    let restored = Cache::load(&saved);
    assert_eq!(restored.transaction_id, 17);
    assert_eq!(restored.pending, pending);
    assert_eq!(restored.cached, cache.cached);
}

#[test]
fn native_game_cache_json_failures_reset_all_partially_loaded_members() {
    let score = json!({"accountId":"a","level":"L","points":1,"properties":{}}).to_string();
    for text in [String::new(), "{".to_owned(), "null".to_owned(), json!({"transactionId":99,"scoresToSend":[{"transactionId":1,"score":score}],"cachedScores":[{"leaderBoardId":"level","rank":true,"score":score}]}).to_string(), json!({"transactionId":9,"scoresToSend":[],"cachedScores":[]}).to_string().replace("\"transactionId\":9", "\"transactionId\":\"9\"")] {
        let cache = Cache::load(&text);
        assert_eq!(cache.transaction_id, 0);
        assert!(cache.pending.is_empty() && cache.cached.is_empty());
    }
}

#[test]
fn native_game_leaderboard_requires_native_types_and_preserves_order_duplicates_defaults() {
    let rows = leaderboard(r#"{"scores":[{"accountId":"own","score":{"points":16777217,"extra":"ignored"},"ranking":{"rank":4294967295}},{"accountId":""},{"accountId":"own","score":{},"ranking":{"rank":2.9}}]}"#).unwrap();
    assert_eq!(
        rows,
        [
            LeaderboardRow {
                account_id: "own".to_owned(),
                points: 16777217,
                rank: -1
            },
            LeaderboardRow {
                account_id: String::new(),
                points: -1,
                rank: -1
            },
            LeaderboardRow {
                account_id: "own".to_owned(),
                points: -1,
                rank: 2
            }
        ]
    );
    for text in [
        "{}",
        "null",
        r#"{"scores":{}}"#,
        r#"{"scores":[{}]}"#,
        r#"{"scores":[{"accountId":1}]}"#,
        r#"{"scores":[{"accountId":"a","score":null}]}"#,
        r#"{"scores":[{"accountId":"a","score":{"points":"1"}}]}"#,
        r#"{"scores":[{"accountId":"a","ranking":{}}]}"#,
        r#"{"scores":[{"accountId":"a","ranking":{"rank":true}}]}"#,
    ] {
        assert!(leaderboard(text).is_err(), "accepted {text}");
    }
}
