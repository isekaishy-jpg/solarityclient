//! Portable triangle model and display aliases for object-lifetime tests.

use std::error::Error;
use std::io::Cursor;
use wow_m2::chunks::material::{M2BlendMode, M2Material, M2RenderFlags};
use wow_m2::chunks::texture::{M2Texture, M2TextureFlags, M2TextureType};
use wow_m2::chunks::vertex::M2Vertex;
use wow_m2::common::{C2Vector, C3Vector, M2ArrayString};
use wow_m2::header::M2Header;
use wow_m2::skin::{OldSkinHeader, SkinBatch, SkinSubmesh};
use wow_m2::{M2Model, M2Version, OldSkin};

pub fn model() -> Result<Vec<u8>, Box<dyn Error>> {
    model_with_animations(&[147, 149, 151])
}

pub fn model_with_animations(animations: &[u16]) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut model = M2Model {
        header: M2Header::new(M2Version::WotLK),
        ..M2Model::default()
    };
    model.header.num_skin_profiles = Some(1);
    model.header.bounding_box_min = [-1.0; 3];
    model.header.bounding_box_max = [1.0; 3];
    model.header.bounding_sphere_radius = 2.0;
    model.textures = vec![M2Texture {
        texture_type: M2TextureType::Hardcoded,
        flags: M2TextureFlags::empty(),
        filename: M2ArrayString::default(),
    }];
    model.materials = vec![M2Material {
        flags: M2RenderFlags::empty(),
        blend_mode: M2BlendMode::OPAQUE,
    }];
    model.raw_data.texture_lookup_table = vec![0];
    model.raw_data.texture_units = vec![0];
    for [x, y, z] in [[0.0, -1.0, -1.0], [0.0, 1.0, -1.0], [0.0, 0.0, 1.0]] {
        model.vertices.push(M2Vertex {
            position: C3Vector { x, y, z },
            bone_weights: [0; 4],
            bone_indices: [0; 4],
            normal: C3Vector {
                x: -1.0,
                y: 0.0,
                z: 0.0,
            },
            tex_coords: C2Vector { x: 0.0, y: 0.0 },
            tex_coords2: Some(C2Vector { x: 0.0, y: 0.0 }),
        });
    }
    let mut buffer = Cursor::new(Vec::new());
    model.write(&mut buffer)?;
    let mut bytes = buffer.into_inner();
    let name_offset = bytes.len() as u32;
    bytes.extend_from_slice(b"GameObject\0");
    bytes[8..12].copy_from_slice(&11_u32.to_le_bytes());
    bytes[12..16].copy_from_slice(&name_offset.to_le_bytes());
    let sequence_offset = bytes.len() as u32;
    for animation in animations {
        let mut sequence = [0_u8; 64];
        sequence[..2].copy_from_slice(&animation.to_le_bytes());
        sequence[4..8].copy_from_slice(&1_000_u32.to_le_bytes());
        sequence[12..16].copy_from_slice(&0x20_u32.to_le_bytes());
        sequence[16..20].copy_from_slice(&32_767_u32.to_le_bytes());
        sequence[20..24].copy_from_slice(&1_u32.to_le_bytes());
        sequence[24..28].copy_from_slice(&1_u32.to_le_bytes());
        sequence[60..62].copy_from_slice(&u16::MAX.to_le_bytes());
        bytes.extend_from_slice(&sequence);
    }
    bytes[0x1C..0x20].copy_from_slice(&(animations.len() as u32).to_le_bytes());
    bytes[0x20..0x24].copy_from_slice(&sequence_offset.to_le_bytes());
    let bone_offset = bytes.len() as u32;
    let mut bone = [0_u8; 88];
    bone[..4].copy_from_slice(&(-1_i32).to_le_bytes());
    for offset in [8, 18, 38, 58] {
        bone[offset..offset + 2].copy_from_slice(&u16::MAX.to_le_bytes());
    }
    bytes.extend_from_slice(&bone);
    bytes[0x2C..0x30].copy_from_slice(&1_u32.to_le_bytes());
    bytes[0x30..0x34].copy_from_slice(&bone_offset.to_le_bytes());
    let weight_offset = bytes.len() as u32;
    bytes.extend_from_slice(&u16::MAX.to_le_bytes());
    bytes[0x90..0x94].copy_from_slice(&1_u32.to_le_bytes());
    bytes[0x94..0x98].copy_from_slice(&weight_offset.to_le_bytes());
    Ok(bytes)
}

pub fn skin() -> Result<Vec<u8>, Box<dyn Error>> {
    let skin = OldSkin {
        header: OldSkinHeader {
            bone_count_max: 1,
            ..OldSkinHeader::new()
        },
        indices: vec![0, 1, 2],
        triangles: vec![0, 1, 2],
        bone_indices: vec![0; 12],
        submeshes: vec![SkinSubmesh {
            id: 0,
            level: 0,
            vertex_start: 0,
            vertex_count: 3,
            triangle_start: 0,
            triangle_count: 3,
            bone_count: 0,
            bone_start: 0,
            bone_influence: 0,
            center: [0.0; 3],
            sort_center: [0.0; 3],
            bounding_radius: 2.0,
        }],
        batches: vec![SkinBatch {
            flags: 0,
            priority_plane: 0,
            shader_id: 0,
            skin_section_index: 0,
            geoset_index: 0,
            color_index: u16::MAX,
            material_index: 0,
            material_layer: 0,
            texture_count: 1,
            texture_combo_index: 0,
            texture_coord_combo_index: 0,
            texture_weight_combo_index: 0,
            texture_transform_combo_index: 0,
        }],
    };
    let mut buffer = Cursor::new(Vec::new());
    skin.write(&mut buffer)?;
    let mut bytes = buffer.into_inner();
    // The dependency writer advances past a section by 40 bytes even though
    // it emits the WotLK 48-byte section. Point at the actual final batch.
    let batch = bytes.len() - 24;
    bytes[40..44].copy_from_slice(&(batch as u32).to_le_bytes());
    bytes[batch + 8..batch + 10].copy_from_slice(&u16::MAX.to_le_bytes());
    Ok(bytes)
}

pub fn displays() -> Vec<u8> {
    let mut strings = vec![0];
    let mut records = Vec::new();
    for (id, path) in [
        (42_u32, "World\\GameObject.mdx"),
        (43, "World\\GameObject.m2"),
    ] {
        let mut row = [0_u32; 19];
        row[0] = id;
        row[1] = strings.len() as u32;
        strings.extend_from_slice(path.as_bytes());
        strings.push(0);
        for (slot, value) in row[12..18]
            .iter_mut()
            .zip([-1.0_f32, -1.0, -1.0, 1.0, 1.0, 1.0])
        {
            *slot = value.to_bits();
        }
        for word in row {
            records.extend_from_slice(&word.to_le_bytes());
        }
    }
    let mut bytes = b"WDBC".to_vec();
    for word in [2_u32, 19, 76, strings.len() as u32] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.extend(records);
    bytes.extend(strings);
    bytes
}
