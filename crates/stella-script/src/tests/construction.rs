use super::*;
use mlua::{Table, Value};

fn sorted_keys(table: &Table) -> Vec<String> {
    let mut keys = table
        .clone()
        .pairs::<String, Value>()
        .map(|pair| pair.unwrap().0)
        .collect::<Vec<_>>();
    keys.sort();
    keys
}

fn expected_keys(shape_fields: &[&str]) -> Vec<String> {
    let mut keys = [
        "alpha",
        "angle",
        "animFrame",
        "animThresholdTimer",
        "animTimer",
        "collisionEnabled",
        "density",
        "friction",
        "mass",
        "name",
        "restitution",
        "sprite",
        "type",
        "x",
        "xVel",
        "y",
        "yVel",
        "z_order",
    ]
    .into_iter()
    .chain(shape_fields.iter().copied())
    .map(str::to_owned)
    .collect::<Vec<_>>();
    keys.sort();
    keys
}

#[test]
fn constructors_publish_only_the_native_objects_world_fields() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("box", "BOX", 1.25, -2.5, 2.25, 3.5,
                    1.75, 0.125, 0.375, true, false, 4.75)
                createCircle("circle", "CIRCLE", 2.25, -3.5, 1.125,
                    2.25, 0.25, 0.5, false, true, 5.75)

                clearVertices()
                addVertex(-1.25, -0.5)
                addVertex(1.75, -0.5)
                addVertex(0.25, 2.5)
                createPolygon("polygon", "POLYGON", 3.25, -4.5, 7.125, 8.25,
                    1.5, 0.375, 0.625, true, false, 6.75)

                clearVertices()
                addVertex(-2.5, 0.25)
                addVertex(3.5, -0.75)
                createLineShape("line", "LINE", 4.25, -5.5, 9.125, 10.25,
                    2.0, 0.5, 0.75, false, false, 7.75)
                createNonPhysicsObject("none", "NONE", 5.25, -6.5, 8.75)
                "#,
        )
        .unwrap();

    let world = object_world(runtime.lua()).unwrap();
    let box_entry = world.get::<Table>("box").unwrap();
    let circle = world.get::<Table>("circle").unwrap();
    let polygon = world.get::<Table>("polygon").unwrap();
    let line = world.get::<Table>("line").unwrap();
    let none = world.get::<Table>("none").unwrap();

    assert_eq!(sorted_keys(&box_entry), expected_keys(&["width", "height"]));
    assert_eq!(sorted_keys(&circle), expected_keys(&["radius"]));
    assert_eq!(sorted_keys(&polygon), expected_keys(&["width", "height"]));
    assert_eq!(sorted_keys(&line), expected_keys(&["width", "height"]));
    assert_eq!(sorted_keys(&none), expected_keys(&[]));

    assert_eq!(box_entry.get::<String>("type").unwrap(), "box");
    assert_eq!(circle.get::<String>("type").unwrap(), "circle");
    assert_eq!(polygon.get::<String>("type").unwrap(), "polygon");
    assert_eq!(line.get::<String>("type").unwrap(), "line");
    assert_eq!(none.get::<String>("type").unwrap(), "none");

    assert_eq!(line.get::<f64>("width").unwrap(), f64::from(9.125_f32));
    assert_eq!(line.get::<f64>("height").unwrap(), f64::from(10.25_f32));
    assert_eq!(polygon.get::<f64>("width").unwrap(), f64::from(7.125_f32));
    assert_eq!(polygon.get::<f64>("height").unwrap(), f64::from(8.25_f32));
    assert_eq!(none.get::<f64>("friction").unwrap(), 0.0);
    assert!(!none.get::<bool>("collisionEnabled").unwrap());

    for entry in [&box_entry, &circle, &polygon, &line, &none] {
        assert_eq!(entry.get::<f64>("angle").unwrap(), 0.0);
        assert_eq!(entry.get::<f64>("xVel").unwrap(), 0.0);
        assert_eq!(entry.get::<f64>("yVel").unwrap(), 0.0);
        assert_eq!(entry.get::<f64>("animTimer").unwrap(), 0.0);
        assert_eq!(entry.get::<f64>("animFrame").unwrap(), 1.0);
        assert_eq!(entry.get::<f64>("animThresholdTimer").unwrap(), 0.0);
        assert_eq!(entry.get::<f64>("alpha").unwrap(), 1.0);
    }

    let bridge = runtime.render.lock().unwrap();
    for name in ["box", "circle", "polygon", "line", "none"] {
        assert_eq!(
            world
                .get::<Table>(name)
                .unwrap()
                .get::<f64>("mass")
                .unwrap(),
            f64::from(bridge.scene[name].body_mass),
            "{name}"
        );
    }
    assert_eq!(bridge.scene["none"].friction, 0.0);
}
