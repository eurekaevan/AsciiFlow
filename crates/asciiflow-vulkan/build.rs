use shaderc::{CompileOptions, Compiler, EnvVersion, OptimizationLevel, ShaderKind, TargetEnv};
use std::{env, fs, path::PathBuf};

fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../shaders/src");
    let output = PathBuf::from(env::var("OUT_DIR").unwrap());
    // Keep the build output authoritative when variant names change. Cargo may
    // reuse OUT_DIR across build-script revisions.
    for obsolete in ["ascii_map.spv", "ascii_render.spv"] {
        let _ = fs::remove_file(output.join(obsolete));
    }
    let compiler = Compiler::new().expect("shaderc compiler unavailable");
    let mut options = CompileOptions::new().expect("shaderc options unavailable");
    options.set_target_env(TargetEnv::Vulkan, EnvVersion::Vulkan1_3 as u32);
    options.set_optimization_level(OptimizationLevel::Performance);
    let map_path = root.join("ascii_map.comp");
    println!("cargo:rerun-if-changed={}", map_path.display());
    let map_source = fs::read_to_string(&map_path).expect("failed to read compute shader");
    for (name, workgroup_size, int64) in [
        ("ascii_map_u64_64", 64, true),
        ("ascii_map_u32_32", 32, false),
        ("ascii_map_u32_64", 64, false),
        ("ascii_map_u32_128", 128, false),
        ("ascii_map_u32_256", 256, false),
    ] {
        let mut map_options = options.clone();
        map_options.add_macro_definition("MAP_WORKGROUP_SIZE", Some(&workgroup_size.to_string()));
        if int64 {
            map_options.add_macro_definition("USE_INT64_SUMS", Some("1"));
        }
        let artifact = compiler
            .compile_into_spirv(
                &map_source,
                ShaderKind::Compute,
                &map_path.to_string_lossy(),
                "main",
                Some(&map_options),
            )
            .unwrap_or_else(|error| panic!("failed to compile {}: {error}", map_path.display()));
        fs::write(output.join(format!("{name}.spv")), artifact.as_binary_u8())
            .expect("failed to write SPIR-V");
    }
    let p010_map_path = root.join("ascii_map_p010.comp");
    println!("cargo:rerun-if-changed={}", p010_map_path.display());
    let p010_map_source = fs::read_to_string(&p010_map_path).expect("failed to read P010 shader");
    for (name, workgroup_size, int64) in [
        ("ascii_map_p010_u32_32", 32, false),
        ("ascii_map_p010_u64_64", 64, true),
    ] {
        let mut p010_options = options.clone();
        p010_options.add_macro_definition("MAP_WORKGROUP_SIZE", Some(&workgroup_size.to_string()));
        if int64 {
            p010_options.add_macro_definition("USE_INT64_SUMS", Some("1"));
        }
        let artifact = compiler
            .compile_into_spirv(
                &p010_map_source,
                ShaderKind::Compute,
                &p010_map_path.to_string_lossy(),
                "main",
                Some(&p010_options),
            )
            .unwrap_or_else(|error| {
                panic!("failed to compile {}: {error}", p010_map_path.display())
            });
        fs::write(output.join(format!("{name}.spv")), artifact.as_binary_u8())
            .expect("failed to write P010 SPIR-V");
    }
    let render_path = root.join("ascii_render.comp");
    println!("cargo:rerun-if-changed={}", render_path.display());
    let render_source = fs::read_to_string(&render_path).expect("failed to read compute shader");
    for (name, x, y, int64) in [
        ("ascii_render_int64_8x8", 8, 8, true),
        ("ascii_render_u32_8x8", 8, 8, false),
        ("ascii_render_u32_16x8", 16, 8, false),
        ("ascii_render_u32_16x16", 16, 16, false),
        ("ascii_render_u32_32x4", 32, 4, false),
        ("ascii_render_lut_8x8", 8, 8, false),
        ("ascii_render_lut_16x8", 16, 8, false),
        ("ascii_render_lut_16x16", 16, 16, false),
        ("ascii_render_lut_32x4", 32, 4, false),
    ] {
        let mut render_options = options.clone();
        render_options.add_macro_definition("LOCAL_SIZE_X", Some(&x.to_string()));
        render_options.add_macro_definition("LOCAL_SIZE_Y", Some(&y.to_string()));
        if int64 {
            render_options.add_macro_definition("USE_INT64_COORDS", Some("1"));
        }
        if name.contains("_lut_") {
            render_options.add_macro_definition("USE_COORD_LUT", Some("1"));
        }
        let artifact = compiler
            .compile_into_spirv(
                &render_source,
                ShaderKind::Compute,
                &render_path.to_string_lossy(),
                "main",
                Some(&render_options),
            )
            .unwrap_or_else(|error| panic!("failed to compile {}: {error}", render_path.display()));
        fs::write(output.join(format!("{name}.spv")), artifact.as_binary_u8())
            .expect("failed to write SPIR-V");
    }
    let p010_render_path = root.join("ascii_render_p010.comp");
    println!("cargo:rerun-if-changed={}", p010_render_path.display());
    let p010_render_source =
        fs::read_to_string(&p010_render_path).expect("failed to read P010 render shader");
    let artifact = compiler
        .compile_into_spirv(
            &p010_render_source,
            ShaderKind::Compute,
            &p010_render_path.to_string_lossy(),
            "main",
            Some(&options),
        )
        .unwrap_or_else(|error| {
            panic!("failed to compile {}: {error}", p010_render_path.display())
        });
    fs::write(
        output.join("ascii_render_p010_lut_32x4.spv"),
        artifact.as_binary_u8(),
    )
    .expect("failed to write P010 render SPIR-V");
}
