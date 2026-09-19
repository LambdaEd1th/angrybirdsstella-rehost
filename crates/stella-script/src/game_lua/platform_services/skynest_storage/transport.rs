//! Frozen storage requests; only the managed identity path performs renewal.
use super::*;
use std::{fmt::Write as _, io::Read};

const MAX_RESPONSE_BYTES: u64 = 8 * 1024 * 1024;

pub(super) fn validate_base_url(url: &str) -> Result<String, &'static str> {
    let url = url.trim().trim_end_matches('/');
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("storage URL must use the http or https scheme");
    }
    let uri: ureq::http::Uri = url.parse().map_err(|_| "invalid storage URL")?;
    if !matches!(uri.scheme_str(), Some("http" | "https")) {
        return Err("storage URL must use the http or https scheme");
    }
    let authority = uri
        .authority()
        .filter(|authority| !authority.host().is_empty())
        .ok_or("storage URL is missing a host")?;
    if authority.as_str().contains('@')
        || uri.query().is_some()
        || url.contains('#')
        || url.contains('\\')
        || url.chars().any(char::is_control)
    {
        return Err("storage URL must have an unambiguous service root");
    }
    let lower = uri.path().to_ascii_lowercase();
    if lower.contains("%2f")
        || lower.contains("%5c")
        || lower
            .replace("%2e", ".")
            .split('/')
            .any(|part| matches!(part, "." | ".."))
    {
        return Err("storage URL must have an unambiguous, non-traversing path");
    }
    Ok(url.to_owned())
}

fn storage_key(key: &str) -> String {
    let mut encoded = String::with_capacity(STORAGE_PREFIX.len() + key.len());
    encoded.push_str(STORAGE_PREFIX);
    for byte in key.bytes() {
        if byte.is_ascii_alphanumeric() {
            encoded.push(char::from(byte));
        } else {
            write!(encoded, "_{byte:X}").expect("writing to String cannot fail");
        }
    }
    encoded
}

fn form_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(char::from(byte));
            }
            b' ' => encoded.push('+'),
            _ => write!(encoded, "%{byte:02X}").expect("writing to String cannot fail"),
        }
    }
    encoded
}

fn online_agent(config: &OnlineConfig) -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        // Never forward scoped access/segment headers to a redirect target.
        .max_redirects(0)
        .timeout_global(Some(config.timeout))
        .build()
        .new_agent()
}

fn service_error(status: Option<u16>, message: &'static str) -> ServiceError {
    ServiceError { status, message }
}

fn request(
    config: &OnlineConfig,
    url: &str,
    body: Option<(&str, &[u8])>,
) -> Result<ureq::http::Response<ureq::Body>, ServiceError> {
    if !config.owner.is_current() {
        return Err(service_error(None, "storage request owner expired"));
    }
    let response = match &config.auth {
        StorageAuth::Managed(identity) => {
            // The session checks its own identity lifetime independently.
            // Keep this callback atomic-only, without adding an identity lock
            // or a cross-provider lock dependency to the cancellation check.
            let storage_is_current = || {
                config.owner.storage_generation.load(Ordering::Acquire) == config.owner.generation
            };
            identity
                .request_if_current(url, body, config.timeout, &storage_is_current)
                .map_err(|status| {
                    service_error(u16::try_from(status).ok(), "storage request failed")
                })?
        }
        StorageAuth::Snapshot {
            access_token,
            segment,
        } => {
            let agent = online_agent(config);
            match body {
                Some((content_type, bytes)) => {
                    let mut request = agent.post(url).header("Content-Type", content_type);
                    if let Some(token) = access_token.as_deref() {
                        request = request.header("X-Access-Token", token);
                    }
                    if let Some(segment) = segment.as_deref() {
                        request = request.header("Rovio-Sgs", segment);
                    }
                    request.send(bytes)
                }
                None => {
                    let mut request = agent.get(url);
                    if let Some(token) = access_token.as_deref() {
                        request = request.header("X-Access-Token", token);
                    }
                    if let Some(segment) = segment.as_deref() {
                        request = request.header("Rovio-Sgs", segment);
                    }
                    request.call()
                }
            }
            .map_err(|_| service_error(None, "storage transport failed"))?
        }
    };
    if !config.owner.is_current() {
        return Err(service_error(None, "storage request owner expired"));
    }
    Ok(response)
}

fn read_json<T: for<'de> Deserialize<'de>>(
    mut response: ureq::http::Response<ureq::Body>,
) -> Result<T, ServiceError> {
    let status = response.status().as_u16();
    if status != 200 {
        // A server may echo credentials or private state in an error body.
        // Keep the status needed by the native callback mapping, never its text.
        return Err(service_error(Some(status), "storage HTTP status rejected"));
    }
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| service_error(Some(status), "storage response read failed"))?;
    if bytes.len() as u64 > MAX_RESPONSE_BYTES {
        return Err(service_error(
            Some(status),
            "storage response exceeds host limit",
        ));
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| service_error(Some(status), "StorageJsonParser: Invalid JSON response"))
}

fn read_single<T: for<'de> Deserialize<'de>>(
    response: ureq::http::Response<ureq::Body>,
) -> Result<T, ServiceError> {
    let values: Vec<T> = read_json(response)?;
    let [value]: [T; 1] = values
        .try_into()
        .map_err(|_| service_error(Some(200), "StorageJsonParser: Invalid JSON response"))?;
    Ok(value)
}

pub(super) fn request_get(config: &OnlineConfig, key: &str) -> Result<StoredValue, ServiceError> {
    let url = format!(
        "{}/state?key={}",
        config.base_url,
        form_component(&storage_key(key))
    );
    let mut value: StoredValue = read_single(request(config, &url, None)?)?;
    value.value = codec::decode(&value.value, &value.encoding)
        .map_err(|message| service_error(Some(200), message))?;
    Ok(value)
}

pub(super) fn request_set(
    config: &OnlineConfig,
    key: &str,
    value: &str,
    hash: &str,
) -> Result<StoredHash, ServiceError> {
    let url = format!("{}/state", config.base_url);
    let encoded = codec::encode(value).map_err(|message| service_error(None, message))?;
    let fields = [
        ("key", storage_key(key)),
        ("value", encoded),
        ("encoding", "SDKv2".to_owned()),
        ("hash", hash.to_owned()),
        ("force", "false".to_owned()),
    ];
    let body = fields
        .iter()
        .map(|(name, value)| format!("{}={}", form_component(name), form_component(value)))
        .collect::<Vec<_>>()
        .join("&");
    read_single(request(
        config,
        &url,
        Some(("application/x-www-form-urlencoded", body.as_bytes())),
    )?)
}

pub(super) fn request_batch(
    config: &OnlineConfig,
    key: &str,
    account_ids: &[String],
) -> Result<BTreeMap<String, String>, ServiceError> {
    let url = format!("{}/states/query", config.base_url);
    let body = serde_json::to_vec(&serde_json::json!({
        "keys": [storage_key(key)], "accountIds": account_ids,
    }))
    .map_err(|_| service_error(None, "storage request encoding failed"))?;
    let response: AccountStatesResponse =
        read_json(request(config, &url, Some(("application/json", &body)))?)?;
    let mut values = BTreeMap::new();
    for account in response.result {
        let [state] = account.states.as_slice() else {
            return Err(service_error(
                Some(200),
                "StorageJsonParser: Invalid JSON response",
            ));
        };
        let value = codec::decode(&state.value, &state.encoding)
            .map_err(|message| service_error(Some(200), message))?;
        values.insert(account.account_id, value);
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ureq::{Body, http::Response};

    fn response(status: u16, bytes: impl Into<Vec<u8>>) -> Response<Body> {
        Response::builder()
            .status(status)
            .body(Body::builder().data(bytes))
            .unwrap()
    }

    #[test]
    fn storage_transport_service_root_preserves_arbitrary_nontraversing_prefixes() {
        for (input, expected) in [
            (
                " https://fixture.invalid/custom/storage/1.0/// ",
                "https://fixture.invalid/custom/storage/1.0",
            ),
            (
                "http://127.0.0.1:12345/proxy/API/v1",
                "http://127.0.0.1:12345/proxy/API/v1",
            ),
            (
                "https://fixture.invalid/file.json/%2Ename",
                "https://fixture.invalid/file.json/%2Ename",
            ),
            ("https://fixture.invalid/", "https://fixture.invalid"),
        ] {
            assert_eq!(validate_base_url(input).unwrap(), expected);
        }
    }

    #[test]
    fn storage_transport_service_root_rejects_ambiguous_and_encoded_traversal() {
        for url in [
            "https://fixture.invalid/api/./storage",
            "https://fixture.invalid/api/../storage",
            "https://fixture.invalid/api/%2e/storage",
            "https://fixture.invalid/api/%2E%2e/storage",
            "https://fixture.invalid/api/.%2e/storage",
            "https://fixture.invalid/api/%2e./storage",
            "https://fixture.invalid/api/%2Fstorage",
            "https://fixture.invalid/api%2fstorage",
            "https://fixture.invalid/api%5Cstorage",
            "https://fixture.invalid/api\\storage",
            "https://fixture.invalid/api/..///",
            "https://fixture.invalid/api?secret=fixture/",
            "https://fixture.invalid/api#fragment",
            "https://user:fixture@fixture.invalid/api",
            "file:///fixture/storage",
            "http:///missing-host",
        ] {
            assert!(validate_base_url(url).is_err(), "accepted {url}");
        }
    }

    #[test]
    fn storage_transport_native_single_array_is_required_for_hash_and_value() {
        let hash: StoredHash = read_single(response(200, br#"[{"hash":"fixture-hash"}]"#)).unwrap();
        assert_eq!(hash.hash, "fixture-hash");
        let value: StoredValue = read_single(response(
            200,
            br#"[{"hash":"h","value":"fixture","encoding":"SDKv1"}]"#,
        ))
        .unwrap();
        assert_eq!(value.value, "fixture");
        for invalid in [
            "{}",
            "[]",
            "null",
            "false",
            "123",
            "[null]",
            "[{}]",
            r#"{"hash":"unwrapped"}"#,
            r#"[{"hash":"one"},{"hash":"two"}]"#,
        ] {
            let error = read_single::<StoredHash>(response(200, invalid)).unwrap_err();
            assert_eq!(error.status, Some(200));
            assert_eq!(error.message, "StorageJsonParser: Invalid JSON response");
        }
    }

    #[test]
    fn storage_transport_exact_200_and_errors_never_retain_body_text() {
        for status in [201, 204, 301, 401, 403, 404, 500] {
            let error = read_single::<StoredHash>(response(status, "synthetic-secret-response"))
                .unwrap_err();
            assert_eq!(error.status, Some(status));
            assert_eq!(error.message, "storage HTTP status rejected");
            assert!(!format!("{error:?}").contains("synthetic-secret-response"));
        }
        let error =
            read_single::<StoredHash>(response(200, "synthetic-private-invalid-json")).unwrap_err();
        assert!(!format!("{error:?}").contains("synthetic-private-invalid-json"));
    }

    #[test]
    fn storage_transport_limits_streaming_responses_before_json_parsing() {
        let body = Body::builder().reader(std::io::repeat(b'x'));
        let response = Response::builder().status(200).body(body).unwrap();
        let error = read_json::<serde_json::Value>(response).unwrap_err();
        assert_eq!(error.status, Some(200));
        assert_eq!(error.message, "storage response exceeds host limit");
    }
}
