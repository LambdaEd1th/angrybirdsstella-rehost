//! Live three-level traversal performed by `sub_10004BAB4`.

use crate::*;

pub(super) struct NativeSceneWalk {
    render: Arc<Mutex<RenderBridge>>,
    cursor: NativeSceneCursor,
}

struct NativeSceneCursor {
    minimum_z: i32,
    maximum_z: i32,
    draw_world_scale: f32,
    z: Option<i32>,
    sheet: Option<u64>,
    name_index: usize,
    emit_z: bool,
    finished: bool,
}

impl NativeSceneWalk {
    pub(super) fn new(
        render: Arc<Mutex<RenderBridge>>,
        bounds: (i32, i32),
        draw_world_scale: f32,
    ) -> Self {
        Self {
            render,
            cursor: NativeSceneCursor {
                minimum_z: bounds.0,
                maximum_z: bounds.1,
                draw_world_scale,
                z: None,
                sheet: None,
                name_index: 0,
                emit_z: false,
                finished: false,
            },
        }
    }
}

impl NativeSceneCursor {
    fn advance_z(&mut self, index: &NativeSceneRenderIndex) -> bool {
        let next = index.next_z_in_range(self.minimum_z, self.maximum_z, self.z);
        self.z = next;
        self.sheet = None;
        self.name_index = 0;
        self.emit_z = next.is_some();
        self.finished = next.is_none();
        next.is_some()
    }
}

impl Iterator for NativeSceneWalk {
    /// `None` names mark every persistent outer z node, including empty ones.
    /// Names are then fetched by live vector index so callback mutations have
    /// the same shift/append behavior as Purple's pointer loop.
    type Item = (i32, Option<(Arc<str>, SceneDrawVisit)>);

    fn next(&mut self) -> Option<Self::Item> {
        // One bridge acquisition corresponds to one resumed native tree walk.
        // The lock is still released before yielding to Lua, and the next call
        // re-reads the live vector length exactly like 0x10004C340. Borrowing
        // owner and cursor independently also avoids an atomic Arc retain and
        // release per item: Purple retains GameLua once for the complete walk.
        let mut bridge = self.render.lock().expect("render bridge lock poisoned");
        let cursor = &mut self.cursor;
        loop {
            let Some(current_z) = cursor.z else {
                if cursor.finished {
                    return None;
                }
                if !cursor.advance_z(&bridge.scene_render_index) {
                    return None;
                }
                continue;
            };
            if cursor.emit_z {
                cursor.emit_z = false;
                return Some((current_z, None));
            }
            let current_sheet = match cursor.sheet {
                Some(current_sheet) => current_sheet,
                None => {
                    let first = bridge.scene_render_index.first_sheet(current_z);
                    let Some(first) = first else {
                        cursor.advance_z(&bridge.scene_render_index);
                        continue;
                    };
                    cursor.sheet = Some(first);
                    cursor.name_index = 0;
                    first
                }
            };
            let name =
                bridge
                    .scene_render_index
                    .name_at(current_z, current_sheet, cursor.name_index);
            if let Some(name) = name {
                cursor.name_index += 1;
                // Purple performs the nested z/sheet/vector read and the
                // following scene-name map lookup in one uninterrupted native
                // tree walk. There is no host synchronization boundary
                // between them. Resolve the compact live visit while this
                // bridge acquisition is already held, then release it before
                // yielding to either Lua callback.
                if let Some(visit) = bridge.scene_draw_visit(name.as_ref()) {
                    return Some((current_z, Some((name, visit))));
                }
                continue;
            }
            // 0x10004C350..0x10004C354 restores only the two scale
            // members after every SpriteSheet/name vector. Translation,
            // angle, pivot, alpha and clipping remain live for the next
            // z-ordered/pre-draw callback exactly as they do in Purple.
            bridge.state.scale_x = f64::from(cursor.draw_world_scale);
            bridge.state.scale_y = f64::from(cursor.draw_world_scale);
            cursor.sheet = bridge
                .scene_render_index
                .next_sheet(current_z, current_sheet);
            cursor.name_index = 0;
            if cursor.sheet.is_none() {
                cursor.advance_z(&bridge.scene_render_index);
            }
        }
    }
}
