//! Authored interior floor shared by registration and lighting regressions.

pub(crate) fn fixture_files() -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
    fn put(bytes: &mut [u8], at: usize, value: u32) {
        bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn chunk(bytes: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
        bytes.extend(tag);
        bytes.extend((data.len() as u32).to_le_bytes());
        bytes.extend(data);
    }
    let mut header = vec![0; 64];
    for (at, value) in [(4, 1), (16, 1), (20, 1), (24, 1), (28, 0x743b2511), (60, 8)] {
        put(&mut header, at, value);
    }
    for (at, value) in [
        (36, -10f32),
        (40, -10.),
        (44, -10.),
        (48, 10.),
        (52, 10.),
        (56, 10.),
    ] {
        put(&mut header, at, value.to_bits());
    }
    let mut root = Vec::new();
    chunk(&mut root, b"REVM", &17u32.to_le_bytes());
    chunk(&mut root, b"DHOM", &header);
    let mut info = vec![0; 32];
    info[4..28].copy_from_slice(&header[36..60]);
    put(&mut info, 28, u32::MAX);
    chunk(&mut root, b"IGOM", &info);
    chunk(&mut root, b"NDOM", b"Receiver.mdx\0");
    let mut doodad = [0; 40];
    put(&mut doodad, 28, 1f32.to_bits());
    put(&mut doodad, 32, 1f32.to_bits());
    put(&mut doodad, 36, 0x10203040);
    chunk(&mut root, b"DDOM", &doodad);
    let mut set = [0; 32];
    set[..18].copy_from_slice(b"Set_$DefaultGlobal");
    put(&mut set, 24, 1);
    chunk(&mut root, b"SDOM", &set);
    let mut group_header = vec![0; 68];
    put(&mut group_header, 8, 0x801);
    group_header[12..36].copy_from_slice(&header[36..60]);
    // 0x20 participates in both native primary and floor-color channels.
    chunk(&mut group_header, b"YPOM", &[0x20, 255]);
    chunk(&mut group_header, b"IVOM", &[0, 0, 1, 0, 2, 0]);
    chunk(
        &mut group_header,
        b"TVOM",
        &floats(&[0., 0., 0., 4., 0., 0., 0., 4., 0.]),
    );
    chunk(
        &mut group_header,
        b"RNOM",
        &floats(&[0., 0., 1., 0., 0., 1., 0., 0., 1.]),
    );
    let mut node = [0; 16];
    node[..8].copy_from_slice(&[4, 0, 255, 255, 255, 255, 1, 0]);
    chunk(&mut group_header, b"NBOM", &node);
    chunk(&mut group_header, b"RBOM", &[0, 0]);
    chunk(
        &mut group_header,
        b"VCOM",
        &[0xff102030u32, 0x40205070, 0x8090a0b0]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    chunk(&mut group_header, b"RDOM", &[0, 0]);
    let mut group = Vec::new();
    chunk(&mut group, b"REVM", &17u32.to_le_bytes());
    chunk(&mut group, b"PGOM", &group_header);
    let mut wdt = Vec::new();
    chunk(&mut wdt, b"REVM", &18u32.to_le_bytes());
    let mut mphd = [0; 32];
    put(&mut mphd, 0, 1);
    chunk(&mut wdt, b"DHPM", &mphd);
    chunk(&mut wdt, b"NIAM", &vec![0; 32768]);
    chunk(&mut wdt, b"OMWM", b"World\\Light.wmo\0");
    let mut modf = [0; 64];
    put(&mut modf, 4, 7);
    chunk(&mut wdt, b"FDOM", &modf);
    let mut fields = [0u32; 66];
    fields[0] = 571;
    fields[1] = 1;
    fields[5] = 1;
    fields[22] = 571;
    fields[59] = u32::MAX;
    let mut map = b"WDBC".to_vec();
    map.extend([1u32, 66, 264, 7].into_iter().flat_map(u32::to_le_bytes));
    map.extend(fields.into_iter().flat_map(u32::to_le_bytes));
    map.extend(b"\0Light\0");
    (root, group, wdt, map)
}

fn floats(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}
