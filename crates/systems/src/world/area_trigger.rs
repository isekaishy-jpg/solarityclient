//! Native AreaTrigger containment, including rounded inverse-box translation.

use solarity_asset::AreaTriggerShape;

/// Prepared geometry for the build-12340 `6CE140` entry-volume predicate.
pub struct AreaTriggerVolume {
    map_id: u32,
    geometry: Geometry,
}

enum Geometry {
    Sphere {
        center: [f64; 3],
        radius_squared: f64,
    },
    Box {
        half_dimensions: [f32; 3],
        cosine: f64,
        sine: f64,
        translation: [f64; 3],
    },
}

impl AreaTriggerVolume {
    /// Prepares one admitted row without changing its authored geometry.
    #[must_use]
    pub fn new(map_id: u32, center: [f32; 3], shape: AreaTriggerShape) -> Self {
        let center = center.map(f64::from);
        let geometry = match shape {
            AreaTriggerShape::Sphere { radius } => Geometry::Sphere {
                center,
                radius_squared: f64::from(radius) * f64::from(radius),
            },
            AreaTriggerShape::Box {
                dimensions,
                rotation_radians,
            } => {
                // 4C3290 stores each x87 trigonometric result as f32; 4C2FC0
                // transposes that rigid basis and separately stores translation.
                let angle = f64::from(rotation_radians);
                let cosine = f64::from(angle.cos() as f32);
                let sine = f64::from(angle.sin() as f32);
                Geometry::Box {
                    half_dimensions: dimensions.map(|dimension| dimension * 0.5),
                    cosine,
                    sine,
                    translation: [
                        f64::from((-center[1] * sine - center[0] * cosine) as f32),
                        f64::from((-center[1] * cosine + center[0] * sine) as f32),
                        -center[2],
                    ],
                }
            }
        };
        Self { map_id, geometry }
    }

    /// Tests a world-space point on the current map.
    ///
    /// Spheres include the radius boundary. Box faces are excluded by `6CB930`.
    #[must_use]
    pub fn contains(&self, map_id: u32, position: [f32; 3]) -> bool {
        if map_id != self.map_id {
            return false;
        }
        let [x, y, z] = position.map(f64::from);
        match self.geometry {
            Geometry::Sphere {
                center,
                radius_squared,
            } => {
                let dx = center[0] - x;
                let dy = center[1] - y;
                let dz = center[2] - z;
                dx * dx + dz * dz + dy * dy <= radius_squared
            }
            Geometry::Box {
                half_dimensions,
                cosine,
                sine,
                translation,
            } => {
                // 4C21B0 stores the transformed point before the strict AABB test.
                let local = [
                    (x * cosine + y * sine + translation[0]) as f32,
                    (-x * sine + y * cosine + translation[1]) as f32,
                    (z + translation[2]) as f32,
                ];
                local
                    .into_iter()
                    .zip(half_dimensions)
                    .all(|(value, half)| -half < value && value < half)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AreaTriggerVolume;
    use solarity_asset::AreaTriggerShape;

    #[test]
    fn containment_matches_original_executable() -> Result<(), std::num::ParseIntError> {
        for line in include_str!("../../tests/fixtures/area-trigger-native.txt").lines() {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            let words: Vec<_> = line
                .split_whitespace()
                .map(|word| u32::from_str_radix(word, 16))
                .collect::<Result<_, _>>()?;
            let float = |index| f32::from_bits(words[index]);
            let shape = if float(5) != 0. {
                AreaTriggerShape::Sphere { radius: float(5) }
            } else {
                AreaTriggerShape::Box {
                    dimensions: [float(6), float(7), float(8)],
                    rotation_radians: float(9),
                }
            };
            let volume = AreaTriggerVolume::new(words[1], [float(2), float(3), float(4)], shape);
            assert_eq!(
                volume.contains(words[0], [float(10), float(11), float(12)]),
                words[13] != 0,
                "{line}"
            );
        }
        Ok(())
    }
}
