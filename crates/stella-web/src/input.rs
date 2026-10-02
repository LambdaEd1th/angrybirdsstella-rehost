//! The browser forwards individual native TouchEvents instead of a capped
//! snapshot. Purple's vector and Lua publication have different count rules.

use anyhow::{Result, bail};
use stella_script::StellaLua;

use super::ScriptResultExt;

#[derive(Default)]
pub(super) struct BrowserTouches {
    points: Vec<(u64, i32, i32)>,
}

impl BrowserTouches {
    pub(super) fn event(
        &mut self,
        runtime: &StellaLua,
        phase: i32,
        id: u32,
        x: f64,
        y: f64,
    ) -> Result<()> {
        let point = (u64::from(id), x as i32, y as i32);
        match phase {
            // GameApp slots +0x60/+0x68/+0x70: append, replace the first
            // matching id, and erase all matches while preserving other order.
            0 => self.points.push(point),
            1 => {
                if let Some(index) = self.points.iter().position(|touch| touch.0 == point.0) {
                    self.points[index] = point;
                }
            }
            2 => self.points.retain(|touch| touch.0 != point.0),
            _ => bail!("Invalid touch phase"),
        }
        runtime.set_touches(&self.points).browser()
    }

    // Retain the existing two-entry ABI for earlier host embedders. The new
    // Pointer Events adapter uses event() and never truncates the vector.
    pub(super) fn replace(
        &mut self,
        runtime: &StellaLua,
        points: &[(u64, i32, i32)],
    ) -> Result<()> {
        self.points = points.to_vec();
        runtime.set_touches(&self.points).browser()
    }

    pub(super) fn clear(&mut self) {
        self.points.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{path::PathBuf, time::SystemTime};

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "stella-browser-touch-{}-{nonce}",
                std::process::id()
            ));
            std::fs::create_dir(&root).unwrap();
            std::fs::create_dir(root.join("data")).unwrap();
            Self(root)
        }

        fn runtime(&self) -> StellaLua {
            StellaLua::new(self.0.join("data")).unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn touch_vector_preserves_native_order_duplicate_rules_and_signed_coordinates() {
        let fixture = Fixture::new();
        let runtime = fixture.runtime();
        let mut input = BrowserTouches::default();
        input.event(&runtime, 0, 7, 10.9, -20.9).unwrap();
        input.event(&runtime, 0, 7, 30.5, 40.5).unwrap();
        input.event(&runtime, 0, 8, 50.0, 60.0).unwrap();
        input.event(&runtime, 1, 7, -70.9, 80.9).unwrap();
        input.event(&runtime, 1, 9, 90.0, 100.0).unwrap();
        assert_eq!(input.points, [(7, -70, 80), (7, 30, 40), (8, 50, 60)]);
        input.event(&runtime, 2, 7, 0.0, 0.0).unwrap();
        assert_eq!(input.points, [(8, 50, 60)]);
        assert!(input.event(&runtime, 3, 8, 0.0, 0.0).is_err());
        assert_eq!(input.points, [(8, 50, 60)]);
    }

    #[test]
    fn third_touch_ends_pinch_even_though_lua_still_publishes_two_entries() {
        let fixture = Fixture::new();
        let runtime = fixture.runtime();
        let mut input = BrowserTouches::default();
        runtime.execute_source("zoomDeltas = {}; function applyUserZoom(delta) table.insert(zoomDeltas, delta) end; setWorldScale(2)").unwrap();
        // Clear the process-global native baseline before this simulated app.
        runtime.update(0.0).unwrap();
        input.event(&runtime, 0, 1, 0.0, 0.0).unwrap();
        input.event(&runtime, 0, 2, 3.0, 4.0).unwrap();
        runtime.update(0.0).unwrap();
        input.event(&runtime, 1, 2, 6.0, 8.0).unwrap();
        runtime.update(0.0).unwrap();
        runtime
            .execute_source("assert(#zoomDeltas == 1 and zoomDeltas[1] == 1)")
            .unwrap();

        input.event(&runtime, 0, 3, 40.0, 50.0).unwrap();
        runtime.update(0.0).unwrap();
        input.event(&runtime, 1, 2, 9.0, 12.0).unwrap();
        runtime.update(0.0).unwrap();
        runtime.execute_source("assert(touchcount == 2 and touches['1'] and touches['2'] and not touches['3']); assert(#zoomDeltas == 1)").unwrap();

        // Return to two fingers: the new distance is a fresh baseline, rather
        // than another update against the pre-third-finger distance.
        input.event(&runtime, 2, 3, 40.0, 50.0).unwrap();
        runtime.update(0.0).unwrap();
        runtime.execute_source("assert(#zoomDeltas == 1)").unwrap();
        input.event(&runtime, 1, 2, 18.0, 24.0).unwrap();
        runtime.update(0.0).unwrap();
        runtime
            .execute_source("assert(#zoomDeltas == 2 and zoomDeltas[2] == 1)")
            .unwrap();
        input.replace(&runtime, &[]).unwrap();
        runtime.update(0.0).unwrap();
    }

    #[test]
    fn activation_reset_discards_all_browser_touch_owners() {
        let fixture = Fixture::new();
        let runtime = fixture.runtime();
        let mut input = BrowserTouches::default();
        input.event(&runtime, 0, u32::MAX, -12.75, 2000.25).unwrap();
        runtime.update(0.0).unwrap();
        runtime
            .execute_source("assert(touches['-1'].x == -12 and touches['-1'].y == 2000)")
            .unwrap();
        runtime.set_application_active(false).unwrap();
        input.clear();
        runtime.set_application_active(true).unwrap();
        input.clear();
        input.event(&runtime, 1, u32::MAX, 999.0, 999.0).unwrap();
        runtime.update(0.0).unwrap();
        runtime.execute_source("assert(touchcount == 0)").unwrap();
    }
}
