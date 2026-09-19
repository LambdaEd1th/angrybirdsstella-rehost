//! FBRequestConnection2DC0D0/2DC5FC/2DCB4C response and multipart transport.
use super::*;

pub(crate) const BOUNDARY: &str = "3i2ndDfv2rTHiSisAbouNdArYfORhtTPEefj3q2f";

pub(crate) struct Response {
    pub status: u16,
    pub value: Value,
    pub error: Option<SocialPlatformError>,
}

impl Response {
    pub fn into_result(self) -> Result<Value, SocialPlatformError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        if !(200..300).contains(&self.status) {
            return Err(SocialPlatformError::Http(self.status));
        }
        Ok(self.value)
    }
}

pub(crate) fn send(url: &str, body: Option<&str>) -> Result<Response, SocialPlatformError> {
    let agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .new_agent();
    let mut response = match body {
        Some(body) => agent
            .post(url)
            .header(
                "Content-Type",
                format!("multipart/form-data; boundary={BOUNDARY}"),
            )
            .send(body),
        None => agent.get(url).call(),
    }
    .map_err(|_| SocialPlatformError::Transport)?;
    let status = response.status().as_u16();
    // The native SDK rejects image MIME types before trying JSON decoding.
    if response
        .headers()
        .get("content-type")
        .and_then(|s| s.to_str().ok())
        .is_some_and(|s| s.starts_with("image"))
    {
        return Err(SocialPlatformError::InvalidResponse);
    }
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| SocialPlatformError::Transport)?;
    if bytes.len() as u64 > MAX_RESPONSE_BYTES {
        return Err(SocialPlatformError::InvalidResponse);
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| SocialPlatformError::InvalidResponse)?;
    Ok(Response {
        status,
        value: decode(text),
        error: None,
    })
}

// NSJSONSerialization options0 admits containers, not JSON fragments.
// 2DCB4C preserves the original text when this first decode fails.
pub(crate) fn decode(text: &str) -> Value {
    match serde_json::from_str::<Value>(text) {
        Ok(value @ (Value::Array(_) | Value::Object(_))) => value,
        _ => serde_json::json!({"FACEBOOK_NON_JSON_RESULT": text}),
    }
}

pub(crate) fn multipart(app_id: &str, paths: &[&str], token: &str) -> String {
    let entries: Vec<_> = paths.iter().map(|path| serde_json::json!({
        "method": "GET",
        "relative_url": format!("{path}?format=json&sdk=ios&access_token={}", encode_query(token)),
    })).collect();
    let batch = serde_json::to_string(&entries).expect("Graph batch serialization");
    let mut body = format!("--{BOUNDARY}\r\n");
    for (key, value) in [("batch_app_id", app_id), ("batch", batch.as_str())] {
        write!(
            body,
            "Content-Disposition: form-data; name=\"{key}\"\r\n\r\n{value}\r\n--{BOUNDARY}\r\n"
        )
        .expect("Graph multipart formatting");
    }
    body
}

pub(crate) fn unpack(
    response: Response,
    count: usize,
) -> Result<Vec<Response>, SocialPlatformError> {
    let connection_error = (!(200..300).contains(&response.status))
        .then_some(SocialPlatformError::Http(response.status));
    let Value::Array(entries) = response.value else {
        return Err(SocialPlatformError::InvalidResponse);
    };
    if entries.len() != count {
        return Err(SocialPlatformError::InvalidResponse);
    }
    entries
        .into_iter()
        .map(|entry| {
            let Value::Object(mut entry) = entry else {
                // Native non-dictionaries reach completion as nil graph objects.
                return Ok(Response {
                    status: 200,
                    value: Value::Null,
                    error: connection_error,
                });
            };
            entry.retain(|_, value| !value.is_null());
            let status = match entry.get("code") {
                None => 200,
                Some(Value::Number(n)) => {
                    let number = n
                        .as_i64()
                        .map(|n| n as i32)
                        .or_else(|| n.as_u64().map(|n| n as i32))
                        .unwrap_or_else(|| n.as_f64().unwrap_or(0.0) as i32);
                    u16::try_from(number).map_err(|_| SocialPlatformError::InvalidResponse)?
                }
                Some(Value::Bool(b)) => u16::from(*b),
                Some(Value::String(s)) => {
                    let s = s.trim_start_matches([' ', '\t']);
                    let end = s
                        .char_indices()
                        .take_while(|(i, c)| {
                            c.is_ascii_digit() || (*i == 0 && matches!(c, '+' | '-'))
                        })
                        .map(|(i, c)| i + c.len_utf8())
                        .last()
                        .unwrap_or(0);
                    let number = s[..end]
                        .parse::<i64>()
                        .unwrap_or(0)
                        .clamp(i32::MIN as i64, i32::MAX as i64);
                    u16::try_from(number).map_err(|_| SocialPlatformError::InvalidResponse)?
                }
                Some(_) => return Err(SocialPlatformError::InvalidResponse),
            };
            let value = match entry.remove("body") {
                None => Value::Null,
                Some(Value::String(text)) => decode(&text),
                Some(_) => return Err(SocialPlatformError::InvalidResponse),
            };
            let error = connection_error.or_else(|| {
                ["error", "error_code", "error_msg", "error_reason"]
                    .iter()
                    .any(|key| entry.contains_key(*key))
                    .then_some(SocialPlatformError::Graph)
            });
            Ok(Response {
                status,
                value,
                error,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn response(value: Value) -> Response {
        Response {
            status: 200,
            value,
            error: None,
        }
    }

    #[test]
    fn facebook_batch_response_count_body_wrapping_and_result_errors_match_sdk() {
        let outer_failure = Response {
            status: 400,
            value: json!([{"code":200,"body":"{\"error\":{\"code\":190}}"}]),
            error: None,
        };
        let child = unpack(outer_failure, 1).unwrap().remove(0);
        assert_eq!(child.value["error"]["code"], 190);
        assert_eq!(
            child.into_result().unwrap_err(),
            SocialPlatformError::Http(400)
        );
        assert!(matches!(
            unpack(response(json!([])), 2),
            Err(SocialPlatformError::InvalidResponse)
        ));
        assert!(matches!(
            unpack(response(decode("not JSON")), 2),
            Err(SocialPlatformError::InvalidResponse)
        ));
        let mut parts = unpack(
            response(json!([
                {"code":200,"body":"not JSON","error":null},
                {"code":"200suffix","body":"{\"error\":null}","error_reason":"denied"},
                {"code":4294967496u64,"body":"[]"},
                {"code":400,"body":"{\"error\":{\"code\":190}}"}
            ])),
            4,
        )
        .unwrap()
        .into_iter();
        assert_eq!(
            parts.next().unwrap().into_result().unwrap(),
            json!({"FACEBOOK_NON_JSON_RESULT":"not JSON"})
        );
        assert_eq!(
            parts.next().unwrap().into_result().unwrap_err(),
            SocialPlatformError::Graph
        );
        assert_eq!(parts.next().unwrap().into_result().unwrap(), json!([]));
        assert_eq!(
            parts.next().unwrap().into_result().unwrap_err(),
            SocialPlatformError::Http(400)
        );
    }
}
