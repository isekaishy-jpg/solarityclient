//! Stock contact markers retain their dispatch mapping after source decomposition.

#[test]
fn contact_markers_match_original_unit_dispatch() -> Result<(), Box<dyn std::error::Error>> {
    let mut cases = 0;
    let mut contacts = 0;
    for line in include_str!("../fixtures/unit_effect_contacts.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let mut fields = line.split_whitespace();
        let hex = fields.next().ok_or("marker")?;
        let mut marker = [0; 4];
        for (i, byte) in marker.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)?;
        }
        let left = fields.next().ok_or("handler")?.parse::<i32>()?;
        assert_eq!(super::foot_contact(marker), left >= 0, "{marker:?}");
        if left >= 0 {
            assert_eq!(i32::from(marker[2] == b'L'), left);
            contacts += 1;
        }
        cases += 1;
    }
    assert_eq!((cases, contacts), (185, 40));
    Ok(())
}
