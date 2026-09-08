//! Compiles every closed renderer shader domain into embedded SPIR-V.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use shaderc::{
    CompileOptions, Compiler, EnvVersion, OptimizationLevel, ShaderKind, SpirvVersion, TargetEnv,
};

fn main() {
    let output = match env::var_os("OUT_DIR") {
        Some(output) => PathBuf::from(output),
        None => panic!("Cargo must define OUT_DIR for build-generated shaders"),
    };
    compile(
        "src/shader/m2_spirv/source/m2.vert.glsl",
        ShaderKind::Vertex,
        &output.join("m2.vert.spv"),
        &[],
    );
    compile(
        "src/shader/m2_spirv/source/m2.frag.glsl",
        ShaderKind::Fragment,
        &output.join("m2-direct.frag.spv"),
        &[("M2_SHADOW_FILTERING", "0")],
    );
    compile(
        "src/shader/m2_spirv/source/m2.frag.glsl",
        ShaderKind::Fragment,
        &output.join("m2-pcf.frag.spv"),
        &[("M2_SHADOW_FILTERING", "1")],
    );
    compile(
        "src/shader/particle_spirv/source/m2_particle.vert.glsl",
        ShaderKind::Vertex,
        &output.join("m2-particle.vert.spv"),
        &[],
    );
    compile(
        "src/shader/particle_spirv/source/m2_particle.frag.glsl",
        ShaderKind::Fragment,
        &output.join("m2-particle.frag.spv"),
        &[],
    );
    compile(
        "src/shader/ribbon_spirv/source/m2_ribbon.vert.glsl",
        ShaderKind::Vertex,
        &output.join("m2-ribbon.vert.spv"),
        &[],
    );
    compile(
        "src/shader/ribbon_spirv/source/m2_ribbon.frag.glsl",
        ShaderKind::Fragment,
        &output.join("m2-ribbon.frag.spv"),
        &[],
    );
    for (name, textured, masked) in [
        ("texture", "1", "0"),
        ("color", "0", "0"),
        ("masked", "1", "1"),
    ] {
        compile(
            "src/shader/ui_spirv/source/ui.vert.glsl",
            ShaderKind::Vertex,
            &output.join(format!("ui-{name}.vert.spv")),
            &[("UI_TEXTURED", textured), ("UI_MASKED", masked)],
        );
        compile(
            "src/shader/ui_spirv/source/ui.frag.glsl",
            ShaderKind::Fragment,
            &output.join(format!("ui-{name}.frag.spv")),
            &[("UI_TEXTURED", textured), ("UI_MASKED", masked)],
        );
    }
    for layer_count in 1..=4 {
        let layer_count = layer_count.to_string();
        compile(
            "src/shader/terrain_spirv/source/terrain.vert.glsl",
            ShaderKind::Vertex,
            &output.join(format!("terrain-{layer_count}.vert.spv")),
            &[("TERRAIN_LAYER_COUNT", &layer_count)],
        );
        compile(
            "src/shader/terrain_spirv/source/terrain.frag.glsl",
            ShaderKind::Fragment,
            &output.join(format!("terrain-{layer_count}.frag.spv")),
            &[("TERRAIN_LAYER_COUNT", &layer_count)],
        );
    }
    for (name, shader) in [("water", "0"), ("water-no-specular", "1"), ("magma", "2")] {
        for (stage, kind) in [("vert", ShaderKind::Vertex), ("frag", ShaderKind::Fragment)] {
            compile(
                &format!("src/shader/liquid_spirv/source/liquid.{stage}.glsl"),
                kind,
                &output.join(format!("liquid-{name}.{stage}.spv")),
                &[("LIQUID_SHADER", shader)],
            );
        }
    }
    for (stage, kind) in [("vert", ShaderKind::Vertex), ("frag", ShaderKind::Fragment)] {
        compile(
            &format!("src/shader/ripple_spirv/source/ripple.{stage}.glsl"),
            kind,
            &output.join(format!("ripple.{stage}.spv")),
            &[],
        );
    }
    for (stage, kind) in [("vert", ShaderKind::Vertex), ("frag", ShaderKind::Fragment)] {
        compile(
            &format!("src/shader/underwater_spirv/source/underwater.{stage}.glsl"),
            kind,
            &output.join(format!("underwater.{stage}.spv")),
            &[],
        );
    }
    for (stage, kind) in [("vert", ShaderKind::Vertex), ("frag", ShaderKind::Fragment)] {
        compile(
            &format!("src/shader/sky_spirv/source/sky.{stage}.glsl"),
            kind,
            &output.join(format!("sky.{stage}.spv")),
            &[],
        );
    }
    for unified in 0..=1 {
        for shader in 0..=6 {
            let unified_value = unified.to_string();
            let shader_value = shader.to_string();
            let stem = format!("world-model-{unified}-{shader}");
            let definitions = [
                ("WORLD_MODEL_SHADER", shader_value.as_str()),
                ("WORLD_MODEL_UNIFIED", unified_value.as_str()),
            ];
            compile(
                "src/shader/world_model_spirv/source/world_model.vert.glsl",
                ShaderKind::Vertex,
                &output.join(format!("{stem}.vert.spv")),
                &definitions,
            );
            compile(
                "src/shader/world_model_spirv/source/world_model.frag.glsl",
                ShaderKind::Fragment,
                &output.join(format!("{stem}.frag.spv")),
                &definitions,
            );
        }
    }
    compile(
        "src/shader/glow_spirv/source/glow.vert.glsl",
        ShaderKind::Vertex,
        &output.join("glow.vert.spv"),
        &[],
    );
    for (name, source) in [
        ("composite", "src/shader/glow_spirv/source/glow.frag.glsl"),
        ("blur", "src/shader/glow_spirv/source/glow_blur.frag.glsl"),
        ("box", "src/shader/glow_spirv/source/glow_box.frag.glsl"),
    ] {
        compile(
            source,
            ShaderKind::Fragment,
            &output.join(format!("glow-{name}.frag.spv")),
            &[],
        );
    }
}

fn compile(source: &str, kind: ShaderKind, output: &Path, definitions: &[(&str, &str)]) {
    println!("cargo:rerun-if-changed={source}");
    let text = fs::read_to_string(source)
        .unwrap_or_else(|error| panic!("could not read build-time shader {source}: {error}"));
    let compiler = Compiler::new()
        .unwrap_or_else(|error| panic!("could not create build-time shader compiler: {error}"));
    let mut options = CompileOptions::new()
        .unwrap_or_else(|error| panic!("could not create build-time shader options: {error}"));
    options.set_target_env(TargetEnv::Vulkan, EnvVersion::Vulkan1_3 as u32);
    options.set_target_spirv(SpirvVersion::V1_6);
    options.set_optimization_level(OptimizationLevel::Performance);
    options.set_warnings_as_errors();
    for (name, value) in definitions {
        options.add_macro_definition(name, Some(value));
    }
    let artifact = compiler
        .compile_into_spirv(&text, kind, source, "main", Some(&options))
        .unwrap_or_else(|error| panic!("build-time shader {source} failed: {error}"));
    fs::write(output, artifact.as_binary_u8()).unwrap_or_else(|error| {
        panic!(
            "could not write build-time shader {}: {error}",
            output.display()
        )
    });
}
