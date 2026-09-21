//! Exact default-set MODR order with an unreferenced missing model.
fn chunk(bytes: &mut Vec<u8>, tag: &[u8; 4], payload: &[u8]) {
    bytes.extend_from_slice(tag);
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
}
fn word(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
pub(crate) fn root() -> Vec<u8> {
    let mut bytes = Vec::new();
    chunk(&mut bytes, b"REVM", &17_u32.to_le_bytes());
    let mut header = [0; 64];
    word(&mut header, 4, 1);
    word(&mut header, 16, 2);
    word(&mut header, 20, 3);
    word(&mut header, 24, 1);
    for (slot, value) in [-4_f32, -4., -4., 4., 4., 4.].iter().enumerate() {
        word(&mut header, 36 + slot * 4, value.to_bits());
    }
    chunk(&mut bytes, b"DHOM", &header);
    let mut info = [0; 32];
    info[4..28].copy_from_slice(&header[36..60]);
    word(&mut info, 28, u32::MAX);
    chunk(&mut bytes, b"IGOM", &info);
    let paths = b"World\\GameObject.m2\0World\\Missing.m2\0";
    chunk(&mut bytes, b"NDOM", paths);
    let missing = b"World\\GameObject.m2\0".len();
    let mut records = Vec::new();
    for (index, (y, color)) in [
        // MODD stores BGRA: red at -Y and blue at +Y.
        (-2_f32, [32, 96, 255, 255]),
        (2., [255, 96, 32, 255]),
        (0., [255; 4]),
    ]
    .into_iter()
    .enumerate()
    {
        records.extend_from_slice(&(if index == 2 { missing as u32 } else { 0_u32 }).to_le_bytes());
        for value in [0., y, 0., 0., 0., 0., 1., 1.] {
            records.extend_from_slice(&value.to_le_bytes());
        }
        records.extend_from_slice(&color);
    }
    chunk(&mut bytes, b"DDOM", &records);
    let mut set = [0; 32];
    set[..18].copy_from_slice(b"Set_$DefaultGlobal");
    word(&mut set, 24, 3);
    chunk(&mut bytes, b"SDOM", &set);
    bytes
}
pub(crate) fn group() -> Vec<u8> {
    let mut bytes = Vec::new();
    chunk(&mut bytes, b"REVM", &17_u32.to_le_bytes());
    let mut header = vec![0; 68];
    for (slot, value) in [-4_f32, -4., -4., 4., 4., 4.].iter().enumerate() {
        word(&mut header, 12 + slot * 4, value.to_bits());
    }
    chunk(&mut header, b"RDOM", &[1, 0, 0, 0, 1, 0]);
    chunk(&mut bytes, b"PGOM", &header);
    bytes
}
pub(crate) fn displays() -> Vec<u8> {
    let path = b"\0World\\Attached.wmo\0";
    let mut bytes = b"WDBC".to_vec();
    for value in [1_u32, 19, 76, path.len() as u32] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    let mut record = [0_u8; 76];
    word(&mut record, 0, 42);
    word(&mut record, 4, 1);
    for (slot, value) in [-4_f32, -4., -4., 4., 4., 4.].iter().enumerate() {
        word(&mut record, 48 + slot * 4, value.to_bits());
    }
    bytes.extend(record);
    bytes.extend(path);
    bytes
}
