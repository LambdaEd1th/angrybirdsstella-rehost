//! Animation assets, timelines and scene transforms recovered from `AnimationWrapper`.

mod asset;
mod shader;
mod timeline;
mod tracks;

use std::collections::{BTreeMap, BTreeSet};

use crate::SpriteGeometry;
use crate::SpriteShader;

pub(crate) use asset::*;
pub(crate) use shader::*;
pub(crate) use timeline::*;
pub(crate) use tracks::*;

#[derive(Debug, Clone)]
pub(crate) struct AnimationControl {
    pub(crate) action: String,
    pub(crate) elapsed: f64,
    /// Time captured after the last native target-application pass. Target
    /// state change detection compares this value with `elapsed`.
    pub(crate) previous_elapsed: f64,
    pub(crate) duration: f64,
    pub(crate) speed: f64,
    pub(crate) paused: bool,
    pub(crate) playing: bool,
    /// The wrapper installs its completion delegate only after the native
    /// 0.00001-second start tick and forced target application.
    pub(crate) callback_installed: bool,
    /// Callback-less state 4 is removed from the active vector on the next
    /// Animation update.
    pub(crate) finished_pending_removal: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct AnimationPlayback {
    /// Native `Animation` active-control vector. Starting a new action appends
    /// a control; restarting an already-active action resets it in place and
    /// does not change precedence.
    pub(crate) controls: Vec<AnimationControl>,
    /// Values last written through EntityTarget's property setters. Removing
    /// the final state for a usage does not call that setter again, so the
    /// component keeps this value until another active control replaces it.
    pub(crate) latched_targets: BTreeMap<String, AnimationLatchedTarget>,
    /// AnimationWrapper component fields overwritten by every `start` call.
    /// Completion callbacks from every control read these shared fields.
    pub(crate) current_action: String,
    pub(crate) mode: String,
    /// A named stop removes the current control from the active vector, while
    /// the wrapper's retained pointer can still be paused/resumed/queried.
    pub(crate) detached_current: Option<AnimationControl>,
    /// Wrapper +0x30 map ownership is separate from the scene's active
    /// controls. close erases this pointer before the entity-removal event;
    /// closeAll erases it only after both scheduler drains.
    pub(crate) wrapper_control_present: bool,
}

impl AnimationPlayback {
    pub(crate) fn loaded(slots: &[String]) -> Self {
        let mut playback = Self {
            controls: Vec::new(),
            latched_targets: BTreeMap::new(),
            current_action: String::new(),
            mode: String::new(),
            detached_current: None,
            wrapper_control_present: false,
        };
        // Loading constructs every SpriteComponentCustom, but its retained
        // AtlasSprite pointer remains null until an EntityTarget applies a
        // discrete sprite state for the first started action.
        for slot in slots {
            let target = playback.latched_targets.entry(slot.clone()).or_default();
            target.sprite_applied = true;
            target.bound_sprite = None;
        }
        playback
    }

    #[cfg(test)]
    pub(crate) fn active(
        action: String,
        mode: String,
        elapsed: f64,
        duration: f64,
        speed: f64,
    ) -> Self {
        Self {
            controls: vec![AnimationControl {
                action: action.clone(),
                elapsed,
                previous_elapsed: elapsed,
                duration,
                speed,
                paused: false,
                playing: true,
                callback_installed: true,
                finished_pending_removal: false,
            }],
            latched_targets: BTreeMap::new(),
            current_action: action,
            mode,
            detached_current: None,
            wrapper_control_present: true,
        }
    }

    pub(crate) fn detached(action: String, duration: f64) -> Self {
        Self {
            controls: Vec::new(),
            latched_targets: BTreeMap::new(),
            current_action: action.clone(),
            mode: "once".to_owned(),
            detached_current: Some(AnimationControl {
                action,
                elapsed: 0.0,
                previous_elapsed: 0.0,
                duration,
                speed: 1.0,
                paused: false,
                playing: false,
                callback_installed: false,
                finished_pending_removal: false,
            }),
            wrapper_control_present: true,
        }
    }

    pub(crate) fn active_control_index(&self, action: &str) -> Option<usize> {
        self.controls
            .iter()
            .position(|control| control.action == action)
    }

    pub(crate) fn current_control(&self) -> Option<&AnimationControl> {
        if !self.wrapper_control_present {
            return None;
        }
        self.active_control_index(&self.current_action)
            .map(|index| &self.controls[index])
            .or(self.detached_current.as_ref())
    }

    pub(crate) fn current_control_mut(&mut self) -> Option<&mut AnimationControl> {
        if !self.wrapper_control_present {
            return None;
        }
        if let Some(index) = self.active_control_index(&self.current_action) {
            return self.controls.get_mut(index);
        }
        self.detached_current.as_mut()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AnimationLatchedTarget {
    pub(crate) translation: [f64; 2],
    pub(crate) scale: [f64; 2],
    pub(crate) rotation: f64,
    pub(crate) alpha: f64,
    pub(crate) sprite: String,
    /// Whether the native sprite ApplyHandler has run for this component.
    /// `None` after an application is a real null `Sprite*`, distinct from a
    /// synthetic/test playback that has never passed through EntityTarget.
    pub(crate) sprite_applied: bool,
    pub(crate) bound_sprite: Option<AnimationBoundSprite>,
    pub(crate) z_order: i64,
}

impl Default for AnimationLatchedTarget {
    fn default() -> Self {
        Self {
            translation: [0.0, 0.0],
            scale: [1.0, 1.0],
            rotation: 0.0,
            alpha: 1.0,
            sprite: String::new(),
            sprite_applied: false,
            bound_sprite: None,
            z_order: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AnimationBoundSprite {
    pub(crate) sprite: String,
    pub(crate) skin_transform: Option<AnimationSkinTransform>,
    pub(crate) region: crate::SpriteCatalogRegion,
    pub(crate) metrics: crate::NativeSpriteMetrics,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AnimationTransform {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) scale_x: f64,
    pub(crate) scale_y: f64,
    pub(crate) angle: f64,
}

impl Default for AnimationTransform {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            angle: 0.0,
        }
    }
}

/// The six float32 members of Purple's live 2D scene matrix, widened only at
/// the Rust storage boundary. Wrapper setters mutate this matrix directly;
/// keeping only angle/scale would lose the native normalization roundings.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AnimationAffine {
    pub(crate) m00: f64,
    pub(crate) m01: f64,
    pub(crate) m10: f64,
    pub(crate) m11: f64,
    pub(crate) x: f64,
    pub(crate) y: f64,
}

impl Default for AnimationAffine {
    fn default() -> Self {
        Self {
            m00: 1.0,
            m01: 0.0,
            m10: 0.0,
            m11: 1.0,
            x: 0.0,
            y: 0.0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AnimationDefinition {
    pub(crate) actions: BTreeMap<String, AnimationAction>,
    pub(crate) entities: BTreeSet<String>,
    pub(crate) parents: BTreeMap<String, String>,
    pub(crate) slots: Vec<String>,
    pub(crate) skins: BTreeMap<String, AnimationSkin>,
}

pub(crate) type AnimationSkin = BTreeMap<String, BTreeMap<String, AnimationSkinTransform>>;

#[derive(Debug, Clone)]
pub(crate) struct AnimationSkinTransform {
    pub(crate) sprite: String,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) scale_x: f64,
    pub(crate) scale_y: f64,
    pub(crate) angle: f64,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AnimationAction {
    pub(crate) targets: BTreeMap<String, AnimationTarget>,
    /// The single shipped `spineEvent` discrete track, including empty reset
    /// keys. Native change detection compares keyframe indices, not only the
    /// parsed non-empty event values.
    pub(crate) event_track: Vec<(f64, Option<AnimationTimelineEvent>)>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AnimationTimelineEvent {
    pub(crate) name: String,
    pub(crate) integer: i32,
    pub(crate) number: f64,
    pub(crate) text: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AnimationTarget {
    pub(crate) translation: Vec<(f64, [f64; 2])>,
    pub(crate) scale: Vec<(f64, [f64; 2])>,
    pub(crate) rotation: Vec<(f64, f64)>,
    pub(crate) alpha: Vec<(f64, f64)>,
    pub(crate) sprite: Vec<(f64, String)>,
    pub(crate) sprite_kind: AnimationSpriteTrackKind,
    pub(crate) z_order: Vec<(f64, i64)>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum AnimationSpriteTrackKind {
    /// Mockup's shipped `DiscreteString` track. The string is a skin alias;
    /// its concrete AtlasSprite pointer is resolved by the ApplyHandler.
    #[default]
    SkinAlias,
    /// `DiscreteSprite` is decoded through the animation loader's resource
    /// callback and stores the concrete Sprite pointer in the keyframe.
    DirectSprite,
}

#[derive(Debug, Default)]
pub(crate) struct AnimationRuntime {
    pub(crate) root_present: bool,
    pub(crate) root_generation: u64,
    pub(crate) pending_scene_attachments: BTreeMap<u64, PendingAnimationScene>,
    /// Entity::setParent appends children. Nested loads can attach two roots
    /// with one tag; findScene sees the first until that identity is removed.
    pub(crate) shadow_scenes: BTreeMap<String, std::collections::VecDeque<PendingAnimationScene>>,
    /// Concrete scene identity retained by asynchronous Entity::remove.
    /// Reloading a tag cannot let a queued old deletion erase its replacement.
    pub(crate) scene_generations: BTreeMap<String, u64>,
    pub(crate) next_scene_generation: u64,
    pub(crate) actions: BTreeMap<String, BTreeMap<String, f64>>,
    pub(crate) definitions: BTreeMap<String, AnimationDefinition>,
    /// Scalar compatibility state retained for draw metadata and diagnostics.
    /// The native-authoritative six-float matrix lives in `matrices`.
    pub(crate) transforms: BTreeMap<String, AnimationTransform>,
    pub(crate) matrices: BTreeMap<String, AnimationAffine>,
    /// Reflection bit propagated from the wrapper scene to all of its entity
    /// descendants by `AnimationWrapper::setScale`. The scene root itself is
    /// immediately reset to false; ordinary entity transforms retain this
    /// bit until another scale call, even if setRotation replaces the root
    /// basis in the meantime.
    pub(crate) descendant_reflections: BTreeMap<String, bool>,
    pub(crate) playback: BTreeMap<String, AnimationPlayback>,
    pub(crate) skins: BTreeMap<String, String>,
    pub(crate) skin_sets: BTreeMap<String, BTreeMap<String, AnimationSkin>>,
    pub(crate) shaders: BTreeMap<String, SpriteShader>,
    /// Native AnimationWrapper groups queued events by component/tag and
    /// drains a snapshot after every scene has updated. Events queued by a Lua
    /// callback therefore wait for the next update.
    pub(crate) pending_event_tags: Vec<String>,
    pub(crate) pending_events: BTreeMap<String, Vec<AnimationTimelineEvent>>,
    /// `AnimationWrapper::update` marks the callback-dispatch phase with the
    /// byte at native wrapper offset `+0xE9`. `close` requests made while that
    /// byte is set are retained until the current event snapshot has drained.
    pub(crate) dispatching_events: bool,
    /// Native `+0xD8` is an insertion-ordered list with linear duplicate
    /// suppression, not a tag-sorted set.
    pub(crate) deferred_close_tags: Vec<String>,
    /// AtlasSprite values captured while decoding a native `DiscreteSprite`
    /// timeline. Shipped `DiscreteString` skin attachments bind later and
    /// retain their concrete region in `AnimationLatchedTarget` instead.
    pub(crate) sprite_geometry: BTreeMap<String, BTreeMap<String, SpriteGeometry>>,
    pub(crate) sprite_metrics: BTreeMap<String, BTreeMap<String, crate::NativeSpriteMetrics>>,
    pub(crate) sprite_regions: BTreeMap<String, BTreeMap<String, crate::SpriteCatalogRegion>>,
    pub(crate) bundle_cache: BTreeMap<String, AnimationAsset>,
    pub(crate) app_data_cache: BTreeMap<String, AnimationAsset>,
    pub(crate) bundle_json_cache: BTreeMap<String, serde_json::Value>,
    pub(crate) app_data_json_cache: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone)]
pub(crate) struct AnimationAsset {
    pub(crate) actions: BTreeMap<String, f64>,
    pub(crate) definition: AnimationDefinition,
}

#[derive(Debug)]
pub(crate) struct PendingAnimationScene {
    pub(crate) tag: String,
    pub(crate) generation: u64,
    pub(crate) root_generation: u64,
    pub(crate) asset: AnimationAsset,
    pub(crate) sprite_geometry: BTreeMap<String, SpriteGeometry>,
    pub(crate) sprite_metrics: BTreeMap<String, crate::NativeSpriteMetrics>,
    pub(crate) sprite_regions: BTreeMap<String, crate::SpriteCatalogRegion>,
}
