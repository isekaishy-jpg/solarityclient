use super::polygon_distance;
use glam::Vec3;
use std::error::Error;
#[test]
fn world_model_fog_polygon_distance_matches_original() -> Result<(), Box<dyn Error>> {
    let mut vertices = Vec::new();
    let mut plane = [0.; 4];
    let mut count = 0;
    for line in include_str!("../fixtures/world_model_fog_distance_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let row = line.split_ascii_whitespace().collect::<Vec<_>>();
        let values = row[2..]
            .iter()
            .map(|v| u32::from_str_radix(v, 16))
            .collect::<Result<Vec<_>, _>>()?;
        if row[0] == "polygon" {
            plane = std::array::from_fn(|i| f32::from_bits(values[i]));
            vertices = values[4..]
                .as_chunks::<3>()
                .0
                .iter()
                .map(|p| p.map(f32::from_bits))
                .collect();
        } else {
            let point = Vec3::new(
                f32::from_bits(values[0]),
                f32::from_bits(values[1]),
                f32::from_bits(values[2]),
            );
            let actual =
                polygon_distance(point, &vertices, Vec3::from_slice(&plane[..3]), plane[3]) as f32;
            assert_eq!(actual.to_bits(), values[3], "{line}: got {actual}");
            count += 1;
        }
    }
    assert_eq!(count, 2640);
    Ok(())
}
