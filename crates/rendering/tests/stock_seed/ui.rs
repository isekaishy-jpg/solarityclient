//! External tests for the renderer-owned UI mesh and batching boundary.

use std::error::Error;

use solarity_asset::AssetPath;
use solarity_rendering::{
    UiMeshPlan, UiRenderBlend, UiRenderQuad, UiRenderSource, UiRenderTransform, UiRenderVertex,
    UiTextureAddressMode, UiTextureResidency,
};

/// Adjacent equal materials merge without changing quad or index order.
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
            8,
            UiRenderSource::Texture(path.clone()),
            [10.0, 0.0, 20.0, 20.0],
        ),
        quad(12, UiRenderSource::VertexColor, [20.0, 0.0, 30.0, 20.0]),
        quad(16, UiRenderSource::Texture(path), [30.0, 0.0, 40.0, 20.0]),
    ];

    let mesh = UiMeshPlan::prepare([800.0, 600.0], quads.into_iter())?;

    assert_eq!(mesh.logical_extent(), [800.0, 600.0]);
    assert_eq!(mesh.vertices().len(), 16);
    assert_eq!(mesh.indices().len(), 24);
    assert_eq!(mesh.object_indices(), [4, 8, 12, 16]);
    assert_eq!(mesh.indices()[0..12], [0, 1, 2, 2, 1, 3, 4, 5, 6, 6, 5, 7]);
    assert_eq!(mesh.batches().len(), 3);
    assert_eq!(mesh.batches()[0].first_index(), 0);
    assert_eq!(mesh.batches()[0].index_count(), 12);
    assert_eq!(mesh.batches()[0].quad_count(), 2);
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

    assert_eq!(mesh.geometry_identity(), identity);
    assert_eq!(mesh.vertex_bytes(), vertex_bytes);
    assert_eq!(mesh.index_bytes(), index_bytes);
    assert_eq!(mesh.batches().len(), 2);
    assert_eq!(mesh.batches()[0].translation(), [0.0, 18.0]);
    assert_eq!(mesh.batches()[1].translation(), [0.0, 0.0]);
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
