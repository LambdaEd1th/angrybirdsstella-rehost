use crate::*;

impl RenderBridge {
    /// `sub_100062520` stores type-2/type-3 sensor RenderObject pointers on
    /// the opposite object. Only type 2 sets the Lua `insideGravity` flag.
    pub(crate) fn begin_native_sensor_overlap(&mut self, first: &str, second: &str) -> Vec<String> {
        let mut set_inside_gravity = Vec::new();
        for (sensor_name, target_name) in [(first, second), (second, first)] {
            let Some(sensor) = self.scene.get(sensor_name) else {
                continue;
            };
            if !sensor.sensor || !matches!(sensor.sensor_type, 2 | 3) {
                continue;
            }
            self.native_sensor_overlaps
                .entry(target_name.to_owned())
                .or_default()
                .insert(sensor_name.to_owned());
            if sensor.sensor_type == 2 && self.inside_gravity_objects.insert(target_name.to_owned())
            {
                set_inside_gravity.push(target_name.to_owned());
            }
        }
        set_inside_gravity
    }

    /// `sub_10006525C` removes every occurrence of the ending sensor pointer
    /// and clears `insideGravity` only when no type-2/type-3 sensor remains.
    pub(crate) fn end_native_sensor_overlap(&mut self, first: &str, second: &str) -> Vec<String> {
        let mut clear_inside_gravity = Vec::new();
        for (target_name, sensor_name) in [(second, first), (first, second)] {
            let became_empty = self
                .native_sensor_overlaps
                .get_mut(target_name)
                .is_some_and(|sensors| {
                    sensors.remove(sensor_name);
                    sensors.is_empty()
                });
            if became_empty {
                self.native_sensor_overlaps.remove(target_name);
                if self.inside_gravity_objects.remove(target_name) {
                    clear_inside_gravity.push(target_name.to_owned());
                }
            }
        }
        clear_inside_gravity
    }
}
