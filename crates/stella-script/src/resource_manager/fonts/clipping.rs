//! ResourceManager `clipText` line splitting (`sub_10004F630`).

pub(crate) fn native_clip_text_lines(
    text: &str,
    maximum_width: f32,
    string_width: impl Fn(&str) -> i32,
) -> (Vec<String>, f32) {
    let characters = text.chars().collect::<Vec<_>>();
    let mut lines = Vec::new();
    let mut widest = 0_i32;
    let mut start = 0_usize;
    let separators = ['\n', ' ', '-', '\u{200b}'];

    while start < characters.len() {
        let mut candidate_end = characters.len();
        let mut scan = start;
        let mut fitted_segments = 0_usize;
        let line_end;

        loop {
            let previous_end = candidate_end;
            candidate_end = characters.len();
            for (index, character) in characters.iter().copied().enumerate().skip(scan) {
                if separators.contains(&character) {
                    candidate_end = index + usize::from(character == '-');
                    break;
                }
            }

            let candidate = characters[start..candidate_end].iter().collect::<String>();
            if string_width(&candidate) as f32 >= maximum_width {
                if fitted_segments == 0 {
                    let mut length = 1_usize;
                    loop {
                        let forced_end = (start + length).min(characters.len());
                        let forced = characters[start..forced_end].iter().collect::<String>();
                        if string_width(&forced) as f32 >= maximum_width {
                            // `0x10004F978..0x10004F9A8` tests lengths
                            // 1, 2, ... but stores the tested length minus
                            // one. The glyph which first reaches the limit
                            // therefore starts the following line.
                            let native_end = start + length - 1;
                            // A first glyph which itself reaches the limit
                            // makes Purple emit an empty line forever. Retain
                            // forward progress at that non-terminating edge.
                            let safe_end = native_end.max(start + 1).min(characters.len());
                            scan = safe_end;
                            line_end = safe_end;
                            break;
                        }
                        if forced_end == characters.len() {
                            scan = forced_end;
                            line_end = forced_end;
                            break;
                        }
                        length += 1;
                    }
                } else {
                    line_end = previous_end;
                }
                break;
            }

            if candidate_end < characters.len() && characters[candidate_end] == '\n' {
                scan = candidate_end;
                line_end = candidate_end;
                break;
            }

            scan = candidate_end;
            while scan < characters.len() && matches!(characters[scan], ' ' | '\u{200b}') {
                scan += 1;
            }
            fitted_segments += 1;
            if scan >= characters.len() {
                line_end = candidate_end;
                break;
            }
        }

        let line = characters[start..line_end].iter().collect::<String>();
        widest = widest.max(string_width(&line));
        lines.push(line);
        if scan < characters.len() && characters[scan] == '\n' {
            scan += 1;
        }
        start = scan;
    }

    // `0x10004FB50` publishes the signed integer maximum through SCVTF S0.
    (lines, widest as f32)
}

/// Purple's `sub_100585CDC` advances one byte after a failed UTF-8 decode and
/// then resumes conversion. Lua strings are byte strings, so clipText accepts
/// malformed input even though the host representation must be valid UTF-8.
pub(crate) fn native_utf8_skipping_invalid(bytes: &[u8]) -> String {
    let mut decoded = String::with_capacity(bytes.len());
    let mut offset = 0_usize;
    while offset < bytes.len() {
        match std::str::from_utf8(&bytes[offset..]) {
            Ok(valid) => {
                decoded.push_str(valid);
                break;
            }
            Err(error) => {
                let valid_length = error.valid_up_to();
                if valid_length != 0 {
                    decoded.push_str(
                        std::str::from_utf8(&bytes[offset..offset + valid_length])
                            .expect("from_utf8 reported this prefix as valid"),
                    );
                    offset += valid_length;
                }
                // The native decoder ignores exactly the byte at the failed
                // iterator position, irrespective of the reported sequence
                // length, and retries at the following byte.
                offset += 1;
            }
        }
    }
    decoded
}
