//! Independent loopback fixture checks for the recovered SDK batch contract.
use serde_json::{Value, json};
use std::{io::Read, net::TcpStream};

pub(crate) fn read_request(stream: &mut TcpStream) -> (String, String) {
    let mut headers = Vec::new();
    while !headers.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        headers.push(byte[0]);
        assert!(headers.len() < 16384);
    }
    let headers = String::from_utf8(headers).unwrap();
    let length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    assert!(length < 65536);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).unwrap();
    (headers, String::from_utf8(body).unwrap())
}

pub(crate) fn assert_batch(
    headers: &str,
    body: &str,
    app: &str,
    paths: &[&str],
    encoded_token: &str,
) {
    assert!(headers.starts_with("POST /v2.0 HTTP/1.1\r\n"), "{headers}");
    let content_type = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-type")
                .then_some(value.trim())
        })
        .unwrap();
    assert_eq!(
        content_type,
        "multipart/form-data; boundary=3i2ndDfv2rTHiSisAbouNdArYfORhtTPEefj3q2f"
    );
    let boundary = "--3i2ndDfv2rTHiSisAbouNdArYfORhtTPEefj3q2f\r\n";
    let parts: Vec<_> = body.split(boundary).collect();
    assert_eq!(parts.len(), 4);
    assert_eq!(parts[0], "");
    assert_eq!(parts[3], "");
    assert_eq!(
        parts[1],
        format!("Content-Disposition: form-data; name=\"batch_app_id\"\r\n\r\n{app}\r\n")
    );
    let batch = parts[2]
        .strip_prefix("Content-Disposition: form-data; name=\"batch\"\r\n\r\n")
        .unwrap()
        .strip_suffix("\r\n")
        .unwrap();
    let entries: Value = serde_json::from_str(batch).unwrap();
    let expected: Vec<_> = paths.iter().map(|path| json!({
        "method": "GET", "relative_url": format!("{path}?format=json&sdk=ios&access_token={encoded_token}")
    })).collect();
    assert_eq!(entries, Value::Array(expected));
}

pub(crate) fn batch_response(entries: &[(u16, Value)]) -> String {
    Value::Array(
        entries
            .iter()
            .map(|(code, body)| json!({"code":code,"body":body.to_string()}))
            .collect(),
    )
    .to_string()
}
