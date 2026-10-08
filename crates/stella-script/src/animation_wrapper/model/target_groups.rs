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
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "alpha" => Some(Self::Alpha),
            "rotation" => Some(Self::Rotation),
            "scale" => Some(Self::Scale),
            "spineEvent" => Some(Self::SpineEvent),
            "sprite" => Some(Self::Sprite),
            "translation" => Some(Self::Translation),
            "zOrder" => Some(Self::ZOrder),
            _ => None,
        }
    }

    fn present(self, target: &AnimationTarget) -> bool {
        match self {
            Self::Alpha => !target.alpha.is_empty(),
            Self::Rotation => !target.rotation.is_empty(),
            Self::Scale => !target.scale.is_empty(),
            Self::SpineEvent => false,
            Self::Sprite => !target.sprite.is_empty(),
            Self::Translation => !target.translation.is_empty(),
            Self::ZOrder => !target.z_order.is_empty(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AnimationTargetState {
    pub(crate) action: String,
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
    pub(crate) fn usage_order(&self) -> BTreeMap<String, Vec<AnimationUsage>> {
        if !self.native_usages.is_empty() {
            return self.native_usages.clone();
        }
        // Parsed scenes retain loadClipJSON's order. Typed unit fixtures
        // built without JSON use the equivalent single-clip lexical order.
        let ordered = [
            AnimationUsage::Alpha,
            AnimationUsage::Rotation,
            AnimationUsage::Scale,
            AnimationUsage::Sprite,
            AnimationUsage::Translation,
            AnimationUsage::ZOrder,
        ];
        let mut usages = self
            .targets
            .iter()
            .map(|(entity, target)| {
                (
                    entity.clone(),
                    ordered
                        .into_iter()
                        .filter(|usage| usage.present(target))
                        .collect(),
                )
            })
            .collect::<BTreeMap<String, Vec<AnimationUsage>>>();
        if !self.event_track.is_empty() {
            usages
                .entry(String::new())
                .or_default()
                .push(AnimationUsage::SpineEvent);
        }
        usages
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
    for (entity, usages) in action.usage_order() {
        let entity_groups = groups.entry(entity).or_default();
        for usage in usages {
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
                .position(|state| state.action == action_name)
                .map(|index| states.remove(index))
                .unwrap_or_else(|| AnimationTargetState {
                    action: action_name.to_owned(),
                    elapsed: 0.0,
                });
            states.push(state);
        }
    }
}

pub(crate) fn detach_animation_target_states(
    groups: &mut BTreeMap<String, Vec<AnimationUsageGroup>>,
    action_name: &str,
    action: &AnimationAction,
) {
    // Stop walks the control's retained target-state vector in creation
    // order. State erasure preserves relative order; an emptied usage group
    // is replaced by the final group (10041E0C0), rather than sorted again.
    for (entity, usages) in action.usage_order() {
        let Some(entity_groups) = groups.get_mut(&entity) else {
            continue;
        };
        for usage in usages {
            let Some(index) = entity_groups.iter().position(|group| group.usage == usage) else {
                continue;
            };
            entity_groups[index]
                .states
                .retain(|state| state.action != action_name);
            if entity_groups[index].states.is_empty() {
                entity_groups.swap_remove(index);
            }
        }
    }
}
