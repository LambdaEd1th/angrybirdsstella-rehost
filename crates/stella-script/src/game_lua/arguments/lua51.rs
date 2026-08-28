//! Coercions used only by hand-written Lua 5.1 C-API members.

use mlua::Value;

/// Lua 5.1's direct `lua_isnumber`/`lua_tonumber` pair also accepts numeric
/// strings. Generated adapters use `arguments::strict` instead.
pub(crate) fn native_lua51_number(value: &Value) -> Option<f64> {
    let value = match value {
        Value::Integer(value) => *value as f64,
        Value::Number(value) => *value,
        Value::String(value) => parse_number(value.as_bytes().as_ref())?,
        _ => return None,
    };
    Some(f64::from(value as f32))
}

fn parse_number(bytes: &[u8]) -> Option<f64> {
    let text = std::str::from_utf8(bytes).ok()?.trim();
    if text.is_empty() {
        return None;
    }
    if let Ok(number) = text.parse::<f64>() {
        return Some(number);
    }

    // luaO_str2d falls back to hexadecimal integer parsing if strtod does
    // not consume an 0x-prefixed value.
    let (sign, unsigned) = match text.as_bytes().first() {
        Some(b'+') => (1.0, &text[1..]),
        Some(b'-') => (-1.0, &text[1..]),
        _ => (1.0, text),
    };
    let digits = unsigned
        .strip_prefix("0x")
        .or_else(|| unsigned.strip_prefix("0X"))?;
    (!digits.is_empty())
        .then(|| u64::from_str_radix(digits, 16).ok())
        .flatten()
        .map(|number| sign * number as f64)
}

/// Lua 5.1's `lua_isstring`/`lua_tolstring` accepts strings and numbers.
pub(crate) fn native_lua51_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.to_string_lossy()),
        Value::Integer(value) => Some(native_float_string(*value as f32)),
        Value::Number(value) => Some(native_float_string(*value as f32)),
        _ => None,
    }
}

/// Purple's float-based Lua 5.1 fork uses `sprintf("%.9g", value)` when
/// `lua_tolstring` converts a number. Keep that contract without depending on
/// the host C locale or the host Lua build's double formatter.
fn native_float_string(value: f32) -> String {
    if value.is_nan() {
        return "nan".to_owned();
    }
    if value == f32::INFINITY {
        return "inf".to_owned();
    }
    if value == f32::NEG_INFINITY {
        return "-inf".to_owned();
    }

    // First round to nine significant digits in scientific form. Its rounded
    // exponent determines the `%g` fixed/scientific choice, including values
    // that carry across a power-of-ten boundary during formatting.
    let scientific = format!("{value:.8e}");
    let (mantissa, exponent) = scientific
        .split_once('e')
        .expect("Rust lower-exponential formatting always contains an exponent");
    let exponent = exponent
        .parse::<i32>()
        .expect("Rust lower-exponential formatting emits a decimal exponent");
    if !(-4..9).contains(&exponent) {
        let mantissa = trim_fractional_zeroes(mantissa);
        return format!("{mantissa}e{exponent:+03}");
    }

    let decimals = (8 - exponent).max(0) as usize;
    trim_fractional_zeroes(&format!("{value:.decimals$}"))
}

fn trim_fractional_zeroes(value: &str) -> String {
    if !value.contains('.') {
        return value.to_owned();
    }
    value.trim_end_matches('0').trim_end_matches('.').to_owned()
}

#[cfg(test)]
mod tests {
    use super::native_float_string;

    #[test]
    fn purple_number_to_string_matches_float_lua_percent_nine_g() {
        for (value, expected) in [
            (0.0_f32, "0"),
            (-0.0_f32, "-0"),
            (1.234_567_9_f32, "1.23456788"),
            (123_456_789.0_f32, "123456792"),
            (1_000_000_000.0_f32, "1e+09"),
            (0.0001_f32, "9.99999975e-05"),
            (0.00001_f32, "9.99999975e-06"),
            (f32::INFINITY, "inf"),
            (f32::NEG_INFINITY, "-inf"),
        ] {
            assert_eq!(native_float_string(value), expected, "{value:?}");
        }
        assert_eq!(native_float_string(f32::NAN), "nan");
    }

    #[cfg(target_vendor = "apple")]
    #[test]
    fn pure_rust_percent_nine_g_matches_darwin_sprintf_corpus() {
        let mut state = 0x8f3a_72d1_u32;
        for _ in 0..100_000 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let value = f32::from_bits(state);
            if !value.is_finite() {
                continue;
            }

            let mut output = [0_i8; 64];
            // SAFETY: output is a valid writable array, the format is a
            // NUL-terminated constant, and C varargs promotes f32 to f64.
            let written = unsafe {
                libc::snprintf(
                    output.as_mut_ptr(),
                    output.len(),
                    c"%.9g".as_ptr(),
                    f64::from(value),
                )
            };
            assert!(written >= 0 && (written as usize) < output.len());
            // SAFETY: successful snprintf always terminates within output.
            let expected = unsafe { std::ffi::CStr::from_ptr(output.as_ptr()) }
                .to_str()
                .unwrap();
            assert_eq!(native_float_string(value), expected, "0x{state:08x}");
        }
    }
}
