//! Exhaustive authored-presence cases captured from original behavior instructions.

use super::GameObjectAnimationRequest;

#[test]
fn missing_animation_requests_match_original_executable() -> Result<(), Box<dyn std::error::Error>>
{
    let fixture = include_str!("../fixtures/native_game_object_animation_requests.txt");
    let mut count = 0;
    for line in fixture.lines().filter(|line| !line.starts_with('#')) {
        let words = line
            .split_whitespace()
            .map(|word| word.parse::<u16>())
            .collect::<Result<Vec<_>, _>>()?;
        let [mask, requested, selected, frozen, reuse_current] = words.as_slice() else {
            return Err("invalid native fixture row".into());
        };
        let actual = GameObjectAnimationRequest::from_presence(*requested, |id| {
            (145..153).contains(&id) && mask & (1 << (id - 145)) != 0
        });
        assert_eq!(actual.animation_id(), *selected, "{line}");
        assert_eq!(actual.frozen(), *frozen != 0, "{line}");
        assert_eq!(
            actual.preserves_current(*selected),
            *reuse_current != 0,
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 2_048);
    Ok(())
}
