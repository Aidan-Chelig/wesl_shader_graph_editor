use std::collections::{HashMap, HashSet};

use thiserror::Error;

use crate::graph::{Node, NodeId, NodeKind, ShaderGraph, ShaderType, Value};

#[derive(Clone, Debug, PartialEq)]
pub struct CompiledShader {
    /// Canonical generated source. The initial node library emits the
    /// WGSL-compatible subset of WESL; module imports will be added with
    /// package-backed nodes.
    pub wesl: String,
    /// Portable shader produced by linking the generated WESL.
    pub wgsl: String,
    /// WESL adapted to Bevy's mesh fragment interface for live preview.
    pub bevy_wesl: String,
    /// Linked WGSL adapted to Bevy's mesh fragment interface for live preview.
    pub bevy_wgsl: String,
    pub node_types: HashMap<NodeId, ShaderType>,
    pub emitted_nodes: Vec<NodeId>,
    pub uniforms: Vec<CompiledUniform>,
    pub textures: Vec<CompiledTexture>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledUniform {
    pub node: NodeId,
    pub index: usize,
    pub shader_type: ShaderType,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledTexture {
    pub node: NodeId,
    pub index: usize,
    pub name: String,
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum CompileError {
    #[error("the graph has no Fragment Output node")]
    MissingOutput,
    #[error("node {0:?} does not exist")]
    MissingNode(NodeId),
    #[error("node {node:?} input {input} is not connected")]
    MissingInput { node: NodeId, input: usize },
    #[error("the graph contains a cycle through node {0:?}")]
    Cycle(NodeId),
    #[error("node {node:?} cannot apply {operation} to {inputs:?}")]
    TypeMismatch {
        node: NodeId,
        operation: &'static str,
        inputs: Vec<ShaderType>,
    },
    #[error("generated WGSL failed to parse: {0}")]
    WgslParse(String),
    #[error("generated WGSL failed validation: {0}")]
    WgslValidation(String),
}

#[derive(Clone, Debug)]
struct CompiledNode {
    shader_type: ShaderType,
    expression: String,
}

#[derive(Clone, Debug, Default)]
struct HelperSources {
    wesl: String,
    wgsl: String,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum LygiaHelper {
    RandomScale,
    Random,
    Random2,
    Random3,
    Random4,
    ValueNoise2,
    Fbm2,
    Voronoi2,
    Pow2,
    Pow3,
    Saturate,
    Center,
    Uncenter,
    Scale2d,
    Rotate2d,
    CircleSdf,
    RectSdf,
    BoxSdf,
    Luma,
    Invert,
    Brightness,
    Contrast,
    Posterize,
    Saturation,
    GammaCorrect,
    BlendScreen,
    BlendOverlay,
    CosinePalette,
}

impl LygiaHelper {
    fn dependencies(self) -> &'static [Self] {
        match self {
            Self::Random2 | Self::Random3 | Self::Random4 => &[Self::Random],
            Self::ValueNoise2 => &[Self::Random2],
            Self::Fbm2 => &[Self::ValueNoise2],
            Self::Voronoi2 => &[Self::Random2],
            Self::Saturation => &[Self::Luma],
            Self::Random
            | Self::RandomScale
            | Self::Pow2
            | Self::Pow3
            | Self::Saturate
            | Self::Center
            | Self::Uncenter
            | Self::Scale2d
            | Self::Rotate2d
            | Self::CircleSdf
            | Self::RectSdf
            | Self::BoxSdf
            | Self::Luma
            | Self::Invert
            | Self::Brightness
            | Self::Contrast
            | Self::Posterize
            | Self::GammaCorrect
            | Self::BlendScreen
            | Self::BlendOverlay
            | Self::CosinePalette => &[],
        }
    }

    fn module_path(self) -> Option<&'static str> {
        match self {
            Self::Random => Some("generative/random.wgsl"),
            Self::Pow2 => Some("math/pow2.wgsl"),
            Self::Pow3 => Some("math/pow3.wgsl"),
            Self::Saturate => Some("math/saturate.wgsl"),
            Self::Center => Some("space/center.wgsl"),
            Self::Uncenter => Some("space/uncenter.wgsl"),
            Self::Scale2d => Some("space/scale.wgsl"),
            Self::Rotate2d => Some("space/rotate.wgsl"),
            Self::RectSdf => Some("sdf/rectSDF.wgsl"),
            Self::BoxSdf => Some("sdf/boxSDF.wgsl"),
            Self::Luma => Some("color/luma.wgsl"),
            Self::Contrast => Some("color/contrast.wgsl"),
            Self::GammaCorrect => Some("color/levels/gamma.wgsl"),
            Self::BlendScreen => Some("color/blend/screen.wgsl"),
            Self::BlendOverlay => Some("color/blend/overlay.wgsl"),
            Self::RandomScale
            | Self::Random2
            | Self::Random3
            | Self::Random4
            | Self::ValueNoise2
            | Self::Fbm2
            | Self::Voronoi2
            | Self::CircleSdf
            | Self::Invert
            | Self::Brightness
            | Self::Posterize
            | Self::Saturation
            | Self::CosinePalette => None,
        }
    }

    fn import_items(self) -> &'static [&'static str] {
        match self {
            Self::Random => &["random"],
            Self::Random2 => &["random2"],
            Self::Random3 => &["random3"],
            Self::Random4 => &["random4"],
            Self::Pow2 => &["pow2", "pow22", "pow23", "pow24"],
            Self::Pow3 => &["pow3", "pow32", "pow33", "pow34"],
            Self::Center => &["center", "center2", "center3"],
            Self::Uncenter => &["uncenter", "uncenter2", "uncenter3"],
            Self::Scale2d => &["scale"],
            Self::Rotate2d => &["rotate"],
            Self::RectSdf => &["rectSDF"],
            Self::BoxSdf => &["boxSDF"],
            Self::Luma => &["luma"],
            Self::Contrast => &["contrast3"],
            Self::GammaCorrect => &["levelsGamma3a"],
            Self::BlendScreen => &["blendScreen3"],
            Self::BlendOverlay => &["blendOverlay3"],
            Self::RandomScale
            | Self::ValueNoise2
            | Self::Fbm2
            | Self::Voronoi2
            | Self::Saturate
            | Self::CircleSdf
            | Self::Invert
            | Self::Brightness
            | Self::Posterize
            | Self::Saturation
            | Self::CosinePalette => &[],
        }
    }

    fn wrapper_source(self) -> &'static str {
        match self {
            Self::RandomScale => "",
            Self::Random => "fn lygia_random(p: f32) -> f32 { return random(p); }\n\n",
            Self::Random2 => "fn lygia_random2(st: vec2<f32>) -> f32 { return random2(st); }\n\n",
            Self::Random3 => "fn lygia_random3(p: vec3<f32>) -> f32 { return random3(p); }\n\n",
            Self::Random4 => "fn lygia_random4(p: vec4<f32>) -> f32 { return random4(p); }\n\n",
            Self::ValueNoise2 => {
                "fn lygia_value_noise2(st: vec2<f32>) -> f32 {\n\
    let i = floor(st);\n\
    let f = fract(st);\n\
    let a = lygia_random2(i);\n\
    let b = lygia_random2(i + vec2<f32>(1.0, 0.0));\n\
    let c = lygia_random2(i + vec2<f32>(0.0, 1.0));\n\
    let d = lygia_random2(i + vec2<f32>(1.0, 1.0));\n\
    let u = f * f * (vec2<f32>(3.0) - 2.0 * f);\n\
    return mix(a, b, u.x) + (c - a) * u.y * (1.0 - u.x) + (d - b) * u.x * u.y;\n\
}\n\n"
            }
            Self::Fbm2 => {
                "fn lygia_fbm2(st: vec2<f32>) -> f32 {\n\
    var value = 0.0;\n\
    var amplitude = 0.5;\n\
    var frequency = 1.0;\n\
    for (var i = 0; i < 5; i = i + 1) {\n\
        value += amplitude * lygia_value_noise2(st * frequency);\n\
        frequency *= 2.0;\n\
        amplitude *= 0.5;\n\
    }\n\
    return value;\n\
}\n\n"
            }
            Self::Voronoi2 => {
                "fn lygia_random2_vec2(st: vec2<f32>) -> vec2<f32> {\n\
    return vec2<f32>(lygia_random2(st), lygia_random2(st + vec2<f32>(19.19, 73.42)));\n\
}\n\
\n\
fn lygia_voronoi2(st: vec2<f32>) -> f32 {\n\
    return lygia_voronoi2_time(st, 0.0);\n\
}\n\
\n\
fn lygia_voronoi2_time(st: vec2<f32>, time: f32) -> f32 {\n\
    let i = floor(st);\n\
    let f = fract(st);\n\
    var min_dist = 8.0;\n\
    for (var y = -1; y <= 1; y = y + 1) {\n\
        for (var x = -1; x <= 1; x = x + 1) {\n\
            let neighbor = vec2<f32>(f32(x), f32(y));\n\
            let point = 0.5 + 0.5 * sin(time + 6.28318530718 * lygia_random2_vec2(i + neighbor));\n\
            let diff = neighbor + point - f;\n\
            min_dist = min(min_dist, dot(diff, diff));\n\
        }\n\
    }\n\
    return sqrt(min_dist);\n\
}\n\n"
            }
            Self::Pow2 | Self::Pow3 | Self::Saturate | Self::Center | Self::Uncenter => "",
            Self::Scale2d => {
                "fn lygia_scale2d(st: vec2<f32>, scale_value: vec2<f32>) -> vec2<f32> { return scale(st, scale_value); }\n\n"
            }
            Self::Rotate2d => {
                "fn lygia_rotate2d(st: vec2<f32>, angle: f32) -> vec2<f32> { return rotate(st, angle); }\n\n"
            }
            Self::CircleSdf => {
                "fn lygia_circle_sdf(st: vec2<f32>, radius: f32) -> f32 {\n\
    return length(st - vec2<f32>(0.5)) - radius;\n\
}\n\n"
            }
            Self::RectSdf => {
                "fn lygia_rect_sdf(st: vec2<f32>, size: vec2<f32>) -> f32 { return rectSDF(st, size); }\n\n"
            }
            Self::BoxSdf => {
                "fn lygia_box_sdf(position: vec3<f32>, bounds: vec3<f32>) -> f32 { return boxSDF(position, bounds); }\n\n"
            }
            Self::Luma => "fn lygia_luma(color: vec3<f32>) -> f32 { return luma(color); }\n\n",
            Self::Invert => {
                "fn lygia_invert(color: vec3<f32>) -> vec3<f32> {\n\
    return vec3<f32>(1.0) - color;\n\
}\n\n"
            }
            Self::Brightness => {
                "fn lygia_brightness(color: vec3<f32>, amount: f32) -> vec3<f32> {\n\
    return color + vec3<f32>(amount);\n\
}\n\n"
            }
            Self::Contrast => {
                "fn lygia_contrast(color: vec3<f32>, amount: f32) -> vec3<f32> { return contrast3(color, amount); }\n\n"
            }
            Self::Posterize => {
                "fn lygia_posterize(color: vec3<f32>, steps: f32) -> vec3<f32> {\n\
    let safe_steps = max(steps, 1.0);\n\
    return floor(color * safe_steps) / safe_steps;\n\
}\n\n"
            }
            Self::Saturation => {
                "fn lygia_saturation(color: vec3<f32>, amount: f32) -> vec3<f32> {\n\
    let gray = vec3<f32>(lygia_luma(color));\n\
    return mix(gray, color, amount);\n\
}\n\n"
            }
            Self::GammaCorrect => {
                "fn lygia_gamma_correct(color: vec3<f32>, gamma: f32) -> vec3<f32> { return levelsGamma3a(max(color, vec3<f32>(0.0)), gamma); }\n\n"
            }
            Self::BlendScreen => {
                "fn lygia_blend_screen(base: vec3<f32>, blend: vec3<f32>) -> vec3<f32> { return blendScreen3(base, blend); }\n\n"
            }
            Self::BlendOverlay => {
                "fn lygia_blend_overlay(base: vec3<f32>, blend: vec3<f32>) -> vec3<f32> { return blendOverlay3(base, blend); }\n\n"
            }
            Self::CosinePalette => {
                "fn lygia_cosine_palette(t: f32) -> vec3<f32> {\n\
    let a = vec3<f32>(0.5, 0.5, 0.5);\n\
    let b = vec3<f32>(0.5, 0.5, 0.5);\n\
    let c = vec3<f32>(1.0, 1.0, 1.0);\n\
    let d = vec3<f32>(0.0, 0.33, 0.67);\n\
    return a + b * cos(6.28318530718 * (c * t + d));\n\
}\n\n"
            }
        }
    }
}

const LYGIA_HELPER_ORDER: [LygiaHelper; 28] = [
    LygiaHelper::RandomScale,
    LygiaHelper::Random,
    LygiaHelper::Random2,
    LygiaHelper::Random3,
    LygiaHelper::Random4,
    LygiaHelper::ValueNoise2,
    LygiaHelper::Fbm2,
    LygiaHelper::Voronoi2,
    LygiaHelper::Pow2,
    LygiaHelper::Pow3,
    LygiaHelper::Saturate,
    LygiaHelper::Center,
    LygiaHelper::Uncenter,
    LygiaHelper::Scale2d,
    LygiaHelper::Rotate2d,
    LygiaHelper::CircleSdf,
    LygiaHelper::RectSdf,
    LygiaHelper::BoxSdf,
    LygiaHelper::Luma,
    LygiaHelper::Invert,
    LygiaHelper::Brightness,
    LygiaHelper::Contrast,
    LygiaHelper::Posterize,
    LygiaHelper::Saturation,
    LygiaHelper::GammaCorrect,
    LygiaHelper::BlendScreen,
    LygiaHelper::BlendOverlay,
    LygiaHelper::CosinePalette,
];

fn lygia_helper_sources(helpers: &HashSet<LygiaHelper>) -> HelperSources {
    if helpers.is_empty() {
        return HelperSources::default();
    }

    let mut import_sources = String::from("// LYGIA imports for graph nodes\n");
    let mut imported_modules: HashMap<&'static str, Vec<&'static str>> = HashMap::new();
    for helper in LYGIA_HELPER_ORDER {
        if helpers.contains(&helper)
            && let Some(path) = helper.module_path()
        {
            imported_modules
                .entry(path)
                .or_default()
                .extend(helper.import_items());
        }
    }
    let mut imported_modules = imported_modules.into_iter().collect::<Vec<_>>();
    imported_modules.sort_by_key(|(path, _)| *path);
    for (path, mut items) in imported_modules {
        items.sort_unstable();
        items.dedup();
        if !items.is_empty() {
            import_sources.push_str(&format!(
                "import lygia::{}::{{{}}};\n",
                path.trim_end_matches(".wgsl").replace('/', "::"),
                items.join(", ")
            ));
        }
    }
    import_sources.push('\n');
    import_sources.push_str("// LYGIA graph compatibility wrappers\n");
    for helper in LYGIA_HELPER_ORDER {
        if helpers.contains(&helper) {
            import_sources.push_str(helper.wrapper_source());
        }
    }

    let mut linked_sources = String::from("// LYGIA vendored modules for graph nodes\n");
    let mut included_modules = HashSet::new();
    for helper in LYGIA_HELPER_ORDER {
        if helpers.contains(&helper)
            && let Some(path) = helper.module_path()
        {
            linked_sources.push_str(&resolve_lygia_module(path, &mut included_modules));
        }
    }
    linked_sources.push_str("// LYGIA graph compatibility wrappers\n");
    for helper in LYGIA_HELPER_ORDER {
        if helpers.contains(&helper) {
            linked_sources.push_str(helper.wrapper_source());
        }
    }

    HelperSources {
        wesl: import_sources,
        wgsl: linked_sources,
    }
}

fn resolve_lygia_module(path: &str, included: &mut HashSet<String>) -> String {
    let path = normalize_lygia_path("", path);
    if !included.insert(path.clone()) {
        return String::new();
    }

    let mut out = format!("// LYGIA: {path}\n");
    let source = lygia_module_source(&path);
    for line in source.lines() {
        if let Some(include) = parse_lygia_include(line) {
            let resolved = normalize_lygia_path(&path, include);
            out.push_str(&resolve_lygia_module(&resolved, included));
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out.push('\n');
    sanitize_lygia_wgsl(&out)
}

fn parse_lygia_include(line: &str) -> Option<&str> {
    let line = line.trim();
    let rest = line.strip_prefix("#include")?.trim();
    rest.strip_prefix('"')?
        .split_once('"')
        .map(|(path, _)| path)
}

fn normalize_lygia_path(current_path: &str, include_path: &str) -> String {
    let mut parts = Vec::new();
    if !current_path.is_empty() && include_path.starts_with('.') {
        parts.extend(
            current_path
                .rsplit_once('/')
                .map_or("", |(dir, _)| dir)
                .split('/'),
        );
    }

    for part in include_path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }

    parts.join("/")
}

fn lygia_module_source(path: &str) -> &'static str {
    match path {
        "generative/random.wgsl" => include_str!("../vendor/lygia/generative/random.wgsl"),
        "math/pow2.wgsl" => include_str!("../vendor/lygia/math/pow2.wgsl"),
        "math/pow3.wgsl" => include_str!("../vendor/lygia/math/pow3.wgsl"),
        "math/saturate.wgsl" => include_str!("../vendor/lygia/math/saturate.wgsl"),
        "space/center.wgsl" => include_str!("../vendor/lygia/space/center.wgsl"),
        "space/uncenter.wgsl" => include_str!("../vendor/lygia/space/uncenter.wgsl"),
        "space/scale.wgsl" => include_str!("../vendor/lygia/space/scale.wgsl"),
        "space/rotate.wgsl" => include_str!("../vendor/lygia/space/rotate.wgsl"),
        "math/rotate2d.wgsl" => include_str!("../vendor/lygia/math/rotate2d.wgsl"),
        "sdf/rectSDF.wgsl" => include_str!("../vendor/lygia/sdf/rectSDF.wgsl"),
        "sdf/boxSDF.wgsl" => include_str!("../vendor/lygia/sdf/boxSDF.wgsl"),
        "color/luma.wgsl" => include_str!("../vendor/lygia/color/luma.wgsl"),
        "color/space/rgb2luma.wgsl" => {
            include_str!("../vendor/lygia/color/space/rgb2luma.wgsl")
        }
        "color/contrast.wgsl" => include_str!("../vendor/lygia/color/contrast.wgsl"),
        "color/levels/gamma.wgsl" => include_str!("../vendor/lygia/color/levels/gamma.wgsl"),
        "color/blend/screen.wgsl" => include_str!("../vendor/lygia/color/blend/screen.wgsl"),
        "color/blend/overlay.wgsl" => include_str!("../vendor/lygia/color/blend/overlay.wgsl"),
        _ => panic!("unsupported LYGIA include: {path}"),
    }
}

fn sanitize_lygia_wgsl(source: &str) -> String {
    source
        .replace("vec2(", "vec2f(")
        .replace("vec3(", "vec3f(")
        .replace("vec4(", "vec4f(")
}

pub fn compile(graph: &ShaderGraph) -> Result<CompiledShader, CompileError> {
    compile_with_preview_node(graph, None)
}

pub fn compile_with_preview_node(
    graph: &ShaderGraph,
    preview_node: Option<NodeId>,
) -> Result<CompiledShader, CompileError> {
    let output = graph.fragment_output().ok_or(CompileError::MissingOutput)?;
    let main_entry = compile_fragment_output(graph, output)?;
    let wesl = generate_graph_wesl(
        &main_entry.helpers.wesl,
        &main_entry.statements,
        output,
        &main_entry.color_expression,
        main_entry.textures.len(),
    );
    let wgsl_source = generate_graph_wesl(
        &main_entry.helpers.wgsl,
        &main_entry.statements,
        output,
        &main_entry.color_expression,
        main_entry.textures.len(),
    );
    let wgsl = link_wesl(&wgsl_source);
    let preview_entry = if let Some(preview_node) = preview_node {
        Some(compile_preview_output(graph, preview_node)?)
    } else {
        None
    };
    let bevy_entry = preview_entry.as_ref().unwrap_or(&main_entry);
    let texture_count = bevy_entry.textures.len().max(1);
    let bevy_wesl = generate_bevy_wesl(
        &bevy_entry.helpers.wesl,
        &bevy_entry.statements,
        &bevy_entry.target,
        &bevy_entry.color_expression.replace("input.uv", "mesh.uv"),
        texture_count,
    );
    let bevy_wgsl = link_wesl(&generate_bevy_wesl(
        &bevy_entry.helpers.wgsl,
        &bevy_entry.statements,
        &bevy_entry.target,
        &bevy_entry.color_expression.replace("input.uv", "mesh.uv"),
        texture_count,
    ));
    let mut node_types = main_entry.node_types.clone();
    if let Some(preview_entry) = &preview_entry {
        node_types.extend(preview_entry.node_types.iter().map(|(id, ty)| (*id, *ty)));
    }
    let uniforms = bevy_entry.uniforms.clone();
    let textures = bevy_entry.textures.clone();
    let emitted_nodes = main_entry.emitted_nodes.clone();

    validate_wgsl(&wgsl)?;

    Ok(CompiledShader {
        wesl,
        wgsl,
        bevy_wesl,
        bevy_wgsl,
        node_types,
        emitted_nodes,
        uniforms,
        textures,
    })
}

#[derive(Clone, Debug)]
struct CompiledEntry {
    target: Node,
    helpers: HelperSources,
    statements: String,
    color_expression: String,
    node_types: HashMap<NodeId, ShaderType>,
    emitted_nodes: Vec<NodeId>,
    uniforms: Vec<CompiledUniform>,
    textures: Vec<CompiledTexture>,
}

fn generate_graph_wesl(
    helpers: &str,
    statements: &str,
    output: &Node,
    color_expression: &str,
    texture_count: usize,
) -> String {
    let uniform_struct = generate_uniform_struct();
    let uniform_bindings = generate_uniform_bindings(0);
    let texture_bindings = generate_texture_declarations(texture_count, 0);
    format!(
        "// Generated WESL module\n\
         // Imports are preserved in this WESL view; linked WGSL is generated separately.\n\n\
         {helpers}\
         struct FragmentInput {{\n    @location(0) uv: vec2<f32>,\n}};\n\n\
         {uniform_struct}\
         {uniform_bindings}\
         {texture_bindings}\
         @fragment\n\
         fn fragment(input: FragmentInput) -> @location(0) vec4<f32> {{\n\
         {statements}    // node: {} \"{}\"\n\
             return {color_expression};\n\
         }}\n",
        output.id.0, output.name,
    )
}

fn compile_fragment_output(
    graph: &ShaderGraph,
    output: &Node,
) -> Result<CompiledEntry, CompileError> {
    let color_connection = output
        .inputs
        .first()
        .and_then(|connection| *connection)
        .ok_or(CompileError::MissingInput {
            node: output.id,
            input: 0,
        })?;
    let mut compiler = Compiler::new(graph);
    let color = compiler.compile_node(color_connection.node)?;
    let color_expression = convert_to_vec4(&color.expression, color.shader_type);
    Ok(compiler.finish(output.clone(), color_expression))
}

fn compile_preview_output(
    graph: &ShaderGraph,
    node_id: NodeId,
) -> Result<CompiledEntry, CompileError> {
    let target = graph
        .node(node_id)
        .ok_or(CompileError::MissingNode(node_id))?
        .clone();
    let mut compiler = Compiler::new(graph);
    let color = compiler.compile_node(node_id)?;
    if color.shader_type != ShaderType::Vec4 {
        return Err(type_mismatch(&target, "preview", &[color.shader_type]));
    }
    Ok(compiler.finish(target, color.expression))
}

fn generate_bevy_wesl(
    helpers: &str,
    statements: &str,
    output: &Node,
    color_expression: &str,
    texture_count: usize,
) -> String {
    let statements = statements.replace("input.uv", "mesh.uv");
    let uniform_struct = generate_uniform_struct();
    let uniform_bindings = generate_uniform_bindings(3);
    let texture_bindings = generate_texture_declarations(texture_count, 3);
    format!(
        "// Generated Bevy preview WESL\n\n\
         {helpers}\
         struct VertexOutput {{\n\
             @builtin(position) position: vec4<f32>,\n\
             @location(2) uv: vec2<f32>,\n\
         }}\n\n\
         {uniform_struct}\
         {uniform_bindings}\
         {texture_bindings}\
         @fragment\n\
         fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {{\n\
         {statements}    // node: {} \"{}\"\n\
             return {color_expression};\n\
         }}\n",
        output.id.0, output.name,
    )
}

fn generate_uniform_struct() -> &'static str {
    "struct GraphMaterial {\n\
         time: vec4<f32>,\n\
     }\n\n\
     struct GraphUserUniforms {\n\
         values: array<vec4<f32>>,\n\
     }\n\n"
}

fn generate_uniform_bindings(group: u32) -> String {
    format!(
        "@group({group}) @binding(0) var<uniform> material: GraphMaterial;\n\
         @group({group}) @binding(1) var<storage, read> graph_user_uniforms: GraphUserUniforms;\n\n"
    )
}

fn generate_texture_declarations(count: usize, group: u32) -> String {
    if count == 0 {
        return String::new();
    }
    let mut out = String::new();
    for i in 0..count {
        let texture_binding = 2 + i * 2;
        let sampler_binding = 3 + i * 2;
        out.push_str(&format!(
            "@group({group}) @binding({texture_binding}) var graph_texture_{i}: texture_2d<f32>;\n\
             @group({group}) @binding({sampler_binding}) var graph_sampler_{i}: sampler;\n"
        ));
    }
    out.push('\n');
    out
}

/// Links generated WESL to portable WGSL.
///
/// The starter node library does not require imports or conditional
/// translation, so stripping WESL-only module comments is sufficient. This
/// boundary is where the WESL linker will be integrated as modular nodes are
/// introduced.
fn link_wesl(wesl: &str) -> String {
    wesl.lines()
        .filter(|line| !line.starts_with("// Generated WESL module"))
        .filter(|line| !line.starts_with("// This graph currently"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim_start()
        .to_owned()
        + "\n"
}

struct Compiler<'a> {
    graph: &'a ShaderGraph,
    visiting: HashSet<NodeId>,
    compiled: HashMap<NodeId, CompiledNode>,
    statements: Vec<String>,
    helpers: HashSet<LygiaHelper>,
    emitted_nodes: Vec<NodeId>,
    uniforms: Vec<CompiledUniform>,
    textures: Vec<CompiledTexture>,
    overrides: HashMap<NodeId, CompiledNode>,
    uniform_offset: usize,
    texture_offset: usize,
}

impl Compiler<'_> {
    fn new(graph: &ShaderGraph) -> Compiler<'_> {
        Compiler {
            graph,
            visiting: HashSet::new(),
            compiled: HashMap::new(),
            statements: Vec::new(),
            helpers: HashSet::new(),
            emitted_nodes: Vec::new(),
            uniforms: Vec::new(),
            textures: Vec::new(),
            overrides: HashMap::new(),
            uniform_offset: 0,
            texture_offset: 0,
        }
    }

    fn new_with_overrides(
        graph: &ShaderGraph,
        overrides: HashMap<NodeId, CompiledNode>,
        uniform_offset: usize,
        texture_offset: usize,
    ) -> Compiler<'_> {
        Compiler {
            graph,
            visiting: HashSet::new(),
            compiled: HashMap::new(),
            statements: Vec::new(),
            helpers: HashSet::new(),
            emitted_nodes: Vec::new(),
            uniforms: Vec::new(),
            textures: Vec::new(),
            overrides,
            uniform_offset,
            texture_offset,
        }
    }

    fn finish(self, target: Node, color_expression: String) -> CompiledEntry {
        CompiledEntry {
            target,
            helpers: lygia_helper_sources(&self.helpers),
            statements: self.statements.concat(),
            color_expression,
            node_types: self
                .compiled
                .into_iter()
                .map(|(id, node)| (id, node.shader_type))
                .collect(),
            emitted_nodes: self.emitted_nodes,
            uniforms: self.uniforms,
            textures: self.textures,
        }
    }

    fn compile_node(&mut self, id: NodeId) -> Result<CompiledNode, CompileError> {
        if let Some(compiled) = self.overrides.get(&id) {
            return Ok(compiled.clone());
        }
        if let Some(compiled) = self.compiled.get(&id) {
            return Ok(compiled.clone());
        }
        if !self.visiting.insert(id) {
            return Err(CompileError::Cycle(id));
        }

        let node = self
            .graph
            .node(id)
            .ok_or(CompileError::MissingNode(id))?
            .clone();
        let compiled = self.compile_kind(&node)?;
        self.visiting.remove(&id);
        self.compiled.insert(id, compiled.clone());
        self.emitted_nodes.push(id);
        Ok(compiled)
    }

    fn compile_kind(&mut self, node: &Node) -> Result<CompiledNode, CompileError> {
        let result = match &node.kind {
            NodeKind::Constant(value) => CompiledNode {
                shader_type: value.shader_type(),
                expression: value_expression(value),
            },
            NodeKind::Uniform(value) => {
                let index = self.uniform_offset + self.uniforms.len();
                let shader_type = value.shader_type();
                self.uniforms.push(CompiledUniform {
                    node: node.id,
                    index,
                    shader_type,
                    name: node.name.clone(),
                });

                CompiledNode {
                    shader_type,
                    expression: uniform_expression(index, shader_type),
                }
            }
            NodeKind::Module(module) => {
                let mut overrides = HashMap::new();
                for (input_index, port) in module.inputs.iter().enumerate() {
                    let Some(connection) = node
                        .inputs
                        .get(input_index)
                        .and_then(|connection| *connection)
                    else {
                        continue;
                    };
                    let input = self.compile_node(connection.node)?;
                    if input.shader_type != port.shader_type {
                        return Err(type_mismatch(node, "module input", &[input.shader_type]));
                    }
                    if let Some(source_node) = port.node {
                        overrides.insert(source_node, input);
                    }
                }

                let mut nested = Compiler::new_with_overrides(
                    &module.graph,
                    overrides,
                    self.uniform_offset + self.uniforms.len(),
                    self.texture_offset + self.textures.len(),
                );
                let output = nested.compile_node(module.root)?;
                if output.shader_type != module.output.shader_type {
                    return Err(type_mismatch(node, "module output", &[output.shader_type]));
                }
                self.statements.extend(nested.statements);
                self.helpers.extend(nested.helpers);
                self.emitted_nodes.extend(nested.emitted_nodes);
                self.uniforms.extend(nested.uniforms);
                self.textures.extend(nested.textures);
                self.compiled.extend(nested.compiled);
                output
            }
            NodeKind::Uv => CompiledNode {
                shader_type: ShaderType::Vec2,
                expression: "input.uv".to_owned(),
            },
            NodeKind::Time => CompiledNode {
                shader_type: ShaderType::F32,
                expression: "material.time.x".to_owned(),
            },
            NodeKind::Add => self.compile_binary(node, "+", "add")?,
            NodeKind::Subtract => self.compile_binary(node, "-", "subtract")?,
            NodeKind::Multiply => self.compile_binary(node, "*", "multiply")?,
            NodeKind::Divide => self.compile_binary(node, "/", "divide")?,
            NodeKind::Sin => self.compile_unary(node, "sin")?,
            NodeKind::Cos => self.compile_unary(node, "cos")?,
            NodeKind::Abs => self.compile_unary(node, "abs")?,
            NodeKind::Fract => self.compile_unary(node, "fract")?,
            NodeKind::LygiaRandom => {
                let input = self.input(node, 0)?;
                let function = match input.shader_type {
                    ShaderType::F32 => {
                        self.require_lygia(LygiaHelper::Random);
                        "lygia_random"
                    }
                    ShaderType::Vec2 => {
                        self.require_lygia(LygiaHelper::Random2);
                        "lygia_random2"
                    }
                    ShaderType::Vec3 => {
                        self.require_lygia(LygiaHelper::Random3);
                        "lygia_random3"
                    }
                    ShaderType::Vec4 => {
                        self.require_lygia(LygiaHelper::Random4);
                        "lygia_random4"
                    }
                };
                CompiledNode {
                    shader_type: ShaderType::F32,
                    expression: format!("{function}({})", input.expression),
                }
            }
            NodeKind::LygiaValueNoise => {
                let input = self.input(node, 0)?;
                if input.shader_type != ShaderType::Vec2 {
                    return Err(type_mismatch(
                        node,
                        "lygia_value_noise",
                        &[input.shader_type],
                    ));
                }
                self.require_lygia(LygiaHelper::ValueNoise2);
                CompiledNode {
                    shader_type: ShaderType::F32,
                    expression: format!("lygia_value_noise2({})", input.expression),
                }
            }
            NodeKind::LygiaFbm => {
                let input = self.input(node, 0)?;
                if input.shader_type != ShaderType::Vec2 {
                    return Err(type_mismatch(node, "lygia_fbm", &[input.shader_type]));
                }
                self.require_lygia(LygiaHelper::Fbm2);
                CompiledNode {
                    shader_type: ShaderType::F32,
                    expression: format!("lygia_fbm2({})", input.expression),
                }
            }
            NodeKind::LygiaVoronoi => {
                let input = self.input(node, 0)?;
                let time = self.optional_input(node, 1)?;
                if input.shader_type != ShaderType::Vec2 {
                    return Err(type_mismatch(node, "lygia_voronoi", &[input.shader_type]));
                }
                if time.shader_type != ShaderType::F32 {
                    return Err(type_mismatch(
                        node,
                        "lygia_voronoi",
                        &[input.shader_type, time.shader_type],
                    ));
                }
                self.require_lygia(LygiaHelper::Voronoi2);
                CompiledNode {
                    shader_type: ShaderType::F32,
                    expression: format!(
                        "lygia_voronoi2_time({}, {})",
                        input.expression, time.expression
                    ),
                }
            }
            NodeKind::LygiaPow2 => self.compile_lygia_unary_dispatch(
                node,
                LygiaHelper::Pow2,
                "lygia_pow2",
                &["pow2", "pow22", "pow23", "pow24"],
            )?,
            NodeKind::LygiaPow3 => self.compile_lygia_unary_dispatch(
                node,
                LygiaHelper::Pow3,
                "lygia_pow3",
                &["pow3", "pow32", "pow33", "pow34"],
            )?,
            NodeKind::LygiaSaturate => {
                let input = self.input(node, 0)?;
                self.require_lygia(LygiaHelper::Saturate);
                CompiledNode {
                    shader_type: input.shader_type,
                    expression: format!(
                        "clamp({}, {}(0.0), {}(1.0))",
                        input.expression,
                        input.shader_type.wgsl(),
                        input.shader_type.wgsl()
                    ),
                }
            }
            NodeKind::LygiaCenter => self.compile_lygia_unary_dispatch(
                node,
                LygiaHelper::Center,
                "lygia_center",
                &["center", "center2", "center3", ""],
            )?,
            NodeKind::LygiaUncenter => self.compile_lygia_unary_dispatch(
                node,
                LygiaHelper::Uncenter,
                "lygia_uncenter",
                &["uncenter", "uncenter2", "uncenter3", ""],
            )?,
            NodeKind::LygiaScale2d => {
                let input = self.input(node, 0)?;
                let scale = self.input(node, 1)?;
                if input.shader_type != ShaderType::Vec2 || scale.shader_type != ShaderType::Vec2 {
                    return Err(type_mismatch(
                        node,
                        "lygia_scale2d",
                        &[input.shader_type, scale.shader_type],
                    ));
                }
                self.require_lygia(LygiaHelper::Scale2d);
                CompiledNode {
                    shader_type: ShaderType::Vec2,
                    expression: format!(
                        "lygia_scale2d({}, {})",
                        input.expression, scale.expression
                    ),
                }
            }
            NodeKind::LygiaRotate2d => {
                let input = self.input(node, 0)?;
                let angle = self.input(node, 1)?;
                if input.shader_type != ShaderType::Vec2 || angle.shader_type != ShaderType::F32 {
                    return Err(type_mismatch(
                        node,
                        "lygia_rotate2d",
                        &[input.shader_type, angle.shader_type],
                    ));
                }
                self.require_lygia(LygiaHelper::Rotate2d);
                CompiledNode {
                    shader_type: ShaderType::Vec2,
                    expression: format!(
                        "lygia_rotate2d({}, {})",
                        input.expression, angle.expression
                    ),
                }
            }
            NodeKind::LygiaCircleSdf => {
                let input = self.input(node, 0)?;
                let radius = self.input(node, 1)?;
                if input.shader_type != ShaderType::Vec2 || radius.shader_type != ShaderType::F32 {
                    return Err(type_mismatch(
                        node,
                        "lygia_circle_sdf",
                        &[input.shader_type, radius.shader_type],
                    ));
                }
                self.require_lygia(LygiaHelper::CircleSdf);
                CompiledNode {
                    shader_type: ShaderType::F32,
                    expression: format!(
                        "lygia_circle_sdf({}, {})",
                        input.expression, radius.expression
                    ),
                }
            }
            NodeKind::LygiaRectSdf => {
                let input = self.input(node, 0)?;
                let size = self.input(node, 1)?;
                if input.shader_type != ShaderType::Vec2 || size.shader_type != ShaderType::Vec2 {
                    return Err(type_mismatch(
                        node,
                        "lygia_rect_sdf",
                        &[input.shader_type, size.shader_type],
                    ));
                }
                self.require_lygia(LygiaHelper::RectSdf);
                CompiledNode {
                    shader_type: ShaderType::F32,
                    expression: format!(
                        "lygia_rect_sdf({}, {})",
                        input.expression, size.expression
                    ),
                }
            }
            NodeKind::LygiaBoxSdf => {
                let position = self.input(node, 0)?;
                let bounds = self.input(node, 1)?;
                if position.shader_type != ShaderType::Vec3
                    || bounds.shader_type != ShaderType::Vec3
                {
                    return Err(type_mismatch(
                        node,
                        "lygia_box_sdf",
                        &[position.shader_type, bounds.shader_type],
                    ));
                }
                self.require_lygia(LygiaHelper::BoxSdf);
                CompiledNode {
                    shader_type: ShaderType::F32,
                    expression: format!(
                        "lygia_box_sdf({}, {})",
                        position.expression, bounds.expression
                    ),
                }
            }
            NodeKind::LygiaLuma => {
                let input = self.input(node, 0)?;
                if input.shader_type != ShaderType::Vec3 && input.shader_type != ShaderType::Vec4 {
                    return Err(type_mismatch(node, "lygia_luma", &[input.shader_type]));
                }
                let expression = if input.shader_type == ShaderType::Vec4 {
                    format!("{}.rgb", input.expression)
                } else {
                    input.expression
                };
                self.require_lygia(LygiaHelper::Luma);
                CompiledNode {
                    shader_type: ShaderType::F32,
                    expression: format!("lygia_luma({expression})"),
                }
            }
            NodeKind::LygiaInvert => {
                self.compile_color_unary(node, "lygia_invert", LygiaHelper::Invert)?
            }
            NodeKind::LygiaBrightness => {
                self.compile_color_amount(node, "lygia_brightness", LygiaHelper::Brightness)?
            }
            NodeKind::LygiaContrast => {
                self.compile_color_amount(node, "lygia_contrast", LygiaHelper::Contrast)?
            }
            NodeKind::LygiaPosterize => {
                self.compile_color_amount(node, "lygia_posterize", LygiaHelper::Posterize)?
            }
            NodeKind::LygiaSaturation => {
                self.compile_color_amount(node, "lygia_saturation", LygiaHelper::Saturation)?
            }
            NodeKind::LygiaGammaCorrect => {
                self.compile_color_amount(node, "lygia_gamma_correct", LygiaHelper::GammaCorrect)?
            }
            NodeKind::LygiaBlendScreen => {
                self.compile_color_blend(node, "lygia_blend_screen", LygiaHelper::BlendScreen)?
            }
            NodeKind::LygiaBlendOverlay => {
                self.compile_color_blend(node, "lygia_blend_overlay", LygiaHelper::BlendOverlay)?
            }
            NodeKind::LygiaCosinePalette => {
                let input = self.input(node, 0)?;
                if input.shader_type != ShaderType::F32 {
                    return Err(type_mismatch(
                        node,
                        "lygia_cosine_palette",
                        &[input.shader_type],
                    ));
                }
                self.require_lygia(LygiaHelper::CosinePalette);
                CompiledNode {
                    shader_type: ShaderType::Vec3,
                    expression: format!("lygia_cosine_palette({})", input.expression),
                }
            }
            NodeKind::Normalize => {
                let input = self.input(node, 0)?;
                if input.shader_type == ShaderType::F32 {
                    return Err(type_mismatch(node, "normalize", &[input.shader_type]));
                }
                CompiledNode {
                    shader_type: input.shader_type,
                    expression: format!("normalize({})", input.expression),
                }
            }
            NodeKind::ComposeVec2 => self.compile_compose(node, 2, ShaderType::Vec2)?,
            NodeKind::ComposeVec3 => self.compile_compose(node, 3, ShaderType::Vec3)?,
            NodeKind::ComposeVec4 => self.compile_compose(node, 4, ShaderType::Vec4)?,
            NodeKind::DecomposeVec2X => self.compile_decompose(node, ShaderType::Vec2, "x")?,
            NodeKind::DecomposeVec2Y => self.compile_decompose(node, ShaderType::Vec2, "y")?,
            NodeKind::DecomposeVec3X => self.compile_decompose(node, ShaderType::Vec3, "x")?,
            NodeKind::DecomposeVec3Y => self.compile_decompose(node, ShaderType::Vec3, "y")?,
            NodeKind::DecomposeVec3Z => self.compile_decompose(node, ShaderType::Vec3, "z")?,
            NodeKind::DecomposeVec4X => self.compile_decompose(node, ShaderType::Vec4, "x")?,
            NodeKind::DecomposeVec4Y => self.compile_decompose(node, ShaderType::Vec4, "y")?,
            NodeKind::DecomposeVec4Z => self.compile_decompose(node, ShaderType::Vec4, "z")?,
            NodeKind::DecomposeVec4W => self.compile_decompose(node, ShaderType::Vec4, "w")?,
            NodeKind::TextureSample => {
                let uv = self.input(node, 0)?;
                if uv.shader_type != ShaderType::Vec2 {
                    return Err(type_mismatch(node, "texture_sample", &[uv.shader_type]));
                }
                let index = self.texture_offset + self.textures.len();
                self.textures.push(CompiledTexture {
                    node: node.id,
                    index,
                    name: node.name.clone(),
                });
                CompiledNode {
                    shader_type: ShaderType::Vec4,
                    expression: format!(
                        "textureSample(graph_texture_{index}, graph_sampler_{index}, {})",
                        uv.expression
                    ),
                }
            }
            NodeKind::FragmentOutput => unreachable!("output is compiled as an entry point"),
        };

        let variable = format!("node_{}", node.id.0);
        self.statements.push(format!(
            "    // node: {} \"{}\"\n    let {}: {} = {};\n",
            node.id.0,
            node.name,
            variable,
            result.shader_type.wgsl(),
            result.expression
        ));

        Ok(CompiledNode {
            shader_type: result.shader_type,
            expression: variable,
        })
    }

    fn input(&mut self, node: &Node, index: usize) -> Result<CompiledNode, CompileError> {
        let connection = node
            .inputs
            .get(index)
            .and_then(|connection| *connection)
            .ok_or(CompileError::MissingInput {
                node: node.id,
                input: index,
            })?;
        self.compile_node(connection.node)
    }

    fn compile_compose(
        &mut self,
        node: &Node,
        width: usize,
        shader_type: ShaderType,
    ) -> Result<CompiledNode, CompileError> {
        let mut inputs = Vec::with_capacity(width);
        for index in 0..width {
            let input = self.input(node, index)?;
            if input.shader_type != ShaderType::F32 {
                return Err(type_mismatch(node, "compose vector", &[input.shader_type]));
            }
            inputs.push(input.expression);
        }

        Ok(CompiledNode {
            shader_type,
            expression: format!("{}({})", shader_type.wgsl(), inputs.join(", ")),
        })
    }

    fn compile_decompose(
        &mut self,
        node: &Node,
        vector_type: ShaderType,
        component: &'static str,
    ) -> Result<CompiledNode, CompileError> {
        let input = self.input(node, 0)?;
        if input.shader_type != vector_type {
            return Err(type_mismatch(
                node,
                "decompose vector",
                &[input.shader_type],
            ));
        }

        Ok(CompiledNode {
            shader_type: ShaderType::F32,
            expression: format!("{}.{}", input.expression, component),
        })
    }

    fn optional_input(&mut self, node: &Node, index: usize) -> Result<CompiledNode, CompileError> {
        let Some(connection) = node.inputs.get(index).and_then(|connection| *connection) else {
            return Ok(CompiledNode {
                shader_type: ShaderType::F32,
                expression: "0.0".to_owned(),
            });
        };
        self.compile_node(connection.node)
    }

    fn require_lygia(&mut self, helper: LygiaHelper) {
        if self.helpers.insert(helper) {
            for dependency in helper.dependencies() {
                self.require_lygia(*dependency);
            }
        }
    }

    fn compile_color_unary(
        &mut self,
        node: &Node,
        function: &'static str,
        helper: LygiaHelper,
    ) -> Result<CompiledNode, CompileError> {
        let color = self.input(node, 0)?;
        let color_expression = color3_expression(node, function, &color)?;
        self.require_lygia(helper);
        Ok(CompiledNode {
            shader_type: color.shader_type,
            expression: wrap_color3_result(
                function,
                &color.expression,
                color.shader_type,
                &[color_expression],
            ),
        })
    }

    fn compile_color_amount(
        &mut self,
        node: &Node,
        function: &'static str,
        helper: LygiaHelper,
    ) -> Result<CompiledNode, CompileError> {
        let color = self.input(node, 0)?;
        let amount = self.input(node, 1)?;
        let color_expression = color3_expression(node, function, &color)?;
        if amount.shader_type != ShaderType::F32 {
            return Err(type_mismatch(
                node,
                function,
                &[color.shader_type, amount.shader_type],
            ));
        }
        self.require_lygia(helper);
        Ok(CompiledNode {
            shader_type: color.shader_type,
            expression: wrap_color3_result(
                function,
                &color.expression,
                color.shader_type,
                &[color_expression, amount.expression],
            ),
        })
    }

    fn compile_color_blend(
        &mut self,
        node: &Node,
        function: &'static str,
        helper: LygiaHelper,
    ) -> Result<CompiledNode, CompileError> {
        let base = self.input(node, 0)?;
        let blend = self.input(node, 1)?;
        let base_expression = color3_expression(node, function, &base)?;
        let blend_expression = color3_expression(node, function, &blend)?;
        self.require_lygia(helper);
        Ok(CompiledNode {
            shader_type: base.shader_type,
            expression: wrap_color3_result(
                function,
                &base.expression,
                base.shader_type,
                &[base_expression, blend_expression],
            ),
        })
    }

    fn compile_lygia_unary_dispatch(
        &mut self,
        node: &Node,
        helper: LygiaHelper,
        operation: &'static str,
        functions: &[&'static str; 4],
    ) -> Result<CompiledNode, CompileError> {
        let input = self.input(node, 0)?;
        let function = match input.shader_type {
            ShaderType::F32 => functions[0],
            ShaderType::Vec2 => functions[1],
            ShaderType::Vec3 => functions[2],
            ShaderType::Vec4 => functions[3],
        };
        if function.is_empty() {
            return Err(type_mismatch(node, operation, &[input.shader_type]));
        }
        self.require_lygia(helper);
        Ok(CompiledNode {
            shader_type: input.shader_type,
            expression: format!("{function}({})", input.expression),
        })
    }

    fn compile_unary(&mut self, node: &Node, function: &str) -> Result<CompiledNode, CompileError> {
        let input = self.input(node, 0)?;
        Ok(CompiledNode {
            shader_type: input.shader_type,
            expression: format!("{function}({})", input.expression),
        })
    }

    fn compile_binary(
        &mut self,
        node: &Node,
        operator: &str,
        operation: &'static str,
    ) -> Result<CompiledNode, CompileError> {
        let left = self.input(node, 0)?;
        let right = self.input(node, 1)?;
        let result_type =
            binary_result_type(left.shader_type, right.shader_type).ok_or_else(|| {
                type_mismatch(node, operation, &[left.shader_type, right.shader_type])
            })?;
        let left_expression = splat_if_needed(&left.expression, left.shader_type, result_type);
        let right_expression = splat_if_needed(&right.expression, right.shader_type, result_type);

        Ok(CompiledNode {
            shader_type: result_type,
            expression: format!("({left_expression} {operator} {right_expression})"),
        })
    }
}

fn binary_result_type(left: ShaderType, right: ShaderType) -> Option<ShaderType> {
    if left == right {
        Some(left)
    } else if left == ShaderType::F32 {
        Some(right)
    } else if right == ShaderType::F32 {
        Some(left)
    } else {
        None
    }
}

fn uniform_expression(index: usize, shader_type: ShaderType) -> String {
    let base = format!("graph_user_uniforms.values[{index}]");
    match shader_type {
        ShaderType::F32 => format!("{base}.x"),
        ShaderType::Vec2 => format!("{base}.xy"),
        ShaderType::Vec3 => format!("{base}.xyz"),
        ShaderType::Vec4 => base,
    }
}

fn splat_if_needed(expression: &str, source: ShaderType, target: ShaderType) -> String {
    if source == ShaderType::F32 && target != ShaderType::F32 {
        format!("{}({expression})", target.wgsl())
    } else {
        expression.to_owned()
    }
}

fn convert_to_vec4(expression: &str, shader_type: ShaderType) -> String {
    match shader_type {
        ShaderType::F32 => format!("vec4<f32>({expression}, {expression}, {expression}, 1.0)"),
        ShaderType::Vec2 => format!("vec4<f32>({expression}, 0.0, 1.0)"),
        ShaderType::Vec3 => format!("vec4<f32>({expression}, 1.0)"),
        ShaderType::Vec4 => expression.to_owned(),
    }
}

fn color3_expression(
    node: &Node,
    operation: &'static str,
    color: &CompiledNode,
) -> Result<String, CompileError> {
    match color.shader_type {
        ShaderType::Vec3 => Ok(color.expression.clone()),
        ShaderType::Vec4 => Ok(format!("{}.rgb", color.expression)),
        _ => Err(type_mismatch(node, operation, &[color.shader_type])),
    }
}

fn wrap_color3_result(
    function: &str,
    alpha_source: &str,
    shader_type: ShaderType,
    arguments: &[String],
) -> String {
    let call = format!("{function}({})", arguments.join(", "));
    if shader_type == ShaderType::Vec4 {
        format!("vec4<f32>({call}, {alpha_source}.a)")
    } else {
        call
    }
}

fn value_expression(value: &Value) -> String {
    match value {
        Value::F32(value) => float_literal(*value),
        Value::Vec2(value) => vector_literal("vec2<f32>", value),
        Value::Vec3(value) => vector_literal("vec3<f32>", value),
        Value::Vec4(value) => vector_literal("vec4<f32>", value),
    }
}

fn vector_literal<const N: usize>(constructor: &str, values: &[f32; N]) -> String {
    let values = values
        .iter()
        .map(|value| float_literal(*value))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{constructor}({values})")
}

fn float_literal(value: f32) -> String {
    let literal = value.to_string();
    if literal.contains(['.', 'e', 'E']) {
        literal
    } else {
        format!("{literal}.0")
    }
}

fn type_mismatch(node: &Node, operation: &'static str, inputs: &[ShaderType]) -> CompileError {
    CompileError::TypeMismatch {
        node: node.id,
        operation,
        inputs: inputs.to_vec(),
    }
}

fn validate_wgsl(source: &str) -> Result<(), CompileError> {
    let module = naga::front::wgsl::parse_str(source)
        .map_err(|error| CompileError::WgslParse(error.emit_to_string(source)))?;
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .map_err(|error| CompileError::WgslValidation(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Connection, Node};

    #[test]
    fn example_graph_generates_wesl_and_valid_wgsl() {
        let compiled = compile(&ShaderGraph::example()).unwrap();
        assert!(compiled.wesl.starts_with("// Generated WESL module"));
        assert!(
            compiled
                .bevy_wesl
                .starts_with("// Generated Bevy preview WESL")
        );
        assert!(compiled.bevy_wesl.contains("@location(2) uv"));
        assert!(!compiled.wgsl.contains("Generated WESL module"));
        assert!(compiled.wgsl.contains("let node_3: vec4<f32>"));
        assert_eq!(compiled.emitted_nodes, [NodeId(1), NodeId(2), NodeId(3)]);
        assert!(!compiled.wgsl.contains("fn lygia_"));
    }

    #[test]
    fn uniform_nodes_emit_material_binding() {
        let mut graph = ShaderGraph::example();
        graph.nodes[1].kind = NodeKind::Uniform(Value::F32(0.25));

        let compiled = compile(&graph).unwrap();

        assert!(
            compiled
                .wgsl
                .contains("@group(0) @binding(0) var<uniform> material")
        );
        assert!(
            compiled
                .wgsl
                .contains("let node_2: f32 = graph_user_uniforms.values[0].x;")
        );
        assert!(
            compiled
                .bevy_wesl
                .contains("@group(3) @binding(0) var<uniform> material")
        );
        assert_eq!(compiled.uniforms[0].node, NodeId(2));
        assert_eq!(compiled.uniforms[0].index, 0);
        assert_eq!(compiled.uniforms[0].shader_type, ShaderType::F32);
    }

    #[test]
    fn constants_and_uniforms_support_all_value_types() {
        let compiled = compile(&lygia_graph([
            (
                NodeId(1),
                NodeKind::Uniform(Value::Vec2([0.25, 0.5])),
                vec![],
            ),
            (
                NodeId(2),
                NodeKind::Constant(Value::Vec2([1.0, 2.0])),
                vec![],
            ),
            (NodeId(3), NodeKind::Add, vec![NodeId(1), NodeId(2)]),
        ]))
        .unwrap();
        assert!(compiled.wgsl.contains("graph_user_uniforms.values[0].xy"));
        assert!(compiled.wgsl.contains("vec2<f32>(1.0, 2.0)"));
        assert_eq!(compiled.uniforms[0].shader_type, ShaderType::Vec2);

        let compiled = compile(&lygia_graph([
            (
                NodeId(1),
                NodeKind::Uniform(Value::Vec3([0.1, 0.2, 0.3])),
                vec![],
            ),
            (
                NodeId(2),
                NodeKind::Constant(Value::Vec3([0.4, 0.5, 0.6])),
                vec![],
            ),
            (NodeId(3), NodeKind::Add, vec![NodeId(1), NodeId(2)]),
        ]))
        .unwrap();
        assert!(compiled.wgsl.contains("graph_user_uniforms.values[0].xyz"));
        assert!(compiled.wgsl.contains("vec3<f32>(0.4, 0.5, 0.6)"));
        assert_eq!(compiled.uniforms[0].shader_type, ShaderType::Vec3);

        let compiled = compile(&lygia_graph([
            (
                NodeId(1),
                NodeKind::Uniform(Value::Vec4([0.2, 0.3, 0.4, 1.0])),
                vec![],
            ),
            (
                NodeId(2),
                NodeKind::Constant(Value::Vec4([0.1, 0.1, 0.1, 0.0])),
                vec![],
            ),
            (NodeId(3), NodeKind::Add, vec![NodeId(1), NodeId(2)]),
        ]))
        .unwrap();
        assert!(compiled.wgsl.contains("graph_user_uniforms.values[0]"));
        assert!(compiled.wgsl.contains("vec4<f32>(0.1, 0.1, 0.1, 0.0)"));
        assert_eq!(compiled.uniforms[0].shader_type, ShaderType::Vec4);
    }

    #[test]
    fn lygia_nodes_emit_valid_wgsl() {
        let uv_id = NodeId(1);
        let noise_id = NodeId(2);
        let output_id = NodeId(3);

        let uv = Node::new(uv_id, NodeKind::Uv, [0.0, 0.0]);
        let mut noise = Node::new(noise_id, NodeKind::LygiaValueNoise, [180.0, 0.0]);
        noise.connect_input(0, uv_id);
        let mut output = Node::new(output_id, NodeKind::FragmentOutput, [360.0, 0.0]);
        output.connect_input(0, noise_id);
        let graph = ShaderGraph {
            format_version: crate::graph::GRAPH_FORMAT_VERSION,
            nodes: vec![uv, noise, output],
        };

        let compiled = compile(&graph).unwrap();

        assert!(
            compiled
                .wesl
                .contains("import lygia::generative::random::{")
        );
        assert!(compiled.wesl.contains("random2"));
        assert!(!compiled.wesl.contains("// LYGIA: generative/random.wgsl"));
        assert!(compiled.wgsl.contains("fn lygia_value_noise2"));
        assert!(compiled.wgsl.contains("// LYGIA: generative/random.wgsl"));
        assert!(compiled.wgsl.contains("fn lygia_random2"));
        assert!(!compiled.wgsl.contains("fn lygia_random3"));
        assert!(!compiled.wgsl.contains("fn lygia_saturation"));
        assert!(
            compiled
                .wgsl
                .contains("let node_2: f32 = lygia_value_noise2(node_1);")
        );
    }

    #[test]
    fn expanded_lygia_nodes_emit_valid_wgsl() {
        let compiled = compile(&lygia_graph([
            (NodeId(1), NodeKind::Uv, vec![]),
            (NodeId(2), NodeKind::LygiaFbm, vec![NodeId(1)]),
            (NodeId(3), NodeKind::LygiaCosinePalette, vec![NodeId(2)]),
        ]))
        .unwrap();
        assert!(compiled.wgsl.contains("fn lygia_fbm2"));
        assert!(compiled.wgsl.contains("fn lygia_cosine_palette"));

        let compiled = compile(&lygia_graph([
            (NodeId(1), NodeKind::Uv, vec![]),
            (
                NodeId(2),
                NodeKind::Constant(Value::F32(std::f32::consts::FRAC_PI_4)),
                vec![],
            ),
            (
                NodeId(3),
                NodeKind::LygiaRotate2d,
                vec![NodeId(1), NodeId(2)],
            ),
            (NodeId(4), NodeKind::Constant(Value::F32(0.35)), vec![]),
            (
                NodeId(5),
                NodeKind::LygiaCircleSdf,
                vec![NodeId(3), NodeId(4)],
            ),
        ]))
        .unwrap();
        assert!(compiled.wgsl.contains("fn lygia_rotate2d"));
        assert!(compiled.wgsl.contains("fn lygia_circle_sdf"));

        let compiled = compile(&lygia_graph([
            (
                NodeId(1),
                NodeKind::Constant(Value::Vec4([0.25, 0.5, 0.75, 1.0])),
                vec![],
            ),
            (NodeId(2), NodeKind::LygiaInvert, vec![NodeId(1)]),
            (NodeId(3), NodeKind::Constant(Value::F32(0.1)), vec![]),
            (
                NodeId(4),
                NodeKind::LygiaBrightness,
                vec![NodeId(2), NodeId(3)],
            ),
            (NodeId(5), NodeKind::Constant(Value::F32(1.25)), vec![]),
            (
                NodeId(6),
                NodeKind::LygiaContrast,
                vec![NodeId(4), NodeId(5)],
            ),
            (NodeId(7), NodeKind::Constant(Value::F32(5.0)), vec![]),
            (
                NodeId(8),
                NodeKind::LygiaPosterize,
                vec![NodeId(6), NodeId(7)],
            ),
        ]))
        .unwrap();
        assert!(compiled.wgsl.contains("fn lygia_invert"));
        assert!(compiled.wgsl.contains("fn lygia_brightness"));
        assert!(compiled.wgsl.contains("fn lygia_contrast"));
        assert!(compiled.wgsl.contains("fn lygia_posterize"));

        let compiled = compile(&lygia_graph([
            (
                NodeId(1),
                NodeKind::Constant(Value::Vec4([0.2, 0.4, 0.6, 1.0])),
                vec![],
            ),
            (
                NodeId(2),
                NodeKind::Constant(Value::Vec4([0.9, 0.3, 0.1, 1.0])),
                vec![],
            ),
            (
                NodeId(3),
                NodeKind::LygiaBlendScreen,
                vec![NodeId(1), NodeId(2)],
            ),
            (
                NodeId(4),
                NodeKind::LygiaBlendOverlay,
                vec![NodeId(3), NodeId(2)],
            ),
        ]))
        .unwrap();
        assert!(compiled.wgsl.contains("fn lygia_blend_screen"));
        assert!(compiled.wgsl.contains("fn lygia_blend_overlay"));

        let compiled = compile(&lygia_graph([
            (NodeId(1), NodeKind::Uv, vec![]),
            (NodeId(2), NodeKind::Time, vec![]),
            (
                NodeId(3),
                NodeKind::LygiaVoronoi,
                vec![NodeId(1), NodeId(2)],
            ),
        ]))
        .unwrap();
        assert!(compiled.wgsl.contains("fn lygia_voronoi2_time"));
        assert!(
            compiled
                .wgsl
                .contains("lygia_voronoi2_time(node_1, node_2)")
        );

        let compiled = compile(&lygia_graph([
            (
                NodeId(1),
                NodeKind::Constant(Value::Vec4([0.2, 0.4, 0.6, 0.8])),
                vec![],
            ),
            (NodeId(2), NodeKind::LygiaPow2, vec![NodeId(1)]),
            (NodeId(3), NodeKind::LygiaPow3, vec![NodeId(2)]),
            (NodeId(4), NodeKind::LygiaSaturate, vec![NodeId(3)]),
        ]))
        .unwrap();
        assert!(compiled.wgsl.contains("// LYGIA: math/pow2.wgsl"));
        assert!(compiled.wgsl.contains("// LYGIA: math/pow3.wgsl"));
        assert!(compiled.wgsl.contains("// LYGIA: math/saturate.wgsl"));
        assert!(compiled.wesl.contains("import lygia::math::pow2::{"));
        assert!(compiled.wesl.contains("import lygia::math::pow3::{"));
        assert!(!compiled.wesl.contains("// LYGIA: math/pow2.wgsl"));

        let compiled = compile(&lygia_graph([
            (NodeId(1), NodeKind::Uv, vec![]),
            (NodeId(2), NodeKind::LygiaCenter, vec![NodeId(1)]),
            (NodeId(3), NodeKind::LygiaUncenter, vec![NodeId(2)]),
            (
                NodeId(4),
                NodeKind::Constant(Value::Vec2([1.5, 0.75])),
                vec![],
            ),
            (
                NodeId(5),
                NodeKind::LygiaScale2d,
                vec![NodeId(3), NodeId(4)],
            ),
            (
                NodeId(6),
                NodeKind::Constant(Value::Vec2([0.75, 0.5])),
                vec![],
            ),
            (
                NodeId(7),
                NodeKind::LygiaRectSdf,
                vec![NodeId(5), NodeId(6)],
            ),
        ]))
        .unwrap();
        assert!(compiled.wgsl.contains("// LYGIA: space/center.wgsl"));
        assert!(compiled.wgsl.contains("// LYGIA: space/uncenter.wgsl"));
        assert!(compiled.wgsl.contains("// LYGIA: space/scale.wgsl"));
        assert!(compiled.wgsl.contains("// LYGIA: sdf/rectSDF.wgsl"));
        assert!(compiled.wgsl.contains("fn lygia_rect_sdf"));

        let compiled = compile(&lygia_graph([
            (
                NodeId(1),
                NodeKind::Constant(Value::Vec3([0.0, 0.1, 0.2])),
                vec![],
            ),
            (
                NodeId(2),
                NodeKind::Constant(Value::Vec3([0.5, 0.5, 0.5])),
                vec![],
            ),
            (NodeId(3), NodeKind::LygiaBoxSdf, vec![NodeId(1), NodeId(2)]),
        ]))
        .unwrap();
        assert!(compiled.wgsl.contains("// LYGIA: sdf/boxSDF.wgsl"));
        assert!(compiled.wgsl.contains("fn lygia_box_sdf"));
    }

    #[test]
    fn vector_compose_and_decompose_nodes_emit_valid_wgsl() {
        let compiled = compile(&lygia_graph([
            (NodeId(1), NodeKind::Constant(Value::F32(0.25)), vec![]),
            (NodeId(2), NodeKind::Constant(Value::F32(0.5)), vec![]),
            (NodeId(3), NodeKind::Constant(Value::F32(0.75)), vec![]),
            (NodeId(4), NodeKind::Constant(Value::F32(1.0)), vec![]),
            (
                NodeId(5),
                NodeKind::ComposeVec4,
                vec![NodeId(1), NodeId(2), NodeId(3), NodeId(4)],
            ),
            (NodeId(6), NodeKind::DecomposeVec4Z, vec![NodeId(5)]),
        ]))
        .unwrap();

        assert!(compiled.wgsl.contains("let node_5: vec4<f32> = vec4<f32>("));
        assert!(compiled.wgsl.contains("let node_6: f32 = node_5.z;"));
        assert_eq!(compiled.node_types[&NodeId(5)], ShaderType::Vec4);
        assert_eq!(compiled.node_types[&NodeId(6)], ShaderType::F32);
    }

    fn lygia_graph<const N: usize>(
        node_specs: [(NodeId, NodeKind, Vec<NodeId>); N],
    ) -> ShaderGraph {
        let mut nodes = Vec::new();
        let mut last_node = None;
        for (index, (id, kind, inputs)) in node_specs.into_iter().enumerate() {
            let mut node = Node::new(id, kind, [index as f32 * 180.0, 0.0]);
            for (input_index, source) in inputs.into_iter().enumerate() {
                node.connect_input(input_index, source);
            }
            last_node = Some(id);
            nodes.push(node);
        }
        let output_id = NodeId(10_000);
        let mut output = Node::new(
            output_id,
            NodeKind::FragmentOutput,
            [nodes.len() as f32 * 180.0, 0.0],
        );
        output.connect_input(
            0,
            last_node.expect("test graph must have at least one node"),
        );
        nodes.push(output);
        ShaderGraph {
            format_version: crate::graph::GRAPH_FORMAT_VERSION,
            nodes,
        }
    }

    #[test]
    fn time_node_uses_system_uniform_without_consuming_user_uniform_slot() {
        let mut graph = ShaderGraph::example();
        graph.nodes[1].kind = NodeKind::Time;

        let compiled = compile(&graph).unwrap();

        assert!(compiled.wgsl.contains("time: vec4<f32>"));
        assert!(compiled.wgsl.contains("let node_2: f32 = material.time.x;"));
        assert!(compiled.uniforms.is_empty());
    }

    #[test]
    fn preview_node_changes_only_bevy_preview_output() {
        let compiled = compile_with_preview_node(&ShaderGraph::example(), Some(NodeId(1))).unwrap();

        assert!(compiled.wesl.contains("return node_3;"));
        assert!(compiled.bevy_wesl.contains("// node: 1 \"Constant\""));
        assert!(compiled.bevy_wesl.contains("return node_1;"));
    }

    #[test]
    fn preview_node_requires_vec4_output() {
        let error =
            compile_with_preview_node(&ShaderGraph::example(), Some(NodeId(2))).unwrap_err();

        assert_eq!(
            error,
            CompileError::TypeMismatch {
                node: NodeId(2),
                operation: "preview",
                inputs: vec![ShaderType::F32],
            }
        );
    }

    #[test]
    fn ignores_nodes_not_reachable_from_output() {
        let mut graph = ShaderGraph::example();
        graph.nodes.push(Node::new(
            NodeId(99),
            NodeKind::Constant(Value::F32(42.0)),
            [0.0, 0.0],
        ));
        let compiled = compile(&graph).unwrap();
        assert!(!compiled.wgsl.contains("node_99"));
    }

    #[test]
    fn reports_cycles() {
        let mut graph = ShaderGraph::example();
        let multiply = graph.node(NodeId(3)).unwrap();
        let mut cyclic = multiply.clone();
        cyclic.inputs[0] = Some(Connection { node: NodeId(3) });
        *graph
            .nodes
            .iter_mut()
            .find(|node| node.id == NodeId(3))
            .unwrap() = cyclic;

        assert_eq!(compile(&graph), Err(CompileError::Cycle(NodeId(3))));
    }

    #[test]
    fn changed_constant_values_are_emitted() {
        let mut graph = ShaderGraph::example();
        graph.nodes[1].kind = NodeKind::Constant(Value::F32(0.25));

        let compiled = compile(&graph).unwrap();
        assert!(compiled.wesl.contains("let node_2: f32 = 0.25;"));
        assert!(compiled.bevy_wesl.contains("let node_2: f32 = 0.25;"));
    }
}
