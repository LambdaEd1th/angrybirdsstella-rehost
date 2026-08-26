//! Native CompoSprite integer bounds used by sprite components and callbacks.

use stella_assets::ka3d::CompositePart;

use crate::{BoundCompositePart, SpriteCatalogRegion, native_fcvtzs_f32};

use super::NativeSpriteMetrics;

pub(crate) fn native_composite_metrics(
    parts: &[BoundCompositePart],
) -> Option<NativeSpriteMetrics> {
    native_composite_metrics_iter(parts.iter().map(|bound| (&bound.part, &bound.region)))
}

pub(crate) fn native_composite_metrics_from_parts(
    parts: &[CompositePart],
    regions: &[SpriteCatalogRegion],
) -> Option<NativeSpriteMetrics> {
    (parts.len() == regions.len())
        .then(|| native_composite_metrics_iter(parts.iter().zip(regions)))
        .flatten()
}

fn native_composite_metrics_iter<'a>(
    parts: impl IntoIterator<Item = (&'a CompositePart, &'a SpriteCatalogRegion)>,
) -> Option<NativeSpriteMetrics> {
    // CompoSprite::updateBounds (`sub_100436D40`) transforms all four raw
    // AtlasSprite corners, truncates every coordinate through FCVTZS, then
    // stores `max-min` as size and `-min` as pivot. COMP entries retain
    // AtlasSprite pointers, so this pass deliberately does not recurse.
    let mut minimum_x = i32::MAX;
    let mut minimum_y = i32::MAX;
    let mut maximum_x = i32::MIN;
    let mut maximum_y = i32::MIN;
    let mut submitted = false;
    for (part, region) in parts.into_iter().filter(|(part, _)| part.visible) {
        submitted = true;
        let sprite = &region.sprite;
        let (sine, cosine) = part.angle.sin_cos();
        let (basis_x_x, basis_x_y) = native_normalize_2d(cosine, sine);
        let (basis_y_x, basis_y_y) = native_normalize_2d(-sine, cosine);
        let scale_x = part.scale_x * part.flip_x;
        let scale_y = part.scale_y * part.flip_y;
        let m00 = scale_x * basis_x_x;
        let m10 = scale_x * basis_x_y;
        let m01 = scale_y * basis_y_x;
        let m11 = scale_y * basis_y_y;

        // sub_10001E440 composes R*Scale with T(-atlasPivot), using one
        // rounded FMUL addend followed by FMADD and a separate FADD. The
        // authored entry translation is added after that composition.
        let negative_pivot_x = -f32::from(sprite.pivot_x);
        let negative_pivot_y = -f32::from(sprite.pivot_y);
        let local_translate_x = 0.0_f32 + m00.mul_add(negative_pivot_x, m01 * negative_pivot_y);
        let local_translate_y = 0.0_f32 + m10.mul_add(negative_pivot_x, m11 * negative_pivot_y);
        let translate_x = part.x + local_translate_x;
        let translate_y = part.y + local_translate_y;

        for (local_x, local_y) in [
            (0.0_f32, 0.0_f32),
            (f32::from(sprite.width), 0.0_f32),
            (0.0_f32, f32::from(sprite.height)),
            (f32::from(sprite.width), f32::from(sprite.height)),
        ] {
            // sub_10057B8A0/sub_100436F58 retain this FMUL, FMADD, FADD
            // staging before each signed conversion.
            let x = translate_x + m00.mul_add(local_x, m01 * local_y);
            let y = translate_y + m10.mul_add(local_x, m11 * local_y);
            let x = native_fcvtzs_f32(x);
            let y = native_fcvtzs_f32(y);
            minimum_x = minimum_x.min(x);
            minimum_y = minimum_y.min(y);
            maximum_x = maximum_x.max(x);
            maximum_y = maximum_y.max(y);
        }
    }

    submitted.then_some(NativeSpriteMetrics {
        width: maximum_x.wrapping_sub(minimum_x),
        height: maximum_y.wrapping_sub(minimum_y),
        pivot_x: minimum_x.wrapping_neg(),
        pivot_y: minimum_y.wrapping_neg(),
    })
}

fn native_normalize_2d(x: f32, y: f32) -> (f32, f32) {
    // math::float2::normalize (`sub_10057B644`): FMUL(y,y),
    // FMADD(x,x,...), FSQRT, guarded reciprocal, then two FMULs.
    let length = x.mul_add(x, y * y).sqrt();
    let inverse = if length >= f32::MIN_POSITIVE {
        1.0_f32 / length
    } else {
        0.0_f32
    };
    (inverse * x, inverse * y)
}

#[cfg(test)]
mod tests {
    use stella_assets::ka3d::SpriteRegion;

    use super::*;

    fn region(
        name: &str,
        width: i16,
        height: i16,
        pivot_x: i16,
        pivot_y: i16,
    ) -> SpriteCatalogRegion {
        SpriteCatalogRegion {
            native_sheet_id: 7,
            texture_source: format!("{name}.pvr"),
            sprite: SpriteRegion {
                name: name.to_owned(),
                x: 0,
                y: 0,
                width,
                height,
                pivot_x,
                pivot_y,
                atlas_rotation: 0,
            },
        }
    }

    #[test]
    fn borrowed_composite_metrics_match_owned_bound_entries() {
        let parts = vec![
            CompositePart {
                sprite: "first".to_owned(),
                x: 10.25,
                y: -4.75,
                scale_x: 1.5,
                scale_y: 0.625,
                flip_x: -1.0,
                flip_y: 1.0,
                angle: 0.375,
                visible: true,
            },
            CompositePart {
                sprite: "hidden".to_owned(),
                x: 9_999.0,
                y: -9_999.0,
                scale_x: 1.0,
                scale_y: 1.0,
                flip_x: 1.0,
                flip_y: 1.0,
                angle: 0.0,
                visible: false,
            },
        ];
        let regions = vec![region("first", 31, 19, 7, -3), region("hidden", 5, 9, 1, 2)];
        let owned = parts
            .iter()
            .cloned()
            .zip(regions.iter().cloned())
            .map(|(part, region)| BoundCompositePart { part, region })
            .collect::<Vec<_>>();

        assert_eq!(
            native_composite_metrics_from_parts(&parts, &regions),
            native_composite_metrics(&owned)
        );
    }

    #[test]
    fn borrowed_composite_metrics_reject_misaligned_retained_arrays() {
        let parts = vec![CompositePart {
            sprite: "orphan".to_owned(),
            x: 0.0,
            y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            flip_x: 1.0,
            flip_y: 1.0,
            angle: 0.0,
            visible: true,
        }];

        assert_eq!(native_composite_metrics_from_parts(&parts, &[]), None);
    }
}
