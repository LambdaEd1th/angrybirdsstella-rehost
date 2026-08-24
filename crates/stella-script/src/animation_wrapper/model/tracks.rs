//! Typed animation-track parsing and native linear/discrete sampling.

pub(crate) fn animation_keyframes(
    track: &serde_json::Value,
) -> impl Iterator<Item = &[serde_json::Value]> {
    track
        .get("keyframes")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_array)
        .map(Vec::as_slice)
}

pub(crate) fn parse_float_track(track: &serde_json::Value) -> Vec<(f64, f64)> {
    animation_keyframes(track)
        .filter_map(|frame| {
            Some((
                f64::from(frame.first()?.as_f64()? as f32),
                f64::from(frame.get(1)?.as_f64()? as f32),
            ))
        })
        .collect()
}

pub(crate) fn parse_float2_track(track: &serde_json::Value) -> Vec<(f64, [f64; 2])> {
    animation_keyframes(track)
        .filter_map(|frame| {
            let time = f64::from(frame.first()?.as_f64()? as f32);
            let value = frame.get(1)?.as_array()?;
            Some((
                time,
                [
                    f64::from(value.first()?.as_f64()? as f32),
                    f64::from(value.get(1)?.as_f64()? as f32),
                ],
            ))
        })
        .collect()
}

pub(crate) fn parse_string_track(track: &serde_json::Value) -> Vec<(f64, String)> {
    animation_keyframes(track)
        .filter_map(|frame| {
            Some((
                f64::from(frame.first()?.as_f64()? as f32),
                frame.get(1)?.as_str()?.to_owned(),
            ))
        })
        .collect()
}

pub(crate) fn parse_int_track(track: &serde_json::Value) -> Vec<(f64, i64)> {
    animation_keyframes(track)
        .filter_map(|frame| {
            Some((
                f64::from(frame.first()?.as_f64()? as f32),
                frame.get(1)?.as_i64()?,
            ))
        })
        .collect()
}

pub(crate) fn sample_float(track: &[(f64, f64)], time: f64, default: f64) -> f64 {
    let Some(first) = track.first() else {
        return default;
    };
    let time = time as f32;
    let upper = track.partition_point(|(key_time, _)| (*key_time as f32) <= time);
    if upper == 0 {
        return first.1;
    }
    let left = track[upper - 1];
    let Some(right) = track.get(upper).copied() else {
        return left.1;
    };
    let span = (right.0 as f32) - (left.0 as f32);
    if span <= 0.0001_f32 {
        return left.1;
    }
    let progress = (time - left.0 as f32) / span;
    f64::from(((right.1 as f32) - (left.1 as f32)).mul_add(progress, left.1 as f32))
}

pub(crate) fn sample_float2(track: &[(f64, [f64; 2])], time: f64, default: [f64; 2]) -> [f64; 2] {
    let Some(first) = track.first() else {
        return default;
    };
    let time = time as f32;
    let upper = track.partition_point(|(key_time, _)| (*key_time as f32) <= time);
    if upper == 0 {
        return first.1;
    }
    let left = track[upper - 1];
    let Some(right) = track.get(upper).copied() else {
        return left.1;
    };
    let span = (right.0 as f32) - (left.0 as f32);
    if span <= 0.0001_f32 {
        return left.1;
    }
    let progress = (time - left.0 as f32) / span;
    [
        f64::from(((right.1[0] as f32) - (left.1[0] as f32)).mul_add(progress, left.1[0] as f32)),
        f64::from(((right.1[1] as f32) - (left.1[1] as f32)).mul_add(progress, left.1[1] as f32)),
    ]
}

pub(crate) fn sample_discrete<T: Clone>(track: &[(f64, T)], time: f64) -> Option<T> {
    let time = time as f32;
    let upper = track.partition_point(|(key_time, _)| (*key_time as f32) <= time);
    if upper == 0 {
        track.first().map(|(_, value)| value.clone())
    } else {
        Some(track[upper - 1].1.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuous_tracks_use_native_float32_threshold_and_fused_arithmetic() {
        let tiny_span = [(0.0, 1.0), (f64::from(0.0001_f32), 9.0)];
        assert_eq!(sample_float(&tiny_span, 0.00005, 0.0), 1.0);

        let track = [(0.0, 0.123_456_79), (1.0, 9.876_543)];
        let progress = 0.345_678_9_f32;
        let expected =
            ((track[1].1 as f32) - (track[0].1 as f32)).mul_add(progress, track[0].1 as f32);
        assert_eq!(
            sample_float(&track, f64::from(progress), 0.0).to_bits(),
            f64::from(expected).to_bits()
        );
    }

    #[test]
    fn parsed_track_times_and_values_are_quantized_to_float32() {
        let track = serde_json::json!({
            "keyframes": [[0.123456789, [1.23456789, 9.87654321]]]
        });
        assert_eq!(
            parse_float2_track(&track),
            vec![(
                f64::from(0.123_456_79_f32),
                [f64::from(1.234_567_9_f32), f64::from(9.876_543_f32)]
            )]
        );
    }
}
