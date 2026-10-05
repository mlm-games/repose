//! Compile every shader with naga and check it against the varying and vertex
//! attribute limits WebGL2 imposes but WebGPU does not.
//!
//! GLSL ES 3.00 only guarantees 15 fragment input vectors, and real WebGL2
//! mobile drivers report as few as 8 — enough to reject pipeline creation with
//! "Statically used varyings do not fit within packing limits". WebGPU has no
//! such limit, so an over-budget shader builds and runs fine on desktop and
//! fails only on a device. That failure is fatal at runtime, so check it here.
//!
//! The packing model below is deliberately conservative: it never lets flat and
//! smoothly interpolated varyings share a vec4 register, and requires each
//! varying to fit wholly within one register. Real GLSL ES packs in declaration
//! order, so locations must be declared ascending and are counted in that same
//! order.
use std::path::Path;

/// Lowest `MAX_VARYING_VECTORS` observed on WebGL2 mobile GPUs, and
/// `MAX_VERTEX_ATTRIBS` from the GLSL ES 3.00 spec floor.
const VARYING_BUDGET: usize = 8;
const VERTEX_ATTR_BUDGET: usize = 16;

/// Registers consumed by the varyings, allocated in declaration order with a
/// varying fitting wholly into the current vec4 register.
fn vectors_in_order(varyings: &[(usize, bool)]) -> usize {
    let mut registers = 0usize;
    let mut used = 0usize;
    let mut current_flat = false;
    for &(components, flat) in varyings {
        if used == 0 || used + components > 4 || flat != current_flat {
            registers += 1;
            used = 0;
            current_flat = flat;
        }
        used += components;
    }
    registers
}

fn components_of(module: &naga::Module, ty: naga::Handle<naga::Type>) -> usize {
    match &module.types[ty].inner {
        naga::TypeInner::Vector { size, .. } => *size as usize,
        _ => 1,
    }
}

/// Every entry point of `stage`. Matched on the stage rather than the name
/// because fragment entries are not all called `fs_main`.
fn entry_points<'a>(
    module: &'a naga::Module,
    stage: naga::ShaderStage,
) -> Vec<&'a naga::EntryPoint> {
    module
        .entry_points
        .iter()
        .filter(|entry| entry.stage == stage)
        .collect()
}

/// `(location, components, flat)` for each `@location` input of one entry
/// point, in declaration order.
///
/// A vertex entry point takes its inputs as individual `@location` arguments
/// while a fragment one passes the whole `VSOut`, whose bindings sit on the
/// members. Both shapes are read so a shader cannot quietly fall out of the
/// budget by switching between them.
fn location_inputs(module: &naga::Module, entry: &naga::EntryPoint) -> Vec<(u32, usize, bool)> {
    let mut inputs = Vec::new();
    for arg in &entry.function.arguments {
        match arg.binding {
            Some(naga::Binding::Location {
                location,
                interpolation,
                ..
            }) => inputs.push((
                location,
                components_of(module, arg.ty),
                matches!(interpolation, Some(naga::Interpolation::Flat)),
            )),
            _ => {
                let naga::TypeInner::Struct { members, .. } = &module.types[arg.ty].inner else {
                    continue;
                };
                for member in members {
                    let Some(naga::Binding::Location {
                        location,
                        interpolation,
                        ..
                    }) = member.binding
                    else {
                        continue;
                    };
                    inputs.push((
                        location,
                        components_of(module, member.ty),
                        matches!(interpolation, Some(naga::Interpolation::Flat)),
                    ));
                }
            }
        }
    }
    inputs
}

#[test]
fn shaders_fit_the_webgl2_varying_budget() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/shaders");
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "wgsl"))
        .collect();
    paths.sort();

    assert!(
        !paths.is_empty(),
        "no shaders found in {}, so this test would pass vacuously",
        dir.display()
    );

    let mut failures = Vec::new();
    for path in &paths {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("shader file name is not utf-8");

        let source = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("{}: cannot read: {e}", path.display()));
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|e| panic!("{name}.wgsl: parse failed: {e}"));

        for entry in entry_points(&module, naga::ShaderStage::Fragment) {
            let inputs = location_inputs(&module, entry);
            if inputs.windows(2).any(|w| w[0].0 >= w[1].0) {
                failures.push(format!(
                    "{name}.wgsl entry `{}`: locations must be declared ascending, got {:?}",
                    entry.name,
                    inputs.iter().map(|(l, ..)| *l).collect::<Vec<_>>()
                ));
            }
            let varyings: Vec<(usize, bool)> =
                inputs.iter().map(|&(_, c, flat)| (c, flat)).collect();
            let registers = vectors_in_order(&varyings);
            if registers > VARYING_BUDGET {
                failures.push(format!(
                    "{name}.wgsl entry `{}`: {registers} varying registers \
                     (budget {VARYING_BUDGET})",
                    entry.name
                ));
            }
        }

        for entry in entry_points(&module, naga::ShaderStage::Vertex) {
            let inputs = location_inputs(&module, entry);
            if inputs.len() > VERTEX_ATTR_BUDGET {
                failures.push(format!(
                    "{name}.wgsl entry `{}`: {} vertex attributes (budget {VERTEX_ATTR_BUDGET})",
                    entry.name,
                    inputs.len()
                ));
            }
            if inputs
                .iter()
                .enumerate()
                .any(|(i, (location, ..))| *location as usize != i)
            {
                failures.push(format!(
                    "{name}.wgsl entry `{}`: vertex locations must be 0..n-1, got {:?}",
                    entry.name,
                    inputs.iter().map(|(l, ..)| *l).collect::<Vec<_>>()
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "shaders exceed WebGL2 limits:\n{}",
        failures.join("\n")
    );
}
