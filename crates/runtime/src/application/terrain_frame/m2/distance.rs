//! Build-12340 scenery size classes and camera-distance opacity.

use glam::{Mat4, Vec3};

/// CMapObj 7BDB10 classifies the largest transformed render-box dimension.
/// The center is the transformed render-box midpoint (4F5E80), not the origin.
#[derive(Clone, Copy)]
pub(super) struct SceneryDistance {
    center: Vec3,
    category: usize,
}

impl SceneryDistance {
    /// 78FB60 compares the stored group depth against native far squares.
    /// 78F570 retains the scaled radius in x87 when forming each square.
    pub(super) fn admits_group(self, depth: f32, detail: f32) -> bool {
        self.category >= minimum_category(depth, detail)
    }
    /// Replays 7F9430's affine bounds and 7BDD31's inclusive class thresholds.
    pub(super) fn new(minimum: Vec3, maximum: Vec3, transform: Mat4) -> Self {
        let mut world_minimum = transform.w_axis.truncate();
        let mut world_maximum = world_minimum;
        for (axis, low, high) in [
            (transform.x_axis.truncate(), minimum.x, maximum.x),
            (transform.y_axis.truncate(), minimum.y, maximum.y),
            (transform.z_axis.truncate(), minimum.z, maximum.z),
        ] {
            let first = axis * low;
            let second = axis * high;
            world_minimum += first.min(second);
            world_maximum += first.max(second);
        }
        let size = (world_maximum - world_minimum).max_element();
        let category = [1.0, 4.0, 15.0, 100.0]
            .iter()
            .position(|limit| size <= *limit)
            .unwrap_or(4);
        Self {
            center: transform.transform_point3((minimum + maximum) * 0.5),
            category,
        }
    }

    /// 78F570 scales only classes 1..=3 by environmentDetail; 791CB0
    /// rejects beyond the far radius and snaps alpha at 0.01 and 0.99.
    pub(super) fn opacity(self, camera: Vec3, environment_detail: f32) -> f32 {
        let distance_squared = self.center.distance_squared(camera);
        let distance = [30.0, 100.0, 200.0, 750.0, 1250.0][self.category];
        let distance = if (1..=3).contains(&self.category) {
            distance * environment_detail
        } else {
            distance
        };
        if distance_squared > distance * distance {
            return 0.0;
        }
        let fade = [5.0, 10.0, 15.0, 20.0, 50.0][self.category];
        let start = distance - fade;
        if distance_squared <= start * start {
            return 1.0;
        }
        let opacity = 1.0 - (distance_squared.sqrt() - start) / fade;
        if opacity > 0.99 {
            1.0
        } else if opacity <= 0.01 {
            0.0
        } else {
            opacity
        }
    }
}

fn minimum_category(depth: f32, detail: f32) -> usize {
    if depth <= 0.0 {
        return 0;
    }
    let square = f64::from(depth) * f64::from(depth);
    [30.0, 100.0, 200.0, 750.0]
        .into_iter()
        .enumerate()
        .position(|(index, radius)| {
            let radius = if index == 0 {
                radius
            } else {
                radius * f64::from(detail)
            };
            square < f64::from((radius * radius) as f32)
        })
        .unwrap_or(4)
}

#[cfg(test)]
mod tests {
    #[test]
    fn minimum_doodad_class_matches_original_boundaries() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut count = 0;
        for line in include_str!(
            "../../../../../systems/tests/fixtures/world_model_doodad_depth_native.txt"
        )
        .lines()
        .filter_map(|line| line.strip_prefix("class "))
        {
            let fields: Vec<_> = line.split_whitespace().collect();
            let detail = f32::from_bits(u32::from_str_radix(fields[0], 16)?);
            let depth = f32::from_bits(u32::from_str_radix(fields[1], 16)?);
            assert_eq!(
                super::minimum_category(depth, detail),
                fields[2].parse::<usize>()?,
                "{line}"
            );
            count += 1;
        }
        assert_eq!(count, 80);
        Ok(())
    }
}
