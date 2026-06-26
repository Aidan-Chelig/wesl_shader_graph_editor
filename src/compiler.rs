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

pub fn compile(graph: &ShaderGraph) -> Result<CompiledShader, CompileError> {
    compile_with_preview_node(graph, None)
}

pub fn compile_with_preview_node(
    graph: &ShaderGraph,
    preview_node: Option<NodeId>,
) -> Result<CompiledShader, CompileError> {
    let output = graph.fragment_output().ok_or(CompileError::MissingOutput)?;
    let main_entry = compile_fragment_output(graph, output)?;
    let uniform_struct = generate_uniform_struct();
    let uniform_bindings = generate_uniform_bindings(0);
    let texture_bindings = generate_texture_declarations(main_entry.textures.len(), 0);
    let wesl = format!(
        "// Generated WESL module\n\
         // This graph currently uses only WESL's WGSL-compatible subset.\n\n\
         struct FragmentInput {{\n    @location(0) uv: vec2<f32>,\n}};\n\n\
         {uniform_struct}\
         {uniform_bindings}\
         {texture_bindings}\
         @fragment\n\
         fn fragment(input: FragmentInput) -> @location(0) vec4<f32> {{\n\
         {}    // node: {} \"{}\"\n\
             return {};\n\
         }}\n",
        main_entry.statements, output.id.0, output.name, main_entry.color_expression,
    );
    let wgsl = link_wesl(&wesl);
    let preview_entry = if let Some(preview_node) = preview_node {
        Some(compile_preview_output(graph, preview_node)?)
    } else {
        None
    };
    let bevy_entry = preview_entry.as_ref().unwrap_or(&main_entry);
    let texture_count = bevy_entry.textures.len().max(1);
    let bevy_wesl = generate_bevy_wesl(
        &bevy_entry.statements,
        &bevy_entry.target,
        &bevy_entry.color_expression.replace("input.uv", "mesh.uv"),
        texture_count,
    );
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
        node_types,
        emitted_nodes,
        uniforms,
        textures,
    })
}

#[derive(Clone, Debug)]
struct CompiledEntry {
    target: Node,
    statements: String,
    color_expression: String,
    node_types: HashMap<NodeId, ShaderType>,
    emitted_nodes: Vec<NodeId>,
    uniforms: Vec<CompiledUniform>,
    textures: Vec<CompiledTexture>,
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
    emitted_nodes: Vec<NodeId>,
    uniforms: Vec<CompiledUniform>,
    textures: Vec<CompiledTexture>,
}

impl Compiler<'_> {
    fn new(graph: &ShaderGraph) -> Compiler<'_> {
        Compiler {
            graph,
            visiting: HashSet::new(),
            compiled: HashMap::new(),
            statements: Vec::new(),
            emitted_nodes: Vec::new(),
            uniforms: Vec::new(),
            textures: Vec::new(),
        }
    }

    fn finish(self, target: Node, color_expression: String) -> CompiledEntry {
        CompiledEntry {
            target,
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
                let index = self.uniforms.len();
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
            NodeKind::TextureSample => {
                let uv = self.input(node, 0)?;
                if uv.shader_type != ShaderType::Vec2 {
                    return Err(type_mismatch(
                        node,
                        "texture_sample",
                        &[uv.shader_type],
                    ));
                }
                let index = self.textures.len();
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
