//! Direct `decomposePolygon` Lua utility (`sub_100035FFC`).

use crate::*;

pub(super) fn install(lua: &Lua, globals: &mlua::Table) -> LuaResult<()> {
    globals.set(
        "decomposePolygon",
        lua.create_function(move |lua, args: MultiValue| {
            // The member consumes its point table directly rather than the
            // createPolygon vertex accumulator.
            let Some(Value::Table(contour)) = args.front() else {
                return Err(runtime_error(
                    "bad argument #1 to 'decomposePolygon' (table expected)",
                ));
            };
            let mut vertices = Vec::with_capacity(contour.raw_len());
            for index in 1..=contour.raw_len() {
                let Value::Table(point) = contour.raw_get::<Value>(index)? else {
                    return Err(runtime_error(format!(
                        "decomposePolygon point #{index} must be table"
                    )));
                };
                vertices.push((
                    f64::from(table_required_number(&point, "x", "decomposePolygon")? as f32),
                    f64::from(table_required_number(&point, "y", "decomposePolygon")? as f32),
                ));
            }

            let result = lua.create_table()?;
            for (polygon_index, polygon) in
                decompose_native_polygon(&vertices).into_iter().enumerate()
            {
                let polygon_table = lua.create_table()?;
                for (point_index, (x, y)) in polygon.into_iter().enumerate() {
                    let point = lua.create_table()?;
                    point.set("x", x)?;
                    point.set("y", y)?;
                    polygon_table.raw_set(point_index + 1, point)?;
                }
                result.raw_set(polygon_index + 1, polygon_table)?;
            }
            Ok(result)
        })?,
    )
}
