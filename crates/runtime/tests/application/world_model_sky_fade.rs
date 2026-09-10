//! Stored DayNight sky fade against original 7F16F0 execution.

#[test]
fn world_model_sky_fade_matches_native_f32_store() -> Result<(), Box<dyn std::error::Error>> {
    let mut count = 0;
    for line in include_str!("../fixtures/world_model_sky_native.txt")
        .lines()
        .filter(|line| line.starts_with("fade "))
    {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        let distance = f32::from_bits(u32::from_str_radix(fields[2], 16)?);
        let actual = super::world_model_skybox_weight((fields[1] == "1").then_some(distance));
        assert_eq!(
            actual.to_bits(),
            u32::from_str_radix(fields[3], 16)?,
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 28);
    Ok(())
}
