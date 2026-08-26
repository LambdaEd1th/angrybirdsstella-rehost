//! Live three-level traversal performed by `sub_10004BAB4`.

use crate::*;

pub(super) struct NativeSceneWalk {
    render: Arc<Mutex<RenderBridge>>,
    cursor: NativeSceneCursor,
}

struct NativeSceneCursor {
    minimum_z: i32,
    maximum_z: i32,
    z: Option<i32>,
    sheet: Option<u64>,
    name_index: usize,
    emit_z: bool,
    finished: bool,
}

impl NativeSceneWalk {
    pub(super) fn new(render: Arc<Mutex<RenderBridge>>, bounds: (i32, i32)) -> Self {
        Self {
            render,
            cursor: NativeSceneCursor {
                minimum_z: bounds.0,
                maximum_z: bounds.1,
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
    type Item = (i32, Option<Arc<str>>);

    fn next(&mut self) -> Option<Self::Item> {
        // One bridge acquisition corresponds to one resumed native tree walk.
        // The lock is still released before yielding to Lua, and the next call
        // re-reads the live vector length exactly like 0x10004C340. Borrowing
        // owner and cursor independently also avoids an atomic Arc retain and
        // release per item: Purple retains GameLua once for the complete walk.
        let bridge = self.render.lock().expect("render bridge lock poisoned");
        let index = &bridge.scene_render_index;
        let cursor = &mut self.cursor;
        loop {
            let Some(current_z) = cursor.z else {
                if cursor.finished {
                    return None;
                }
                if !cursor.advance_z(index) {
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
                    let first = index.first_sheet(current_z);
                    let Some(first) = first else {
                        cursor.advance_z(index);
                        continue;
                    };
                    cursor.sheet = Some(first);
                    cursor.name_index = 0;
                    first
                }
            };
            let name = index.name_at(current_z, current_sheet, cursor.name_index);
            if let Some(name) = name {
                cursor.name_index += 1;
                return Some((current_z, Some(name)));
            }
            cursor.sheet = index.next_sheet(current_z, current_sheet);
            cursor.name_index = 0;
            if cursor.sheet.is_none() {
                cursor.advance_z(index);
            }
        }
    }
}
