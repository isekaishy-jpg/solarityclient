//! Native modified-click queries over the admitted binding and input images.

use crate::{UiBindingAssignments, UiModifierKeys, UiPointerButton};

pub(crate) fn is_modified_click(
    query: Option<&str>,
    bindings: Option<&UiBindingAssignments>,
    keys: UiModifierKeys,
    button: Option<UiPointerButton>,
) -> bool {
    let held = u8::from(keys.left_shift())
        | u8::from(keys.right_shift()) << 1
        | u8::from(keys.left_control()) << 2
        | u8::from(keys.right_control()) << 3
        | u8::from(keys.left_alt()) << 4
        | u8::from(keys.right_alt()) << 5;
    let Some(query) = query else {
        return held != 0
            || bindings.is_some_and(|bindings| {
                bindings
                    .modified_clicks()
                    .iter()
                    .any(|binding| assigned_chord_active(binding.chord().as_str(), held, button))
            });
    };
    let (mask, _) = modifier_prefix(query);
    if mask != 0 {
        // 55F940's explicit modifier expression requires every named family.
        return [3, 12, 48]
            .into_iter()
            .all(|family| mask & family == 0 || mask & family & held != 0);
    }
    bindings
        .and_then(|bindings| {
            bindings.modified_click(query).or_else(|| {
                bindings
                    .modified_clicks()
                    .iter()
                    .find(|binding| binding.action().eq_ignore_ascii_case(query))
            })
        })
        .is_some_and(|binding| assigned_chord_active(binding.chord().as_str(), held, button))
}

fn assigned_chord_active(chord: &str, held: u8, button: Option<UiPointerButton>) -> bool {
    let (mask, suffix) = modifier_prefix(chord);
    let required_button = match suffix {
        "BUTTON1" => Some(UiPointerButton::Left),
        "BUTTON2" => Some(UiPointerButton::Right),
        "BUTTON3" => Some(UiPointerButton::Middle),
        "BUTTON4" => Some(UiPointerButton::Button4),
        "BUTTON5" => Some(UiPointerButton::Button5),
        _ => None,
    };
    // 55D6B0 tests any assigned modifier bit, then the current button context.
    // A query outside button dispatch has no button filter.
    (mask != 0 || required_button.is_some())
        && (mask == 0 || mask & held != 0)
        && required_button
            .zip(button)
            .is_none_or(|(required, current)| required == current)
}

fn modifier_prefix(mut text: &str) -> (u8, &str) {
    let mut mask = 0;
    loop {
        let Some((token, bits)) = [
            ("LSHIFT", 1),
            ("RSHIFT", 2),
            ("SHIFT", 3),
            ("LCTRL", 4),
            ("RCTRL", 8),
            ("CTRL", 12),
            ("LALT", 16),
            ("RALT", 32),
            ("ALT", 48),
        ]
        .into_iter()
        .find(|(token, _)| {
            text.get(..token.len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(token))
        }) else {
            return (mask, text);
        };
        mask |= bits;
        text = &text[token.len()..];
        text = text.strip_prefix('-').unwrap_or(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expression_and_assignment_use_distinct_native_modifier_rules() {
        let left_shift = UiModifierKeys::new(true, false, false, false, false, false);
        assert!(!is_modified_click(
            Some("CTRL-SHIFT"),
            None,
            left_shift,
            None
        ));
        assert!(assigned_chord_active("CTRL-SHIFT", 1, None));
        assert!(is_modified_click(Some("LSHIFT"), None, left_shift, None));
        assert!(!is_modified_click(Some("RSHIFT"), None, left_shift, None));
        assert!(is_modified_click(None, None, left_shift, None));
        assert!(!is_modified_click(
            None,
            None,
            UiModifierKeys::default(),
            None
        ));
        assert!(!assigned_chord_active("NONE", 63, None));
    }

    #[test]
    fn button_filter_exists_only_inside_button_dispatch() {
        assert!(assigned_chord_active("SHIFT-BUTTON1", 1, None));
        assert!(assigned_chord_active(
            "SHIFT-BUTTON1",
            1,
            Some(UiPointerButton::Left)
        ));
        assert!(!assigned_chord_active(
            "SHIFT-BUTTON1",
            1,
            Some(UiPointerButton::Right)
        ));
        assert!(!assigned_chord_active(
            "SHIFT-BUTTON1",
            0,
            Some(UiPointerButton::Left)
        ));
        assert!(assigned_chord_active("BUTTON1", 0, None));
    }
}
