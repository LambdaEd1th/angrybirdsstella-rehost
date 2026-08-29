use super::*;

#[test]
fn polygon_max_separation_uses_native_centroid_seeded_directional_search() {
    // This valid convex pair has two local separation maxima. A full SAT
    // scan picks edge 2, but sub_10085FB84 seeds from the centroid direction
    // and climbs toward edge 4 without crossing the intervening minimum.
    let reference = [
        (0.967_828_45, 1.182_618_5),
        (0.532_282_1, 2.614_938_5),
        (-0.964_526_4, 2.643_320_6),
        (-1.454_058_5, 1.228_541_5),
        (-0.259_797_54, 0.325_778_04),
    ];
    let incident = [
        (2.444_748_4, -8.019_812),
        (6.982_384, 6.576_342),
        (-7.927_074, 3.207_973_2),
    ];
    let (separation, edge, _) = polygon_max_separation(&reference, &incident).unwrap();
    assert_eq!(edge, 4);
    assert_eq!(separation, f64::from(-6.751_747_6_f32));
}

#[test]
fn polygon_reference_face_uses_native_relative_and_absolute_tolerance() {
    // This shallow rotated overlap lies in the interval where the old
    // ad-hoc +0.0001 comparison selected polygon B, while
    // sub_10085F648's 0.98f*A+0.001f comparison retains polygon A.
    let first = [
        (-0.9645613337590933, -0.6554551345568048),
        (1.0322539816720482, -0.5426340547018801),
        (0.9645613337590933, 0.6554551345568048),
        (-1.0322539816720482, 0.5426340547018801),
    ];
    let second = [
        (-0.9494428060021692, 0.55545994581184),
        (1.0476783075740406, 0.6627318326607218),
        (0.9833151754647115, 1.8610045008064477),
        (-1.0138059381114983, 1.753732613957566),
    ];
    let (separation_a, _, _) = polygon_max_separation(&first, &second).unwrap();
    let (separation_b, _, _) = polygon_max_separation(&second, &first).unwrap();
    assert!(separation_b as f32 > separation_a as f32 + 0.0001_f32);
    assert!(separation_b as f32 <= (separation_a as f32).mul_add(0.98_f32, 0.001_f32));
    let manifold = polygon_manifold(&first, &second).expect("shallow polygon contact");
    assert!(matches!(
        manifold.manifold_type(),
        ContactManifoldType::FaceFirst
    ));
}

#[test]
fn polygon_manifold_final_filter_keeps_unordered_separations() {
    let unordered = [(f64::NAN, 0.0), (f64::NAN, 1.0), (f64::NAN, 2.0)];
    let finite = [(-1.0, -1.0), (1.0, -1.0), (0.0, 1.0)];

    let manifold = polygon_manifold(&unordered, &finite)
        .expect("native B.LE/B.GT final filters retain unordered points");

    assert!(manifold.penetration.is_nan());
    assert_eq!(manifold.position.point_count, 2);
}

#[test]
fn edge_polygon_final_manifold_filter_keeps_unordered_separations() {
    let unordered = [(f64::NAN, 0.0), (f64::NAN, 1.0), (f64::NAN, 2.0)];

    let manifold = polygon_segment_manifold(&unordered, ((-1.0, 0.0), (1.0, 0.0)), true)
        .expect("native B.LE/B.GT edge-polygon filters retain unordered points");

    assert!(manifold.penetration.is_nan());
    assert_eq!(manifold.position.point_count, 2);
}

#[test]
fn rotated_polygon_pair_keeps_native_reference_and_incident_local_points() {
    let first = [
        (-1.0_f32, -0.5_f32),
        (1.0_f32, -0.5_f32),
        (1.0_f32, 0.5_f32),
        (-1.0_f32, 0.5_f32),
    ];
    let second = first;
    let angle = 0.37_f32;
    let (sine, cosine) = angle.sin_cos();
    let first_transform = NativeToiTransform {
        position: (12.25, -7.5),
        sine,
        cosine,
    };
    let second_transform = NativeToiTransform {
        position: first_transform.point((0.0, 0.9)),
        sine,
        cosine,
    };
    let manifold =
        polygon_manifold_at_transforms(&first, first_transform, &second, second_transform)
            .expect("rotated polygon face contact");
    let local = manifold.position;
    assert!(matches!(
        local.manifold_type,
        ContactManifoldType::FaceFirst
    ));
    assert_eq!(local.local_normal.0.to_bits(), 0.0_f32.to_bits());
    assert_eq!(local.local_normal.1.to_bits(), 1.0_f32.to_bits());
    assert_eq!(local.local_point.0.to_bits(), 0.0_f32.to_bits());
    assert_eq!(local.local_point.1.to_bits(), 0.5_f32.to_bits());
    assert_eq!(local.point_count, 2);
    let expected_first = second_transform.inverse_point(second_transform.point(second[0]));
    let expected_second = second_transform.inverse_point(second_transform.point(second[1]));
    assert_eq!(
        local.local_points[0].0.to_bits(),
        expected_first.0.to_bits()
    );
    assert_eq!(
        local.local_points[0].1.to_bits(),
        expected_first.1.to_bits()
    );
    assert_eq!(
        local.local_points[1].0.to_bits(),
        expected_second.0.to_bits()
    );
    assert_eq!(
        local.local_points[1].1.to_bits(),
        expected_second.1.to_bits()
    );
    let refreshed = manifold.at_native_transforms(first_transform, second_transform);
    assert_eq!(manifold.normal_x.to_bits(), refreshed.normal_x.to_bits());
    assert_eq!(manifold.normal_y.to_bits(), refreshed.normal_y.to_bits());
    assert_eq!((manifold.penetration as f32).to_bits(), 0x3DD4_FE2D);
    assert_eq!(
        manifold.penetration.to_bits(),
        refreshed.penetration.to_bits()
    );
    assert_eq!(manifold.point_x.to_bits(), refreshed.point_x.to_bits());
    assert_eq!(manifold.point_y.to_bits(), refreshed.point_y.to_bits());
    let initial_second = manifold.secondary.expect("initial second point");
    let refreshed_second = refreshed.secondary.expect("refreshed second point");
    assert_eq!(
        initial_second.penetration.to_bits(),
        refreshed_second.penetration.to_bits()
    );
    assert_eq!(
        initial_second.point_x.to_bits(),
        refreshed_second.point_x.to_bits()
    );
    assert_eq!(
        initial_second.point_y.to_bits(),
        refreshed_second.point_y.to_bits()
    );
}

#[test]
fn rotated_polygon_pair_rebuilds_native_face_second_world_manifold() {
    let first = [
        (-1.0_f32, -0.5_f32),
        (1.0_f32, -0.5_f32),
        (1.0_f32, 0.5_f32),
        (-1.0_f32, 0.5_f32),
    ];
    let second = [
        (-0.8_f32, -0.45_f32),
        (0.8_f32, -0.45_f32),
        (0.8_f32, 0.45_f32),
        (-0.8_f32, 0.45_f32),
    ];
    let global_angle = -0.29_f32;
    let relative_angle = 0.5_f32;
    let (global_sine, global_cosine) = global_angle.sin_cos();
    let (first_sine, first_cosine) = (global_angle + relative_angle).sin_cos();
    let global_transform = NativeToiTransform {
        position: (-8.75, 3.125),
        sine: global_sine,
        cosine: global_cosine,
    };
    let first_transform = NativeToiTransform {
        position: global_transform.position,
        sine: first_sine,
        cosine: first_cosine,
    };
    let second_transform = NativeToiTransform {
        position: global_transform.point((0.0, 0.9)),
        sine: global_sine,
        cosine: global_cosine,
    };
    let manifold =
        polygon_manifold_at_transforms(&first, first_transform, &second, second_transform)
            .expect("rotated FaceB polygon contact");
    assert!(matches!(
        manifold.position.manifold_type,
        ContactManifoldType::FaceSecond
    ));
    assert_eq!((manifold.penetration as f32).to_bits(), 0x3EF1_C667);
    let refreshed = manifold.at_native_transforms(first_transform, second_transform);
    assert_eq!(manifold.normal_x.to_bits(), refreshed.normal_x.to_bits());
    assert_eq!(manifold.normal_y.to_bits(), refreshed.normal_y.to_bits());
    assert_eq!(
        manifold.penetration.to_bits(),
        refreshed.penetration.to_bits()
    );
    assert_eq!(manifold.point_x.to_bits(), refreshed.point_x.to_bits());
    assert_eq!(manifold.point_y.to_bits(), refreshed.point_y.to_bits());
    assert_eq!(manifold.position.point_count, 1);
    assert!(manifold.secondary.is_none());
    assert!(refreshed.secondary.is_none());
}

#[test]
fn polygon_circle_uses_native_float_boundary_and_zero_feature_id() {
    let polygon = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    let radius = 0.5_f32;
    let total_radius = radius + BOX2D_POLYGON_RADIUS as f32;
    let center = (0.0, f64::from(1.0_f32 + total_radius));
    let manifold = circle_polygon_manifold(center, f64::from(radius), &polygon, false)
        .expect("native <= radius boundary contact");
    assert!(matches!(
        manifold.manifold_type(),
        ContactManifoldType::FaceFirst
    ));
    assert_eq!(manifold.feature_id, 0);
    assert_eq!((manifold.normal_x, manifold.normal_y), (0.0, 1.0));
    assert_eq!(manifold.penetration, 0.0);

    let outside = (0.0, f64::from((1.0_f32 + total_radius).next_up()));
    assert!(circle_polygon_manifold(outside, f64::from(radius), &polygon, false).is_none());
}

#[test]
fn polygon_circle_does_not_repair_clockwise_setter_normals() {
    let clockwise = [(-1.0, -1.0), (-1.0, 1.0), (1.0, 1.0), (1.0, -1.0)];

    assert!(circle_polygon_manifold((1.5, 0.0), 0.5, &clockwise, false).is_none());
}

#[test]
fn polygon_circle_retains_a_duplicate_setter_edge() {
    let polygon = [
        (-1.0, -1.0),
        (1.0, -1.0),
        (1.0, -1.0),
        (1.0, 1.0),
        (-1.0, 1.0),
    ];
    let manifold = circle_polygon_manifold((0.0, 1.5), 0.5, &polygon, false)
        .expect("duplicate edge remains in the native stored-normal walk");

    assert_eq!((manifold.normal_x, manifold.normal_y), (0.0, 1.0));
}

#[test]
fn polygon_circle_unordered_face_scan_keeps_the_seed_normal() {
    let polygon = [(f64::NAN, 0.0), (f64::NAN, 1.0), (f64::NAN, 2.0)];

    let manifold = circle_polygon_manifold((0.0, 0.0), 0.5, &polygon, false)
        .expect("native FMAX/LT flow keeps the unordered face manifold");
    let local = manifold.position;

    assert_eq!(local.local_normal.0.to_bits(), 1.0_f32.to_bits());
    assert!(local.local_normal.1.is_nan());
    assert!(manifold.normal_y.is_nan());
    assert!(manifold.penetration.is_nan());
}

#[test]
fn polygon_circle_vertex_normal_multiplies_one_native_reciprocal() {
    let polygon = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
    let center = (
        f64::from(-f32::from_bits(0x3F2F_4093)),
        f64::from(-f32::from_bits(0x3F67_1388)),
    );

    let manifold = circle_polygon_manifold(center, 1.2, &polygon, false)
        .expect("lower-left vertex remains inside the combined radius");

    assert_eq!(manifold.position.local_normal.0.to_bits(), 0xBF1A_B254);
    assert_eq!(manifold.position.local_normal.1.to_bits(), 0xBF4B_F913);
    assert_ne!(
        manifold.position.local_normal.0.to_bits(),
        (center.0 as f32 / f32::from_bits(0x3F91_021E)).to_bits()
    );
}

#[test]
fn rotated_polygon_circle_keeps_shape_local_normal_and_reference_point() {
    let polygon = [
        (-0.7_f32, -0.4_f32),
        (1.3_f32, -0.4_f32),
        (1.3_f32, 0.6_f32),
        (-0.7_f32, 0.6_f32),
    ];
    let angle = 0.37_f32;
    let (sine, cosine) = angle.sin_cos();
    let polygon_transform = NativeToiTransform {
        position: (12.25, -7.5),
        sine,
        cosine,
    };
    let circle_world_center = polygon_transform.point((0.2, 1.1));
    let manifold = circle_polygon_manifold_at_transforms(
        (0.0, 0.0),
        0.5,
        NativeToiTransform {
            position: circle_world_center,
            sine: 0.0,
            cosine: 1.0,
        },
        &polygon,
        polygon_transform,
        false,
    )
    .expect("rotated polygon face contact");
    let local = manifold.position;
    let expected_plane_x = (polygon[2].0 + polygon[3].0) * 0.5;
    let expected_plane_y = (polygon[2].1 + polygon[3].1) * 0.5;
    assert!(matches!(
        local.manifold_type,
        ContactManifoldType::FaceFirst
    ));
    assert_eq!(local.local_normal.0.to_bits(), 0.0_f32.to_bits());
    assert_eq!(local.local_normal.1.to_bits(), 1.0_f32.to_bits());
    assert_eq!(local.local_point.0.to_bits(), expected_plane_x.to_bits());
    assert_eq!(local.local_point.1.to_bits(), expected_plane_y.to_bits());
    assert_eq!(local.local_points[0], (0.0, 0.0));
}

#[test]
fn circle_circle_uses_native_inclusive_boundary_and_small_delta_axis() {
    let touching = circle_circle_manifold((0.0, 0.0), 1.0, (2.0, 0.0), 1.0)
        .expect("native squared-radius boundary contact");
    assert_eq!(touching.feature_id, 0);
    assert_eq!(touching.penetration, 0.0);
    assert_eq!((touching.point_x, touching.point_y), (1.0, 0.0));
    assert!(
        circle_circle_manifold((0.0, 0.0), 1.0, (f64::from(2.0_f32.next_up()), 0.0), 1.0,)
            .is_none()
    );

    let tiny_y = f32::EPSILON * 0.5_f32;
    let near = circle_circle_manifold((0.0, 0.0), 1.0, (0.0, f64::from(tiny_y)), 1.0)
        .expect("sub-epsilon circle contact");
    assert_eq!((near.normal_x, near.normal_y), (1.0, 0.0));
    assert_eq!(near.penetration, 2.0);
    assert_eq!(near.point_y, f64::from(tiny_y * 0.5_f32));
}

#[test]
fn edge_circle_uses_native_regions_features_and_inclusive_radius() {
    let edge = ((-1.0, 0.0), (1.0, 0.0));
    let radius = 0.5_f32;
    let combined = radius + BOX2D_POLYGON_RADIUS as f32;

    let face = circle_segment_manifold((0.0, f64::from(combined)), f64::from(radius), edge, false)
        .expect("native edge face boundary contact");
    assert_eq!((face.normal_x, face.normal_y), (0.0, 1.0));
    // b2WorldManifold independently builds both skin surfaces before taking
    // their projected separation, leaving this exact quarter-epsilon word.
    assert_eq!(
        (face.penetration as f32).to_bits(),
        (f32::EPSILON * 0.25_f32).to_bits()
    );
    assert_eq!(face.feature_id, contact_feature_id(0, 0, 1, 0));

    let vertex = circle_segment_manifold(
        (f64::from(-1.0_f32 - combined), 0.0),
        f64::from(radius),
        edge,
        false,
    )
    .expect("native edge vertex boundary contact");
    assert!(matches!(
        vertex.manifold_type(),
        ContactManifoldType::Circles
    ));
    assert_eq!((vertex.normal_x, vertex.normal_y), (-1.0, 0.0));
    assert_eq!(vertex.penetration, 0.0);
    assert_eq!(vertex.feature_id, contact_feature_id(0, 0, 0, 0));

    let reversed =
        circle_segment_manifold((0.0, f64::from(combined)), f64::from(radius), edge, true)
            .expect("circle-first edge contact");
    assert_eq!((reversed.normal_x, reversed.normal_y), (0.0, -1.0));
    assert_eq!(reversed.feature_id, contact_feature_id(0, 0, 0, 1));
}

#[test]
fn degenerate_edge_circle_uses_the_first_endpoint_region() {
    let radius = 0.5_f32;
    let combined = radius + BOX2D_POLYGON_RADIUS as f32;
    let point = (2.0_f64, -3.0_f64);
    let manifold = circle_segment_manifold(
        (f64::from(2.0_f32 + combined), -3.0),
        f64::from(radius),
        (point, point),
        false,
    )
    .expect("zero-length b2EdgeShape endpoint contact");

    assert!(matches!(
        manifold.manifold_type(),
        ContactManifoldType::Circles
    ));
    assert_eq!((manifold.normal_x, manifold.normal_y), (1.0, 0.0));
    // The local endpoint reconstruction loses one epsilon and the subsequent
    // b2WorldManifold surface projection exposes a second one.
    assert_eq!(manifold.penetration, f64::from(2.0_f32 * f32::EPSILON));
    assert_eq!(manifold.feature_id, contact_feature_id(0, 0, 0, 0));
}

#[test]
fn edge_circle_arm_le_routes_unordered_projection_to_the_first_endpoint() {
    let manifold = circle_segment_manifold((f64::NAN, 0.0), 0.5, ((0.0, 0.0), (1.0, 0.0)), false)
        .expect("ARM B.LE accepts the unordered endpoint region");

    assert!(matches!(
        manifold.manifold_type(),
        ContactManifoldType::Circles
    ));
    assert_eq!(manifold.feature_id, contact_feature_id(0, 0, 0, 0));
}

#[test]
fn edge_circle_unordered_face_side_follows_native_not_ge_path() {
    let huge = f32::MAX;
    let half = huge * 0.5_f32;
    let manifold = circle_segment_manifold(
        (f64::from(half), f64::from(half)),
        0.5,
        ((0.0, 0.0), (f64::from(huge), f64::from(huge))),
        false,
    )
    .expect("overflowed cross product remains a native face manifold");

    assert!(matches!(
        manifold.manifold_type(),
        ContactManifoldType::FaceFirst
    ));
    assert_eq!(
        manifold.position.local_normal.0.to_bits(),
        0.0_f32.to_bits()
    );
    assert_eq!(
        manifold.position.local_normal.1.to_bits(),
        (-0.0_f32).to_bits()
    );
}

#[test]
fn sub_epsilon_edge_circle_face_is_not_rejected_before_native_division() {
    let edge_length = f32::EPSILON * 0.5_f32;
    let radius = 0.5_f32;
    let combined = radius + BOX2D_POLYGON_RADIUS as f32;
    let manifold = circle_segment_manifold(
        (f64::from(edge_length * 0.5_f32), f64::from(combined)),
        f64::from(radius),
        ((0.0, 0.0), (f64::from(edge_length), 0.0)),
        false,
    )
    .expect("native face branch divides by a sub-epsilon edge length");

    assert!(matches!(
        manifold.manifold_type(),
        ContactManifoldType::FaceFirst
    ));
    assert_eq!(manifold.normal_x.to_bits(), f64::from(-0.0_f32).to_bits());
    assert_eq!(manifold.normal_y, f64::from(edge_length));
    // The retained non-unit face normal is used again by b2WorldManifold;
    // both projected surface deltas underflow to a signed zero separation.
    assert_eq!(
        manifold.penetration.to_bits(),
        f64::from(-0.0_f32).to_bits()
    );
    assert_eq!(manifold.feature_id, contact_feature_id(0, 0, 1, 0));
}

#[test]
fn rotated_edge_circle_face_keeps_native_shape_local_witnesses() {
    let segment = ((-1.0_f32, 0.0_f32), (1.0_f32, 0.0_f32));
    let radius = 0.5_f32;
    let combined = radius + BOX2D_POLYGON_RADIUS as f32;
    let angle = 0.37_f32;
    let (sine, cosine) = angle.sin_cos();
    let segment_transform = NativeToiTransform {
        position: (12.25, -7.5),
        sine,
        cosine,
    };
    let circle_local_center = (0.25_f32, -0.125_f32);
    let desired_edge_center = segment_transform.rotate((0.0, combined - 0.001));
    let desired_center = (
        desired_edge_center.0 + segment_transform.position.0,
        desired_edge_center.1 + segment_transform.position.1,
    );
    let rotated_center = segment_transform.rotate(circle_local_center);
    let circle_transform = NativeToiTransform {
        position: (
            desired_center.0 - rotated_center.0,
            desired_center.1 - rotated_center.1,
        ),
        sine,
        cosine,
    };
    let manifold = circle_segment_manifold_at_transforms(
        circle_local_center,
        radius,
        circle_transform,
        segment,
        segment_transform,
        false,
    )
    .expect("rotated edge face contact");
    let local = manifold.position;
    assert!(matches!(
        local.manifold_type,
        ContactManifoldType::FaceFirst
    ));
    assert_eq!(local.local_normal.0.to_bits(), (-0.0_f32).to_bits());
    assert_eq!(local.local_normal.1.to_bits(), 1.0_f32.to_bits());
    assert_eq!(local.local_point, segment.0);
    assert_eq!(local.local_points[0], circle_local_center);
    let expected_normal = segment_transform.rotate((-0.0, 1.0));
    assert_eq!(manifold.normal_x, f64::from(expected_normal.0));
    assert_eq!(manifold.normal_y, f64::from(expected_normal.1));
    let refreshed = manifold.at_native_transforms(segment_transform, circle_transform);
    assert_eq!(manifold.normal_x.to_bits(), refreshed.normal_x.to_bits());
    assert_eq!(manifold.normal_y.to_bits(), refreshed.normal_y.to_bits());
    assert_eq!((manifold.penetration as f32).to_bits(), 0x3A83_0CDF);
    assert_eq!(
        manifold.penetration.to_bits(),
        refreshed.penetration.to_bits()
    );
    assert_eq!(manifold.point_x.to_bits(), refreshed.point_x.to_bits());
    assert_eq!(manifold.point_y.to_bits(), refreshed.point_y.to_bits());
}

#[test]
fn rotated_circle_edge_endpoint_keeps_native_shape_local_witnesses() {
    let segment = ((-1.0_f32, 0.0_f32), (1.0_f32, 0.0_f32));
    let radius = 0.5_f32;
    let combined = radius + BOX2D_POLYGON_RADIUS as f32;
    let angle = -0.61_f32;
    let (sine, cosine) = angle.sin_cos();
    let segment_transform = NativeToiTransform {
        position: (-4.5, 9.75),
        sine,
        cosine,
    };
    let circle_local_center = (-0.375_f32, 0.125_f32);
    let desired_edge_center = segment_transform.rotate((-1.0 - combined + 0.001, 0.0));
    let desired_center = (
        desired_edge_center.0 + segment_transform.position.0,
        desired_edge_center.1 + segment_transform.position.1,
    );
    let rotated_center = segment_transform.rotate(circle_local_center);
    let circle_transform = NativeToiTransform {
        position: (
            desired_center.0 - rotated_center.0,
            desired_center.1 - rotated_center.1,
        ),
        sine,
        cosine,
    };
    let manifold = circle_segment_manifold_at_transforms(
        circle_local_center,
        radius,
        circle_transform,
        segment,
        segment_transform,
        true,
    )
    .expect("rotated circle/edge endpoint contact");
    let local = manifold.position;
    assert!(matches!(local.manifold_type, ContactManifoldType::Circles));
    assert_eq!(local.local_point, circle_local_center);
    assert_eq!(local.local_points[0], segment.0);
    assert_eq!(local.first_radius.to_bits(), radius.to_bits());
    assert_eq!(
        local.second_radius.to_bits(),
        (BOX2D_POLYGON_RADIUS as f32).to_bits()
    );
    let locally_rotated_axis = segment_transform.rotate((1.0, 0.0));
    assert_ne!(
        (manifold.normal_x as f32).to_bits(),
        locally_rotated_axis.0.to_bits()
    );
    assert_eq!((manifold.normal_x as f32).to_bits(), 0x3F51_D476);
    assert_eq!((manifold.normal_y as f32).to_bits(), 0xBF12_A76D);
    assert_eq!(manifold.feature_id, contact_feature_id(0, 0, 0, 0));
    let refreshed = manifold.at_native_transforms(circle_transform, segment_transform);
    assert_eq!(manifold.normal_x.to_bits(), refreshed.normal_x.to_bits());
    assert_eq!(manifold.normal_y.to_bits(), refreshed.normal_y.to_bits());
    assert_eq!(
        manifold.penetration.to_bits(),
        refreshed.penetration.to_bits()
    );
    assert_eq!(manifold.point_x.to_bits(), refreshed.point_x.to_bits());
    assert_eq!(manifold.point_y.to_bits(), refreshed.point_y.to_bits());
}

#[test]
fn edge_circle_face_distance_uses_native_fused_boundary_sequence() {
    let edge = (
        (f64::from(-11.723_374_f32), f64::from(21.018_469_f32)),
        (f64::from(-10.910_686_5_f32), f64::from(-7.053_440_6_f32)),
    );
    let center = (f64::from(-11.130_099_f32), f64::from(9.055_822_f32));
    let radius = f64::from(0.244_851_32_f32);

    // The former closest-point expression rounds distance² to
    // 0.0609355718 and fabricates a boundary contact. sub_10085E8AC's
    // FNMADD/FMADD sequence yields 0.0609355979, just outside the squared
    // combined radius 0.0609355755.
    assert!(circle_segment_manifold(center, radius, edge, false).is_none());
}

#[test]
fn recovered_two_point_manifold_and_block_solver_balance_face_contact() {
    let first = [(-0.5, 0.25), (0.5, 0.25), (0.5, 1.25), (-0.5, 1.25)];
    let second = [(0.1, -1.0), (1.1, -1.0), (1.1, 1.0), (0.1, 1.0)];
    let manifold = polygon_manifold(&first, &second).expect("overlapping box manifold");
    assert!(manifold.secondary.is_some());
    assert!((manifold.normal_x - 1.0).abs() < 1e-9);
    let feature_ids = [manifold.feature_id, manifold.secondary.unwrap().feature_id];
    assert_eq!(
        feature_ids,
        [
            contact_feature_id(1, 3, 1, 0),
            contact_feature_id(1, 3, 0, 1),
        ]
    );

    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("mover", "", 0, 0.75, 1, 1, 1, 0, 0, true, false, 1)
                createBox("wall", "", 0.6, 0, 1, 2, 0, 0, 0, true, false, 1)
                setVelocity("mover", 6, 0)
                "#,
        )
        .unwrap();
    let mut bridge = runtime.render.lock().unwrap();
    bridge.solve_contacts();
    let pair = ("mover".to_owned(), "wall".to_owned(), 0, 0);
    let constraint = bridge.contact_velocity_constraints[&pair];
    assert_eq!(constraint.point_count, 2);
    assert!(constraint.normal_k.0 > 0.0);
    assert!(constraint.normal_k.2 > 0.0);
    assert!(
        [
            constraint.points[0].first_radius.0,
            constraint.points[0].first_radius.1,
            constraint.points[0].second_radius.0,
            constraint.points[0].second_radius.1,
            constraint.points[0].normal_mass,
            constraint.points[0].tangent_mass,
            constraint.points[0].velocity_bias,
            constraint.points[1].first_radius.0,
            constraint.points[1].first_radius.1,
            constraint.points[1].second_radius.0,
            constraint.points[1].second_radius.1,
            constraint.points[1].normal_mass,
            constraint.points[1].tangent_mass,
            constraint.points[1].velocity_bias,
            constraint.normal_mass.0,
            constraint.normal_mass.1,
            constraint.normal_mass.2,
            constraint.normal_k.0,
            constraint.normal_k.1,
            constraint.normal_k.2,
        ]
        .into_iter()
        .all(f32::is_finite)
    );
    let mover = &bridge.scene["mover"];
    assert!(mover.velocity_x < 6.0);
    assert!(
        mover.angular_velocity.abs() < 1e-9,
        "angular_velocity={}",
        mover.angular_velocity
    );
    let impulse = bridge
        .contact_impulses
        .get(&pair)
        .expect("cached face impulse");
    assert!(impulse.normal > 0.0);
    assert!(impulse.secondary_normal > 0.0);
}

#[test]
fn recovered_edge_polygon_manifold_clips_two_face_points() {
    let polygon = [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)];
    let manifold = polygon_segment_manifold(&polygon, ((-2.0, 0.501), (2.0, 0.501)), true)
        .expect("box/edge manifold");
    let second = manifold.secondary.expect("second clipped edge point");
    assert!(matches!(
        manifold.manifold_type(),
        ContactManifoldType::FaceSecond
    ));
    assert!((manifold.normal_y - 1.0).abs() < 1e-9);
    assert_eq!(
        [manifold.feature_id, second.feature_id],
        [
            contact_feature_id(2, 0, 0, 1),
            contact_feature_id(3, 0, 0, 1),
        ]
    );
    let mut contact_x = [manifold.point_x, second.point_x];
    contact_x.sort_by(f64::total_cmp);
    assert!((contact_x[0] + 0.5).abs() < 1e-9);
    assert!((contact_x[1] - 0.5).abs() < 1e-9);

    let total_radius = (2.0 * BOX2D_POLYGON_RADIUS) as f32;
    let boundary_polygon = [(-0.5, -1.0), (0.5, -1.0), (0.5, 0.0), (-0.5, 0.0)];
    let touching = polygon_segment_manifold(
        &boundary_polygon,
        (
            (-2.0, f64::from(total_radius)),
            (2.0, f64::from(total_radius)),
        ),
        true,
    )
    .expect("native edge-polygon <= total-radius boundary");
    assert_eq!(touching.penetration, 0.0);
    assert!(
        polygon_segment_manifold(
            &boundary_polygon,
            (
                (-2.0, f64::from(total_radius.next_up())),
                (2.0, f64::from(total_radius.next_up())),
            ),
            true,
        )
        .is_none()
    );

    let polygon_axis = polygon_segment_manifold(&polygon, ((0.502, 0.502), (0.504, 0.504)), true)
        .expect("polygon-primary corner contact");
    assert!(matches!(
        polygon_axis.manifold_type(),
        ContactManifoldType::FaceFirst
    ));
    assert_eq!((polygon_axis.normal_x, polygon_axis.normal_y), (1.0, 0.0));
}

#[test]
fn degenerate_edge_polygon_preserves_zero_tangent_and_two_contacts() {
    let polygon = [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)];
    let manifold = polygon_segment_manifold(&polygon, ((0.0, 0.0), (0.0, 0.0)), true)
        .expect("zero-length b2EdgeShape/polygon manifold");

    assert!(matches!(
        manifold.manifold_type(),
        ContactManifoldType::FaceSecond
    ));
    assert_eq!(manifold.position.point_count, 2);
    assert_eq!(manifold.normal_x, 0.0);
    assert_eq!(manifold.normal_y, 0.0);
    // b2WorldManifold reprojects the retained zero face normal. Both surface
    // offsets collapse to zero lanes, so the final negated separation is -0.
    assert_eq!(
        manifold.penetration.to_bits(),
        f64::from(-0.0_f32).to_bits()
    );
    assert!(manifold.secondary.is_some());
}

#[test]
fn narrow_phase_rejects_aabb_only_circle_corner_overlap() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createBox("box", "", 0, 0, 2, 2, 0, 0, 0, true, false, 1)
                createCircle("circle", "", 1.8, 1.8, 1, 1, 0, 0, true, false, 1)
                setVelocity("circle", 0.1, 0)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    let box_aabb = bridge.scene["box"].collision_aabb().unwrap();
    let circle_aabb = bridge.scene["circle"].collision_aabb().unwrap();
    assert!(box_aabb.2 > circle_aabb.0 && box_aabb.3 > circle_aabb.1);
    assert!(bridge.solve_contacts().is_empty());
    assert!(bridge.active_contacts.is_empty());
}

#[test]
fn recovered_edge_fixtures_are_independent_two_sided_capsules() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(-3, 0)
                addVertex(0, 0)
                addVertex(3, 0)
                createLineShape("ground", "", 0, 0, 6, 0, 0, 0, 0, true, false, 1)
                createCircle("above", "", -1, 0.9, 1, 1, 0, 0, true, false, 1)
                createCircle("below", "", 1, -0.9, 1, 1, 0, 0, true, false, 1)
                setVelocity("above", 0, -0.5)
                setVelocity("below", 0, 0.5)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    assert_eq!(bridge.scene["ground"].collision_segments().len(), 2);
    let events = bridge.solve_contacts();
    assert!(events.iter().filter(|event| event.impulse > 0.0).count() >= 2);
    assert!(bridge.scene["above"].velocity_y.abs() < f64::from(f32::EPSILON));
    assert!(bridge.scene["below"].velocity_y.abs() < f64::from(f32::EPSILON));
}

#[test]
fn degenerate_create_line_fixture_reaches_native_contact_dispatch() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(0, 0)
                addVertex(0, 0)
                createLineShape("edge", "", 0, 0, 0, 0, 0, 0, 0, true, false, 1)
                createCircle("circle", "", 0.5, 0, 0.5, 1, 0, 0, true, false, 1)
                "#,
        )
        .unwrap();

    let bridge = runtime.render.lock().unwrap();
    let edge = &bridge.scene["edge"];
    assert_eq!(edge.collision_segments(), vec![((0.0, 0.0), (0.0, 0.0))]);
    assert_eq!(
        edge.collision_fixture_aabb(0),
        Some((-0.002_f32, -0.002_f32, 0.002_f32, 0.002_f32))
    );
    let manifold = edge
        .collision_fixture_manifold(&bridge.scene["circle"], 0, 0)
        .expect("degenerate line fixture must not be filtered before contact evaluation");
    assert!(matches!(
        manifold.manifold_type(),
        ContactManifoldType::Circles
    ));
    assert_eq!(manifold.feature_id, contact_feature_id(0, 0, 0, 0));
    assert_eq!(manifold.penetration, f64::from(BOX2D_POLYGON_RADIUS as f32));
}

#[test]
fn native_contact_factory_does_not_register_edge_edge_pairs() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                clearVertices()
                addVertex(-1, 0)
                addVertex(1, 0)
                createLineShape("horizontal", "", 0, 0, 2, 0, 0, 0, 0, true, false, 1)
                clearVertices()
                addVertex(0, -1)
                addVertex(0, 1)
                createLineShape("vertical", "", 0, 0, 0, 2, 1, 0, 0, true, false, 1)
                setWorldGravity(0, 0)
                "#,
        )
        .unwrap();

    let mut bridge = runtime.render.lock().unwrap();
    assert!(
        bridge.scene["horizontal"]
            .collision_fixture_manifolds(&bridge.scene["vertical"])
            .is_empty()
    );
    assert!(bridge.solve_contacts().is_empty());
    assert!(bridge.active_contacts.is_empty());
}

#[test]
fn native_shape_constructor_adapters_are_strict_and_narrow_to_float32() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                box_number_type_fails = not pcall(
                    createBox, "bad_box", "", "0", 0, 1, 1,
                    1, 0, 0, true, false, 1
                )
                box_short_fails = not pcall(
                    createBox, "short_box", "", 0, 0, 1, 1,
                    1, 0, 0, true, false
                )
                circle_name_type_fails = not pcall(
                    createCircle, 1, "", 0, 0, 1,
                    1, 0, 0, true, false, 1
                )
                circle_bool_type_fails = not pcall(
                    createCircle, "bad_circle", "", 0, 0, 1,
                    1, 0, 0, 1, false, 1
                )
                clearVertices(); addVertex(-1, 0); addVertex(1, 0)
                line_bool_type_fails = not pcall(
                    createLineShape, "bad_line", "", 0, 0, 2, 0,
                    0, 0, 0, true, 0, 1
                )
                polygon_number_type_fails = not pcall(
                    createPolygon, "bad_polygon", "", 0, 0, 2, 2,
                    1, 0, "0", true, false, 1
                )
                nonphysics_string_type_fails = not pcall(
                    createNonPhysicsObject, "bad_visual", 2, 0, 0, 1
                )
                add_vertex_missing_fails = not pcall(addVertex, 1)
                add_vertex_string_fails = not pcall(addVertex, "1", 2)
                clearVertices()
                addVertex(0.99999999, -0.99999999)
                createPolygon(
                    "rounded_polygon", "", 0, 0, 1, 1,
                    1, 0, 0, true, false, 1
                )

                createBox(
                    "rounded", "SPRITE", 0.99999999, -0.99999999,
                    2.9999999, 3.9999999, 4.9999999,
                    0.19999999, 0.29999999, true, false, 5.9999999
                )
                createNonPhysicsObject(
                    "rounded_visual", "SPRITE", 0.99999999,
                    -0.99999999, 5.9999999
                )
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    for field in [
        "box_number_type_fails",
        "box_short_fails",
        "circle_name_type_fails",
        "circle_bool_type_fails",
        "line_bool_type_fails",
        "polygon_number_type_fails",
        "nonphysics_string_type_fails",
        "add_vertex_missing_fails",
        "add_vertex_string_fails",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }
    let world = object_world(runtime.lua()).unwrap();
    for name in [
        "bad_box",
        "short_box",
        "bad_circle",
        "bad_line",
        "bad_polygon",
        "bad_visual",
    ] {
        assert!(matches!(world.raw_get::<Value>(name).unwrap(), Value::Nil));
    }

    let bridge = runtime.render.lock().unwrap();
    let rounded = &bridge.scene["rounded"];
    assert_eq!((rounded.x, rounded.y), (1.0, -1.0));
    assert_eq!(rounded.density, f64::from(4.9999999_f32));
    assert_eq!(rounded.friction, f64::from(0.19999999_f32));
    assert_eq!(rounded.restitution, f64::from(0.29999999_f32));
    assert_eq!(rounded.z_order, f64::from(5.9999999_f32));
    assert!(matches!(
        &bridge.scene["rounded_polygon"].collision_shape,
        CollisionShape::Polygon { vertices, .. }
            if vertices == &vec![(
                f64::from(0.99999999_f32),
                f64::from(-0.99999999_f32)
            )]
    ));
    assert!(matches!(
        rounded.collision_shape,
        CollisionShape::Box { width, height }
            if width == f64::from(2.9999999_f32)
                && height == f64::from(3.9999999_f32)
    ));
    let visual = &bridge.scene["rounded_visual"];
    assert_eq!(
        (visual.x, visual.y, visual.z_order),
        (1.0, -1.0, f64::from(5.9999999_f32))
    );
}

#[test]
fn rotated_edge_polygon_keeps_edge_reference_in_edge_local_space() {
    let polygon = [
        (-0.5_f32, -0.5_f32),
        (0.5_f32, -0.5_f32),
        (0.5_f32, 0.5_f32),
        (-0.5_f32, 0.5_f32),
    ];
    let segment = ((-2.0_f32, 0.501_f32), (2.0_f32, 0.501_f32));
    let angle = 0.37_f32;
    let (sine, cosine) = angle.sin_cos();
    let transform = NativeToiTransform {
        position: (12.25, -7.5),
        sine,
        cosine,
    };
    let manifold =
        polygon_segment_manifold_at_transforms(&polygon, transform, segment, transform, true)
            .expect("rotated edge/polygon contact");
    let local = manifold.position;
    assert!(matches!(
        local.manifold_type,
        ContactManifoldType::FaceSecond
    ));
    assert_eq!(local.local_normal.0.to_bits(), 0.0_f32.to_bits());
    assert_eq!(local.local_normal.1.to_bits(), (-1.0_f32).to_bits());
    assert_eq!(local.local_point.0.to_bits(), segment.0.0.to_bits());
    assert_eq!(local.local_point.1.to_bits(), segment.0.1.to_bits());
    assert_eq!(local.point_count, 2);
    assert_eq!(local.local_points[0].1.to_bits(), 0.5_f32.to_bits());
    assert_eq!(local.local_points[1].1.to_bits(), 0.5_f32.to_bits());
    let expected_world_normal = transform.rotate((0.0, 1.0));
    assert_eq!(
        (manifold.normal_x as f32).to_bits(),
        expected_world_normal.0.to_bits()
    );
    assert_eq!(
        (manifold.normal_y as f32).to_bits(),
        expected_world_normal.1.to_bits()
    );
    let refreshed = manifold.at_native_transforms(transform, transform);
    assert_eq!((manifold.penetration as f32).to_bits(), 0x3B44_934E);
    assert_eq!(
        manifold.penetration.to_bits(),
        refreshed.penetration.to_bits()
    );
    assert_eq!(manifold.point_x.to_bits(), refreshed.point_x.to_bits());
    assert_eq!(manifold.point_y.to_bits(), refreshed.point_y.to_bits());
    let initial_second = manifold.secondary.expect("initial second point");
    let refreshed_second = refreshed.secondary.expect("refreshed second point");
    assert_eq!(
        initial_second.penetration.to_bits(),
        refreshed_second.penetration.to_bits()
    );
    assert_eq!(
        initial_second.point_x.to_bits(),
        refreshed_second.point_x.to_bits()
    );
    assert_eq!(
        initial_second.point_y.to_bits(),
        refreshed_second.point_y.to_bits()
    );
}

#[test]
fn rotated_edge_polygon_keeps_polygon_reference_in_polygon_local_space() {
    let polygon = [
        (-0.5_f32, -0.5_f32),
        (0.5_f32, -0.5_f32),
        (0.5_f32, 0.5_f32),
        (-0.5_f32, 0.5_f32),
    ];
    let segment = ((-0.2_f32, 0.505_f32), (0.2_f32, 0.495_f32));
    let angle = -0.29_f32;
    let (sine, cosine) = angle.sin_cos();
    let transform = NativeToiTransform {
        position: (-8.75, 3.125),
        sine,
        cosine,
    };
    let manifold =
        polygon_segment_manifold_at_transforms(&polygon, transform, segment, transform, true)
            .expect("rotated polygon-reference contact");
    let local = manifold.position;
    assert!(matches!(
        local.manifold_type,
        ContactManifoldType::FaceFirst
    ));
    assert_eq!(local.local_normal.0.to_bits(), 0.0_f32.to_bits());
    assert_eq!(local.local_normal.1.to_bits(), 1.0_f32.to_bits());
    assert_eq!(local.local_point.0.to_bits(), 0.5_f32.to_bits());
    assert_eq!(local.local_point.1.to_bits(), 0.5_f32.to_bits());
    assert_eq!(local.point_count, 1);
    assert_eq!(local.local_points[0].0.to_bits(), segment.1.0.to_bits());
    assert_eq!(local.local_points[0].1.to_bits(), segment.1.1.to_bits());
    let refreshed = manifold.at_native_transforms(transform, transform);
    assert_eq!(manifold.normal_x.to_bits(), refreshed.normal_x.to_bits());
    assert_eq!(manifold.normal_y.to_bits(), refreshed.normal_y.to_bits());
    assert_eq!(
        manifold.penetration.to_bits(),
        refreshed.penetration.to_bits()
    );
    assert_eq!(manifold.point_x.to_bits(), refreshed.point_x.to_bits());
    assert_eq!(manifold.point_y.to_bits(), refreshed.point_y.to_bits());
}
