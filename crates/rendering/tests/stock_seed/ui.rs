//! External tests for the renderer-owned UI mesh and batching boundary.

use std::error::Error;

use solarity_asset::AssetPath;
use solarity_rendering::{
    UiMeshPlan, UiMeshPlanError, UiRenderBlend, UiRenderQuad, UiRenderSource, UiRenderState,
    UiRenderTransform, UiRenderVertex, UiTextureAddressMode, UiTextureResidency,
};

/// Adjacent equal materials on one object merge without changing quad order.
#[test]
fn ui_mesh_batches_only_adjacent_equal_materials() -> Result<(), Box<dyn Error>> {
    let path = AssetPath::new("Interface\\Glues\\Shared.blp")?;
    let quads = vec![
        quad(
            4,
            UiRenderSource::Texture(path.clone()),
            [0.0, 0.0, 10.0, 20.0],
        ),
        quad(
            4,
            UiRenderSource::Texture(path.clone()),
            [10.0, 0.0, 20.0, 20.0],
        ),
        quad(12, UiRenderSource::VertexColor, [20.0, 0.0, 30.0, 20.0]),
        quad(16, UiRenderSource::Texture(path), [30.0, 0.0, 40.0, 20.0]),
    ];

    let mesh = UiMeshPlan::prepare([800.0, 600.0], quads.into_iter())?;

    assert_eq!(mesh.logical_extent(), [800.0, 600.0]);
    assert_eq!(mesh.vertices().len(), 16);
    assert_eq!(mesh.indices().len(), 12);
    assert_eq!(mesh.object_indices(), [4, 4, 12, 16]);
    assert_eq!(mesh.indices(), [0, 1, 2, 2, 1, 3, 4, 5, 6, 6, 5, 7]);
    assert_eq!(mesh.batches().len(), 3);
    assert_eq!(mesh.batches()[0].first_index(), 0);
    assert_eq!(mesh.batches()[0].index_count(), 12);
    assert_eq!(mesh.batches()[0].quad_count(), 2);
    assert_eq!(mesh.batches()[1].first_index(), 0);
    assert_eq!(mesh.batches()[2].first_index(), 0);
    assert_eq!(mesh.batches()[1].first_quad(), 2);
    assert_eq!(mesh.batches()[2].first_quad(), 3);
    assert_eq!(
        mesh.vertices()[0].position(),
        [0.0, 20.0],
        "quad vertices begin at the upper-left stock corner"
    );
    assert_eq!(
        mesh.vertex_bytes().len(),
        mesh.vertices().len() * UiRenderVertex::BYTE_SIZE
    );
    assert_eq!(mesh.index_bytes().len(), mesh.indices().len() * 4);
    Ok(())
}

/// Object-local batches keep inherited opacity independently addressable.
#[test]
fn ui_mesh_refreshes_object_opacity_without_replacing_geometry() -> Result<(), Box<dyn Error>> {
    let path = AssetPath::new("Interface\\Glues\\Shared.blp")?;
    let quads = vec![
        quad(
            4,
            UiRenderSource::Texture(path.clone()),
            [0.0, 0.0, 10.0, 20.0],
        )
        .with_opacity(0.75),
        quad(8, UiRenderSource::Texture(path), [10.0, 0.0, 20.0, 20.0]).with_opacity(0.5),
    ];
    let mut mesh = UiMeshPlan::prepare([800.0, 600.0], quads.into_iter())?;
    let identity = mesh.geometry_identity();
    let vertex_bytes = mesh.vertex_bytes().to_vec();
    let index_bytes = mesh.index_bytes().to_vec();

    assert_eq!(mesh.batches().len(), 2);
    assert_eq!(mesh.batches()[0].opacity(), 0.75);
    assert_eq!(mesh.batches()[1].opacity(), 0.5);

    mesh.refresh_object_opacities(|object_index| match object_index {
        4 => Some(0.25),
        8 => Some(0.875),
        _ => None,
    })?;

    assert_eq!(mesh.geometry_identity(), identity);
    assert_eq!(mesh.vertex_bytes(), vertex_bytes);
    assert_eq!(mesh.index_bytes(), index_bytes);
    assert_eq!(mesh.batches()[0].opacity(), 0.25);
    assert_eq!(mesh.batches()[1].opacity(), 0.875);
    Ok(())
}

/// Button font-state colors patch only their owned glyph vertices and retain indices.
#[test]
fn ui_mesh_recolors_one_object_without_rebuilding_topology() -> Result<(), Box<dyn Error>> {
    let quads = vec![
        quad(4, UiRenderSource::GlyphAtlas(7), [0.0, 0.0, 10.0, 20.0]),
        quad(8, UiRenderSource::GlyphAtlas(7), [10.0, 0.0, 20.0, 20.0]),
        quad(4, UiRenderSource::GlyphAtlas(7), [20.0, 0.0, 30.0, 20.0]),
    ];
    let mut mesh = UiMeshPlan::prepare([800.0, 600.0], quads.into_iter())?;
    let identity = mesh.geometry_identity();
    let positions = mesh
        .vertices()
        .iter()
        .map(|vertex| vertex.position())
        .collect::<Vec<_>>();
    let texture_coordinates = mesh
        .vertices()
        .iter()
        .map(|vertex| vertex.texture_coordinates())
        .collect::<Vec<_>>();
    let indices = mesh.index_bytes().to_vec();
    let gold = [1.0, 0.82, 0.0, 1.0];

    assert!(mesh.replace_object_quad_colors(4, &[[gold; 4], [gold; 4]])?);

    assert_ne!(mesh.geometry_identity(), identity);
    assert_eq!(mesh.index_bytes(), indices);
    assert_eq!(
        mesh.vertices()
            .iter()
            .map(|vertex| vertex.position())
            .collect::<Vec<_>>(),
        positions
    );
    assert_eq!(
        mesh.vertices()
            .iter()
            .map(|vertex| vertex.texture_coordinates())
            .collect::<Vec<_>>(),
        texture_coordinates
    );
    assert!(
        mesh.vertices()[0..4]
            .iter()
            .all(|vertex| vertex.color() == gold)
    );
    assert!(
        mesh.vertices()[4..8]
            .iter()
            .all(|vertex| vertex.color() == [1.0; 4])
    );
    assert!(
        mesh.vertices()[8..12]
            .iter()
            .all(|vertex| vertex.color() == gold)
    );
    assert!(!mesh.replace_object_quad_colors(4, &[[gold; 4]])?);
    Ok(())
}

/// EditBox text replaces only glyph slots when the same object owns a backdrop.
#[test]
fn ui_mesh_replaces_one_object_source_without_rebuilding_topology() -> Result<(), Box<dyn Error>> {
    let backdrop = AssetPath::new("Interface\\Glues\\EditBox.blp")?;
    let atlas = UiRenderSource::GlyphAtlas(7);
    let quads = vec![
        quad(4, UiRenderSource::Texture(backdrop), [0.0, 0.0, 40.0, 20.0]),
        quad(4, atlas.clone(), [2.0, 2.0, 10.0, 18.0]),
        quad(8, atlas.clone(), [50.0, 2.0, 58.0, 18.0]),
        quad(4, atlas.clone(), [10.0, 2.0, 12.0, 18.0]).with_state(UiRenderState::EditBoxCaret(4)),
    ];
    let mut mesh = UiMeshPlan::prepare([800.0, 600.0], quads.into_iter())?;
    mesh.set_state_opacity(UiRenderState::EditBoxCaret(4), 0.0)?;
    let identity = mesh.geometry_identity();
    let indices = mesh.index_bytes().to_vec();
    let batches = mesh.batches().to_vec();
    let backdrop_vertices = mesh.vertices()[0..4].to_vec();
    let replacement = vec![
        quad(4, atlas.clone(), [4.0, 2.0, 14.0, 18.0]),
        quad(4, atlas.clone(), [14.0, 2.0, 16.0, 18.0]).with_state(UiRenderState::EditBoxCaret(4)),
    ];

    assert!(mesh.replace_object_source_quads(4, &atlas, &replacement)?);
    assert_ne!(mesh.geometry_identity(), identity);
    assert_eq!(mesh.index_bytes(), indices);
    assert_eq!(mesh.batches(), batches);
    assert_eq!(&mesh.vertices()[0..4], backdrop_vertices);
    assert_eq!(mesh.vertices()[4].position(), [4.0, 18.0]);
    assert_eq!(mesh.vertices()[12].position(), [14.0, 18.0]);
    assert!(!mesh.replace_object_source_quads(4, &atlas, &replacement[..1])?);
    Ok(())
}

/// An EditBox blink changes only its retained caret draw slot.
#[test]
fn ui_mesh_refreshes_caret_opacity_without_replacing_geometry() -> Result<(), Box<dyn Error>> {
    let caret = UiRenderState::EditBoxCaret(8);
    let quads = vec![
        quad(8, UiRenderSource::GlyphAtlas(7), [0.0, 0.0, 10.0, 20.0]),
        quad(8, UiRenderSource::GlyphAtlas(7), [10.0, 0.0, 12.0, 20.0]).with_state(caret),
    ];
    let mut mesh = UiMeshPlan::prepare([800.0, 600.0], quads.into_iter())?;
    let identity = mesh.geometry_identity();

    assert_eq!(mesh.batches().len(), 2);
    mesh.set_state_opacity(caret, 0.0)?;

    assert_eq!(mesh.geometry_identity(), identity);
    assert_eq!(mesh.batches()[0].opacity(), 1.0);
    assert_eq!(mesh.batches()[1].opacity(), 0.0);
    Ok(())
}

/// Coverage generations retain typed identity and batch independently of BLPs.
#[test]
fn ui_mesh_batches_adjacent_glyph_atlas_quads() -> Result<(), Box<dyn Error>> {
    let quads = vec![
        quad(21, UiRenderSource::GlyphAtlas(7), [0.0, 0.0, 10.0, 20.0]),
        quad(21, UiRenderSource::GlyphAtlas(7), [10.0, 0.0, 20.0, 20.0]),
        quad(22, UiRenderSource::GlyphAtlas(8), [20.0, 0.0, 30.0, 20.0]),
    ];

    let mesh = UiMeshPlan::prepare([800.0, 600.0], quads.into_iter())?;

    assert_eq!(mesh.batches().len(), 2);
    assert_eq!(mesh.batches()[0].source(), &UiRenderSource::GlyphAtlas(7));
    assert_eq!(mesh.batches()[0].quad_count(), 2);
    assert_eq!(mesh.batches()[1].source(), &UiRenderSource::GlyphAtlas(8));
    assert_eq!(mesh.batches()[1].quad_count(), 1);
    Ok(())
}

/// A retained ScrollFrame submits only glyphs intersecting its translated clip.
#[test]
fn ui_mesh_selects_visible_window_from_retained_scroll_batch() -> Result<(), Box<dyn Error>> {
    let transform = UiRenderTransform::ScrollFrame(3);
    let atlas = UiRenderSource::GlyphAtlas(7);
    let clip = Some([0.0, 0.0, 100.0, 20.0]);
    let quads = vec![
        quad(2, UiRenderSource::VertexColor, [0.0, 0.0, 10.0, 10.0]),
        quad(4, atlas.clone(), [0.0, -30.0, 10.0, -20.0]).with_transform(
            transform,
            [0.0, 20.0],
            clip,
        ),
        quad(4, atlas.clone(), [0.0, -20.0, 10.0, -10.0]).with_transform(
            transform,
            [0.0, 20.0],
            clip,
        ),
        quad(4, atlas.clone(), [0.0, -10.0, 10.0, 0.0]).with_transform(
            transform,
            [0.0, 20.0],
            clip,
        ),
        quad(4, atlas, [0.0, 0.0, 10.0, 10.0]).with_transform(transform, [0.0, 20.0], clip),
    ];
    let mut mesh = UiMeshPlan::prepare([800.0, 600.0], quads.into_iter())?;

    assert_eq!(mesh.clipped_batch_quad_range(0), Some((0, 1)));
    assert_eq!(mesh.clipped_batch_quad_range(1), Some((2, 2)));
    mesh.set_transform_translation(transform, [0.0, 30.0]);
    assert_eq!(mesh.clipped_batch_quad_range(1), Some((1, 2)));
    mesh.set_transform_translation(transform, [0.0, -30.0]);
    assert_eq!(mesh.clipped_batch_quad_range(1), Some((1, 0)));
    assert_eq!(mesh.clipped_batch_quad_range(2), None);
    Ok(())
}

/// Scroll translation changes draw state without touching immutable mesh bytes.
#[test]
fn ui_mesh_retains_geometry_across_scroll_transforms() -> Result<(), Box<dyn Error>> {
    let transform = UiRenderTransform::ScrollFrame(12);
    let quads = vec![
        quad(21, UiRenderSource::GlyphAtlas(7), [0.0, 0.0, 10.0, 20.0]).with_transform(
            transform,
            [0.0, 4.0],
            Some([0.0, 0.0, 20.0, 20.0]),
        ),
        quad(21, UiRenderSource::GlyphAtlas(7), [10.0, 0.0, 20.0, 20.0]).with_transform(
            transform,
            [0.0, 4.0],
            Some([0.0, 0.0, 20.0, 20.0]),
        ),
        quad(22, UiRenderSource::GlyphAtlas(7), [20.0, 0.0, 30.0, 20.0]),
    ];
    let mut mesh = UiMeshPlan::prepare([800.0, 600.0], quads.into_iter())?;
    let identity = mesh.geometry_identity();
    let vertex_bytes = mesh.vertex_bytes().to_vec();
    let index_bytes = mesh.index_bytes().to_vec();

    mesh.set_transform_translation(transform, [0.0, 18.0]);
    mesh.translate_object(21, [3.0, 2.0])?;

    assert_eq!(mesh.geometry_identity(), identity);
    assert_eq!(mesh.vertex_bytes(), vertex_bytes);
    assert_eq!(mesh.index_bytes(), index_bytes);
    assert_eq!(mesh.batches().len(), 2);
    assert_eq!(mesh.batches()[0].translation(), [3.0, 20.0]);
    assert_eq!(mesh.batches()[1].translation(), [0.0, 0.0]);
    mesh.set_transform_translation(transform, [0.0, 9.0]);
    assert_eq!(mesh.batches()[0].translation(), [3.0, 11.0]);
    Ok(())
}

/// Developer tooling may retain native triangle meshes without quad expansion.
#[test]
fn ui_mesh_retains_single_material_indexed_triangles() -> Result<(), Box<dyn Error>> {
    let vertices = vec![
        UiRenderVertex::new([10.0, 20.0], [0.0, 0.0], [1.0; 4]),
        UiRenderVertex::new([30.0, 20.0], [1.0, 0.0], [1.0; 4]),
        UiRenderVertex::new([20.0, 40.0], [0.5, 1.0], [1.0; 4]),
    ];
    let mesh = UiMeshPlan::prepare_indexed(
        [800.0, 600.0],
        vertices.clone(),
        vec![0, 1, 2],
        UiRenderSource::GlyphAtlas(91),
        Some([8.0, 18.0, 32.0, 42.0]),
    )?;

    assert_eq!(mesh.vertices(), vertices);
    assert_eq!(mesh.indices(), [0, 1, 2]);
    assert!(mesh.object_indices().is_empty());
    assert_eq!(mesh.batches().len(), 1);
    assert_eq!(mesh.batches()[0].source(), &UiRenderSource::GlyphAtlas(91));
    assert_eq!(mesh.batches()[0].index_count(), 3);
    assert_eq!(mesh.batches()[0].quad_count(), 0);
    assert_eq!(mesh.batches()[0].clip(), Some([8.0, 18.0, 32.0, 42.0]));
    assert_eq!(
        mesh.vertex_bytes().len(),
        vertices.len() * UiRenderVertex::BYTE_SIZE
    );
    assert_eq!(mesh.index_bytes().len(), 3 * size_of::<u32>());

    let Err(error) = UiMeshPlan::prepare_indexed(
        [800.0, 600.0],
        vertices,
        vec![3],
        UiRenderSource::GlyphAtlas(91),
        None,
    ) else {
        return Err("an index outside the vertex array must be rejected".into());
    };
    assert_eq!(
        error,
        UiMeshPlanError::IndexOutOfRange {
            index: 3,
            vertex_count: 3,
        }
    );
    Ok(())
}

/// Builds one alpha-blended, clamped, blocking quad for mesh tests.
fn quad(object_index: usize, source: UiRenderSource, bounds: [f32; 4]) -> UiRenderQuad {
    UiRenderQuad::new(
        object_index,
        source,
        UiRenderBlend::Alpha,
        UiTextureAddressMode::Clamp,
        UiTextureAddressMode::Clamp,
        UiTextureResidency::Blocking,
        false,
        bounds,
        [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]],
        [[1.0; 4]; 4],
    )
}
