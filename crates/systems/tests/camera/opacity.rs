use super::player_camera_opacity;

#[test]
fn ordinary_follow_opacity_matches_native_camera_fragment() {
    let mut count = 0;
    for line in include_str!("../fixtures/camera_opacity_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let words: Vec<_> = line
            .split_whitespace()
            .map(|word| {
                u32::from_str_radix(word, 16)
                    .unwrap_or_else(|error| panic!("native word {word}: {error}"))
            })
            .collect();
        assert_eq!(words.len(), 5);
        assert_eq!(
            u32::from(player_camera_opacity(
                f32::from_bits(words[0]),
                f32::from_bits(words[1]),
                f32::from_bits(words[2]),
                f32::from_bits(words[3]),
            )),
            words[4],
            "native record {count}: {line}",
        );
        count += 1;
    }
    assert_eq!(count, 1864);
}
