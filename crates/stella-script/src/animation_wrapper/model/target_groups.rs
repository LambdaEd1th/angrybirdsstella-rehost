//! EntityTarget ApplyCallback/State ownership and ordering, recovered with IDA.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AnimationUsage {
    Alpha,
    Rotation,
    Scale,
    SpineEvent,
    Sprite,
    Translation,
    ZOrder,
}

impl AnimationUsage {
    fn present(self, target: &AnimationTarget) -> bool {
        match self {
            Self::Alpha => !target.alpha.is_empty(),
            Self::Rotation => !target.rotation.is_empty(),
            Self::Scale => !target.scale.is_empty(),
            Self::SpineEvent => !target.event_track.is_empty(),
            Self::Sprite => !target.sprite.is_empty(),
            Self::Translation => !target.translation.is_empty(),
            Self::ZOrder => !target.z_order.is_empty(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AnimationTargetState {
    pub(crate) action: String,
    /// Timeline pointer identity within a control, independent of usage.
    pub(crate) clip_index: usize,
    /// StateBase +8 retains its own time; an unselected state does not advance
    /// when Animation copies Control's previous time (10041D354).
    pub(crate) elapsed: f32,
}

#[derive(Debug, Clone)]
pub(crate) struct AnimationUsageGroup {
    pub(crate) usage: AnimationUsage,
    pub(crate) states: Vec<AnimationTargetState>,
}

impl AnimationAction {
    fn timelines(&self) -> impl Iterator<Item = (usize, &str, AnimationUsage)> {
        // loadClipJSON visits ordered string maps; registration order differs.
        const USAGES: [AnimationUsage; 7] = [
            AnimationUsage::Alpha,
            AnimationUsage::Rotation,
            AnimationUsage::Scale,
            AnimationUsage::SpineEvent,
            AnimationUsage::Sprite,
            AnimationUsage::Translation,
            AnimationUsage::ZOrder,
        ];
        self.clips
            .iter()
            .enumerate()
            .flat_map(|(clip_index, clip)| {
                clip.targets.iter().flat_map(move |(entity, target)| {
                    USAGES.into_iter().filter_map(move |usage| {
                        usage
                            .present(target)
                            .then_some((clip_index, entity.as_str(), usage))
                    })
                })
            })
    }
}

pub(crate) fn attach_animation_target_states(
    groups: &mut BTreeMap<String, Vec<AnimationUsageGroup>>,
    action_name: &str,
    action: &AnimationAction,
) {
    // loadClipJSON traverses string-keyed red-black trees in order
    // (1004148B4), and Clip appends timelines (10040C930). EntityTarget then
    // appends each first usage and move-appends an already attached State
    // on every start, including a named restart (10041D918/10041D9D8).
    for (clip_index, entity, usage) in action.timelines() {
        let entity_groups = groups.entry(entity.to_owned()).or_default();
        let index = entity_groups.iter().position(|group| group.usage == usage);
        let index = index.unwrap_or_else(|| {
            entity_groups.push(AnimationUsageGroup {
                usage,
                states: Vec::new(),
            });
            entity_groups.len() - 1
        });
        let states = &mut entity_groups[index].states;
        let state = states
            .iter()
            .position(|state| state.action == action_name && state.clip_index == clip_index)
            .map(|index| states.remove(index))
            .unwrap_or_else(|| AnimationTargetState {
                action: action_name.to_owned(),
                clip_index,
                elapsed: 0.0,
            });
        states.push(state);
    }
}

pub(crate) fn detach_animation_target_states(
    groups: &mut BTreeMap<String, Vec<AnimationUsageGroup>>,
    action_name: &str,
    action: &AnimationAction,
) {
    // Each retained State is removed in creation order (10041E0C0). A usage
    // shared by two clips stays live until its final individual State leaves.
    for (clip_index, entity, usage) in action.timelines() {
        let Some(entity_groups) = groups.get_mut(entity) else {
            continue;
        };
        let Some(index) = entity_groups.iter().position(|group| group.usage == usage) else {
            continue;
        };
        let states = &mut entity_groups[index].states;
        if let Some(state_index) = states
            .iter()
            .position(|state| state.action == action_name && state.clip_index == clip_index)
        {
            states.remove(state_index);
        }
        if states.is_empty() {
            entity_groups.swap_remove(index);
        }
    }
}
