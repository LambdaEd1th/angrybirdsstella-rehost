//! Identity request routing, recovered from Purple's `sub_100674A48`.

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct IdentityEndpoint {
    /// Explicit origin and optional reverse-proxy prefix, ending in /identity.
    service_root: String,
}

impl IdentityEndpoint {
    pub(super) fn parse(value: &str) -> Result<Self, String> {
        let value = value.trim();
        if !value.starts_with("http://") && !value.starts_with("https://") {
            return Err("identity URL must use the http or https scheme".to_owned());
        }
        if value.contains('#') {
            return Err("identity URL must not contain a fragment".to_owned());
        }
        let uri: ureq::http::Uri = value
            .parse()
            .map_err(|_| "identity URL is not a valid absolute HTTP URL".to_owned())?;
        let authority = uri
            .authority()
            .filter(|authority| !authority.host().is_empty())
            .ok_or_else(|| "identity URL is missing a host".to_owned())?;
        if authority.as_str().contains('@') || uri.query().is_some() {
            return Err("identity URL must not contain user information or a query".to_owned());
        }
        let path = uri.path().trim_end_matches('/');
        // Reject ambiguous path separators and traversal instead of letting an
        // HTTP implementation normalize the caller's reverse-proxy prefix.
        let lower = path.to_ascii_lowercase();
        if path.contains('\\')
            || lower.contains("%2f")
            || lower.contains("%5c")
            || lower
                .replace("%2e", ".")
                .split('/')
                .any(|part| matches!(part, "." | ".."))
        {
            return Err("identity URL must have an unambiguous, non-traversing path".to_owned());
        }
        let root = path
            .strip_suffix("/2.0")
            .or_else(|| path.strip_suffix("/3.0"))
            .filter(|root| root.ends_with("/identity"))
            .ok_or_else(|| "identity URL must end in /identity/2.0 or /identity/3.0".to_owned())?;
        Ok(Self {
            service_root: format!("{}://{}{root}", uri.scheme_str().unwrap(), authority),
        })
    }

    pub(super) fn request_url(&self, operation: &str) -> String {
        // 0x100674A48 creates identity/2.0, lowercases only the comparison
        // copy of the operation, then assigns version 3.0 for these four
        // exact routes at 0x100674B34..0x100674B50. The operation itself is
        // unchanged. This selection does not implement those operations.
        let version = if ["abid/login", "guest/upgrade", "profile/own", "refresh"]
            .iter()
            .any(|candidate| operation.eq_ignore_ascii_case(candidate))
        {
            "3.0"
        } else {
            "2.0"
        };
        format!("{}/{version}/{operation}", self.service_root)
    }

    pub(super) fn session_url(&self, client_id: &str) -> String {
        // 100744F5C..100744FF8 uses the same parent clientId as access metadata.
        // Encode the explicitly configured id as one path component: host
        // configuration must not escape the provider's reverse-proxy prefix.
        let mut component = String::new();
        for byte in client_id.bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'~') {
                component.push(char::from(byte));
            } else {
                use std::fmt::Write as _;
                write!(&mut component, "%{byte:02X}").expect("String writes cannot fail");
            }
        }
        let root = self.service_root.strip_suffix("/identity").unwrap();
        format!("{root}/session/1/apps/{component}/sessions")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_routes_preserve_origin_prefix_and_native_version_exceptions() {
        for configured_version in ["2.0", "3.0"] {
            let root = IdentityEndpoint::parse(&format!(
                " https://[::1]:8443/proxy/purple%20test/identity/{configured_version}/ "
            ))
            .unwrap();
            for operation in [
                "abid/login",
                "guest/upgrade",
                "profile/own",
                "refresh",
                "PROFILE/OWN",
            ] {
                assert_eq!(
                    root.request_url(operation),
                    format!("https://[::1]:8443/proxy/purple%20test/identity/3.0/{operation}")
                );
            }
            for operation in [
                "access",
                "profile/nickname/validate",
                "profile/own/child",
                "refresh/",
            ] {
                assert_eq!(
                    root.request_url(operation),
                    format!("https://[::1]:8443/proxy/purple%20test/identity/2.0/{operation}")
                );
            }
        }
    }

    #[test]
    fn identity_routes_reject_ambiguous_or_nonservice_roots() {
        for value in [
            "file:///identity/2.0",
            "//host/identity/2.0",
            "http:///identity/2.0",
            "http://host",
            "http://host/custom",
            "http://host/identity",
            "http://host/identity/4.0",
            "http://host/notidentity/2.0",
            "http://host/identity/2.0/access",
            "http://user:password@host/identity/2.0",
            "http://host/identity/2.0?token=value",
            "http://host/identity/2.0#fragment",
            "http://host/../identity/2.0",
            "http://host/%2e%2E/identity/2.0",
            "http://host/prefix%2fother/identity/2.0",
        ] {
            assert!(IdentityEndpoint::parse(value).is_err(), "accepted {value}");
        }
    }
}
