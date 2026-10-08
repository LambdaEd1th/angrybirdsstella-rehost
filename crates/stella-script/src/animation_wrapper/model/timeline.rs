//! Animation action parsing and native discrete `spineEvent` state changes.

use super::*;

pub(crate) fn parse_animation_action(value: &serde_json::Value, action: &mut AnimationAction) {
    let Some(clips) = value.get("clips").and_then(serde_json::Value::as_object) else {
        return;
    };
    for clip in clips.values() {
        let mut parsed = AnimationClip::default();
        let Some(targets) = clip.get("targets").and_then(serde_json::Value::as_object) else {
            continue;
        };
        for (name, properties) in targets {
            let target = parsed.targets.entry(name.clone()).or_default();
            if let Some(track) = properties.get("translation") {
                target.translation = parse_float2_track(track);
            }
            if let Some(track) = properties.get("scale") {
                target.scale = parse_float2_track(track);
            }
            if let Some(track) = properties.get("rotation") {
                target.rotation = parse_float_track(track);
            }
            if let Some(track) = properties.get("alpha") {
                target.alpha = parse_float_track(track);
            }
            if let Some(track) = properties.get("sprite") {
                target.sprite = parse_string_track(track);
                target.sprite_kind = if track.get("type").and_then(serde_json::Value::as_str)
                    == Some("DiscreteSprite")
                {
                    AnimationSpriteTrackKind::DirectSprite
                } else {
                    AnimationSpriteTrackKind::SkinAlias
                };
            }
            if let Some(track) = properties.get("zOrder") {
                target.z_order = parse_int_track(track);
            }
            if let Some(track) = properties.get("spineEvent") {
                target
                    .event_track
                    .extend(parse_string_track(track).into_iter().map(|(time, raw)| {
                        (f64::from(time as f32), parse_animation_timeline_event(&raw))
                    }));
            }
        }
        // loadAnimationJSON's ordered map appends each Clip; loadClipJSON
        // appends target/usage Timelines without merging (100414BD4/10040C930).
        action.clips.push(parsed);
    }
}

pub(crate) fn parse_animation_timeline_event(raw: &str) -> Option<AnimationTimelineEvent> {
    if raw.is_empty() {
        return None;
    }
    let mut fields = raw.split(':');
    let name = fields.next().unwrap_or_default();
    if name.is_empty() {
        return None;
    }
    Some(AnimationTimelineEvent {
        name: name.to_owned(),
        integer: fields
            .next()
            .unwrap_or_default()
            .parse::<i32>()
            .unwrap_or(0),
        number: f64::from(
            fields
                .next()
                .unwrap_or_default()
                .parse::<f32>()
                .unwrap_or(0.0),
        ),
        // sub_1000121F4 takes the substring only up to the next colon.
        text: fields.next().unwrap_or_default().to_owned(),
    })
}

fn animation_event_state_index(
    track: &[(f64, Option<AnimationTimelineEvent>)],
    time: f64,
) -> Option<usize> {
    if track.is_empty() {
        return None;
    }
    let upper = track.partition_point(|(key_time, _)| *key_time <= time);
    let index = upper.saturating_sub(1).min(track.len() - 1);
    Some(index)
}

/// Mode 2/4 application (`seek`/`start`) always invokes the registered apply
/// callback with the currently selected discrete state. AnimationWrapper then
/// ignores only an empty string value.
pub(crate) fn animation_event_at(
    target: &AnimationTarget,
    time: f64,
) -> Option<AnimationTimelineEvent> {
    let index = animation_event_state_index(&target.event_track, time)?;
    target.event_track[index].1.clone()
}

/// Ordinary mode 3 application invokes a discrete callback only when the
/// selected keyframe index changed, and then exposes only the final state.
pub(crate) fn animation_event_after_state_change(
    target: &AnimationTarget,
    start: f64,
    end: f64,
) -> Option<AnimationTimelineEvent> {
    let start_index = animation_event_state_index(&target.event_track, start)?;
    let end_index = animation_event_state_index(&target.event_track, end)?;
    // 100421F1C compares State's key index; 10041E41C gates mode 3's
    // callback on that result. Retain owned payloads only for queued events.
    if start_index == end_index {
        None
    } else {
        target.event_track[end_index].1.clone()
    }
}

pub(crate) fn queue_animation_event(
    runtime: &mut AnimationRuntime,
    tag: &str,
    event: AnimationTimelineEvent,
) {
    match runtime.pending_events.entry(tag.to_owned()) {
        std::collections::btree_map::Entry::Vacant(entry) => {
            runtime.pending_event_tags.push(tag.to_owned());
            entry.insert(vec![event]);
        }
        std::collections::btree_map::Entry::Occupied(mut entry) => {
            entry.get_mut().push(event);
        }
    }
}
