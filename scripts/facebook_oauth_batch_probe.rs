//! Independent release fixture for the SDK's native multipart batch contract.
use super::*;

pub(super) fn reply_batch(
    stream: &mut TcpStream,
    request: &str,
    token: &str,
    profile: &str,
    extension: bool,
    birthday_granted: bool,
) {
    let (headers, body) = request.split_once("\r\n\r\n").unwrap();
    assert!(headers.starts_with("POST /v2.0 HTTP/1.1\r\n"));
    let mime = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-type")
                .then_some(value.trim())
        })
        .unwrap();
    assert_eq!(
        mime,
        "multipart/form-data; boundary=3i2ndDfv2rTHiSisAbouNdArYfORhtTPEefj3q2f"
    );
    let boundary = "--3i2ndDfv2rTHiSisAbouNdArYfORhtTPEefj3q2f\r\n";
    let mut paths = vec!["me"];
    if extension {
        paths.push("method/auth.extendSSOAccessToken");
    }
    paths.push("me/permissions");
    let entries: Vec<_> = paths.into_iter().map(|path| format!(
        r#"{{"method":"GET","relative_url":"{path}?format=json&sdk=ios&access_token={token}"}}"#)).collect();
    let expected = format!(
        "{boundary}Content-Disposition: form-data; name=\"batch_app_id\"\r\n\r\n12345\r\n{boundary}Content-Disposition: form-data; name=\"batch\"\r\n\r\n[{}]\r\n{boundary}",
        entries.join(",")
    );
    assert_eq!(body, expected);
    let permissions = r#"{"data":[{"permission":"public_profile","status":"granted"},{"permission":"email","status":"granted"},{"permission":"user_friends","status":"granted"},{"permission":"user_birthday","status":"granted"}]}"#;
    let permissions = permissions.replace(
        "\"user_birthday\",\"status\":\"granted\"",
        if birthday_granted {
            "\"user_birthday\",\"status\":\"granted\""
        } else {
            "\"user_birthday\",\"status\":\"declined\""
        },
    );
    let mut bodies = vec![profile];
    if extension {
        bodies.push(r#"{"access_token":"synthetic-extended-release","expires_at":4102444800}"#);
    }
    bodies.push(&permissions);
    let results: Vec<_> = bodies
        .into_iter()
        .map(|body| format!(r#"{{"code":200,"body":{body:?}}}"#))
        .collect();
    reply(stream, 200, &format!("[{}]", results.join(",")));
}
