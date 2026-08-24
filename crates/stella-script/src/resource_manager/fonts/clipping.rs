//! ResourceManager `clipText` line splitting (`sub_10004F630`).

pub(crate) fn native_clip_text_lines(
    text: &str,
    maximum_width: f32,
    string_width: impl Fn(&str) -> i32,
) -> (Vec<String>, i32) {
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
                    let mut length = 0_usize;
                    loop {
                        length += 1;
                        let forced_end = (start + length).min(characters.len());
                        let forced = characters[start..forced_end].iter().collect::<String>();
                        if string_width(&forced) as f32 >= maximum_width
                            || forced_end == characters.len()
                        {
                            scan = forced_end;
                            line_end = forced_end;
                            break;
                        }
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

    (lines, widest)
}
