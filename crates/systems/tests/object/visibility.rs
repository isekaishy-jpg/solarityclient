use super::player_flags_hide_model;

#[test]
fn ordinary_player_visibility_matches_native_scene_and_retirement_queries() {
    let mut count = 0;
    for line in include_str!("../fixtures/player_visibility_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let row: Vec<_> = line
            .split_whitespace()
            .map(|word| {
                u32::from_str_radix(word, 16)
                    .unwrap_or_else(|error| panic!("native word {word}: {error}"))
            })
            .collect();
        assert_eq!(row.len(), 9);
        let arena = row[1] == 40 && row[2] == 4;
        let hidden = player_flags_hide_model(row[0], arena) || (row[3] != 0 && row[4] == 0);
        assert_eq!(u32::from(row[5] & 1 == 0), row[6], "scene mask: {line}");
        assert_eq!(u32::from(hidden), row[7], "hidden: {line}");
        assert_eq!(u32::from(!hidden), row[8], "retirement: {line}");
        count += 1;
    }
    assert_eq!(count, 1152);
}
