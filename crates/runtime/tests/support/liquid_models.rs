//! Generated WMO liquid roots and exact material tables for runtime tests.

use super::bootstrap_texture_blp;

/// A one-cell MLIQ and complete material resources, without installed client data.
pub(crate) fn files(
    group_flags: u32,
    root_group_flags: u32,
    liquid_flags: u32,
    liquid_id: u32,
    tint: u32,
) -> Vec<(String, Vec<u8>)> {
    let mut root = Vec::new();
    chunk(&mut root, b"REVM", &17u32.to_le_bytes());
    let mut header = [0; 64];
    set_word(&mut header, 0, 1);
    set_word(&mut header, 4, 1);
    header[60..62].copy_from_slice(&4u16.to_le_bytes());
    for (index, value) in [-1f32, -1., -1., 5., 5., 3.].into_iter().enumerate() {
        set_word(&mut header, 36 + index * 4, value.to_bits());
    }
    chunk(&mut root, b"DHOM", &header);
    chunk(&mut root, b"XTOM", &[0]);
    let mut material = [0; 64];
    set_word(&mut material, 28, tint);
    chunk(&mut root, b"TMOM", &material);
    let mut info = [0; 32];
    set_word(&mut info, 0, root_group_flags);
    info[4..28].copy_from_slice(&header[36..60]);
    set_word(&mut info, 28, u32::MAX);
    chunk(&mut root, b"IGOM", &info);
    let mut group = Vec::new();
    chunk(&mut group, b"REVM", &17u32.to_le_bytes());
    let mut header = vec![0; 68];
    set_word(&mut header, 8, group_flags | 0x1000);
    header[12..36].copy_from_slice(&info[4..28]);
    set_word(&mut header, 52, liquid_id);
    // An ordinary triangle makes the same source admissible to the WMO GPU path.
    chunk(&mut header, b"YPOM", &[0x20, 0]);
    chunk(&mut header, b"IVOM", &[0, 0, 1, 0, 2, 0]);
    let mut positions = Vec::new();
    for point in [[0f32, 0., 0.], [1., 0., 0.], [0., 1., 0.]] {
        for value in point {
            positions.extend(value.to_le_bytes());
        }
    }
    chunk(&mut header, b"TVOM", &positions);
    let mut normals = Vec::new();
    for _ in 0..3 {
        for value in [0f32, 0., 1.] {
            normals.extend(value.to_le_bytes());
        }
    }
    chunk(&mut header, b"RNOM", &normals);
    chunk(&mut header, b"VTOM", &[0; 24]);
    let mut batch = [0; 24];
    batch[16..18].copy_from_slice(&3u16.to_le_bytes());
    batch[20..22].copy_from_slice(&2u16.to_le_bytes());
    chunk(&mut header, b"ABOM", &batch);
    header[42..44].copy_from_slice(&1u16.to_le_bytes());
    let mut grid = Vec::new();
    for value in [2u32, 2, 1, 1, 0, 0, 0] {
        grid.extend(value.to_le_bytes());
    }
    grid.extend(0u16.to_le_bytes());
    for _ in 0..4 {
        grid.extend([17, 2, 3, 4]);
        grid.extend(1f32.to_le_bytes());
    }
    grid.push(0);
    chunk(&mut header, b"QILM", &grid);
    chunk(&mut group, b"PGOM", &header);
    let mut files = vec![
        ("World\\Liquid.wmo".to_owned(), root),
        ("World\\Liquid_000.wmo".to_owned(), group),
    ];
    let mut strings = vec![0];
    let mut rows = Vec::new();
    for id in [1, 2, 5, 13, 14, 17, 19, 20, 21, 22] {
        let mut row = [0; 45];
        row[0] = id;
        row[2] = liquid_flags;
        row[14] = if id == 21 { 2 } else { 1 };
        row[15] = strings.len() as u32;
        let path = format!("XTextures\\type{id}.blp");
        strings.extend(path.as_bytes());
        strings.push(0);
        files.push((path, bootstrap_texture_blp()));
        row[16] = strings.len() as u32;
        strings.extend(if id == 17 {
            b"proceduralWmoWaterTex".as_slice()
        } else {
            b"proceduralRiverDepthTex".as_slice()
        });
        strings.push(0);
        row[23] = 1f32.to_bits();
        row[25] = 1f32.to_bits();
        row[41] = u32::from(id != 13);
        row[42] = 1250;
        rows.extend(row);
    }
    files.push((
        "DBFilesClient\\LiquidType.dbc".to_owned(),
        dbc(45, &rows, &strings),
    ));
    files.push((
        "DBFilesClient\\LiquidMaterial.dbc".to_owned(),
        dbc(3, &[1, 0, 1, 2, 1, 0], &[0]),
    ));
    let mut display = [0; 19];
    display[0] = 42;
    display[1] = 1;
    for (index, value) in [-1f32, -1., -1., 5., 5., 3.].into_iter().enumerate() {
        display[12 + index] = value.to_bits();
    }
    files.push((
        "DBFilesClient\\GameObjectDisplayInfo.dbc".to_owned(),
        dbc(19, &display, b"\0World\\Liquid.wmo\0"),
    ));
    files
}

/// Encodes a complete fixed-word WDBC fixture table.
fn dbc(fields: u32, rows: &[u32], strings: &[u8]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [
        rows.len() as u32 / fields,
        fields,
        fields * 4,
        strings.len() as u32,
    ] {
        bytes.extend(value.to_le_bytes());
    }
    for value in rows {
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend(strings);
    bytes
}

/// Writes the stock reversed-fourcc chunk representation.
fn chunk(bytes: &mut Vec<u8>, magic: &[u8; 4], payload: &[u8]) {
    bytes.extend(magic);
    bytes.extend((payload.len() as u32).to_le_bytes());
    bytes.extend(payload);
}

/// Changes a generated little-endian header field.
fn set_word(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
