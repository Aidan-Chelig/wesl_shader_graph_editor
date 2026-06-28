use serde::{Deserialize, Serialize};

pub const GRAPH_FORMAT_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct NodeId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ShaderType {
    F32,
    Vec2,
    Vec3,
    Vec4,
}

impl ShaderType {
    pub fn wgsl(self) -> &'static str {
        match self {
            Self::F32 => "f32",
            Self::Vec2 => "vec2<f32>",
            Self::Vec3 => "vec3<f32>",
            Self::Vec4 => "vec4<f32>",
        }
    }

    pub fn width(self) -> usize {
        match self {
            Self::F32 => 1,
            Self::Vec2 => 2,
            Self::Vec3 => 3,
            Self::Vec4 => 4,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Value {
    F32(f32),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
}

impl Value {
    pub fn shader_type(&self) -> ShaderType {
        match self {
            Self::F32(_) => ShaderType::F32,
            Self::Vec2(_) => ShaderType::Vec2,
            Self::Vec3(_) => ShaderType::Vec3,
            Self::Vec4(_) => ShaderType::Vec4,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModulePort {
    pub name: String,
    pub shader_type: ShaderType,
    pub node: Option<NodeId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModuleDefinition {
    pub id: u64,
    pub name: String,
    pub root: NodeId,
    pub nodes: Vec<NodeId>,
    pub inputs: Vec<ModulePort>,
    pub output: ModulePort,
    pub graph: Box<ShaderGraph>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum NodeKind {
    Constant(Value),
    Uniform(Value),
    Module(Box<ModuleDefinition>),
    Uv,
    Time,
    Add,
    Subtract,
    Multiply,
    Divide,
    Sin,
    Cos,
    Abs,
    Fract,
    Normalize,
    ComposeVec2,
    ComposeVec3,
    ComposeVec4,
    DecomposeVec2X,
    DecomposeVec2Y,
    DecomposeVec3X,
    DecomposeVec3Y,
    DecomposeVec3Z,
    DecomposeVec4X,
    DecomposeVec4Y,
    DecomposeVec4Z,
    DecomposeVec4W,
    LygiaRandom,
    LygiaValueNoise,
    LygiaFbm,
    LygiaVoronoi,
    LygiaPow2,
    LygiaPow3,
    LygiaSaturate,
    LygiaCenter,
    LygiaUncenter,
    LygiaScale2d,
    LygiaRotate2d,
    LygiaCircleSdf,
    LygiaRectSdf,
    LygiaBoxSdf,
    LygiaLuma,
    LygiaInvert,
    LygiaBrightness,
    LygiaContrast,
    LygiaPosterize,
    LygiaSaturation,
    LygiaGammaCorrect,
    LygiaBlendScreen,
    LygiaBlendOverlay,
    LygiaCosinePalette,
    TextureSample,
    FragmentOutput,
}

impl NodeKind {
    pub fn title(&self) -> &'static str {
        match self {
            Self::Constant(_) => "Constant",
            Self::Uniform(_) => "Uniform",
            Self::Module(_) => "Module",
            Self::Uv => "UV",
            Self::Time => "Time",
            Self::Add => "Add",
            Self::Subtract => "Subtract",
            Self::Multiply => "Multiply",
            Self::Divide => "Divide",
            Self::Sin => "Sine",
            Self::Cos => "Cosine",
            Self::Abs => "Absolute",
            Self::Fract => "Fract",
            Self::Normalize => "Normalize",
            Self::ComposeVec2 => "Compose vec2",
            Self::ComposeVec3 => "Compose vec3",
            Self::ComposeVec4 => "Compose vec4",
            Self::DecomposeVec2X => "Vec2 X",
            Self::DecomposeVec2Y => "Vec2 Y",
            Self::DecomposeVec3X => "Vec3 X",
            Self::DecomposeVec3Y => "Vec3 Y",
            Self::DecomposeVec3Z => "Vec3 Z",
            Self::DecomposeVec4X => "Vec4 X",
            Self::DecomposeVec4Y => "Vec4 Y",
            Self::DecomposeVec4Z => "Vec4 Z",
            Self::DecomposeVec4W => "Vec4 W",
            Self::LygiaRandom => "LYGIA Random",
            Self::LygiaValueNoise => "LYGIA Value Noise",
            Self::LygiaFbm => "LYGIA FBM",
            Self::LygiaVoronoi => "LYGIA Voronoi",
            Self::LygiaPow2 => "LYGIA Pow2",
            Self::LygiaPow3 => "LYGIA Pow3",
            Self::LygiaSaturate => "LYGIA Saturate",
            Self::LygiaCenter => "LYGIA Center",
            Self::LygiaUncenter => "LYGIA Uncenter",
            Self::LygiaScale2d => "LYGIA Scale 2D",
            Self::LygiaRotate2d => "LYGIA Rotate 2D",
            Self::LygiaCircleSdf => "LYGIA Circle SDF",
            Self::LygiaRectSdf => "LYGIA Rect SDF",
            Self::LygiaBoxSdf => "LYGIA Box SDF",
            Self::LygiaLuma => "LYGIA Luma",
            Self::LygiaInvert => "LYGIA Invert",
            Self::LygiaBrightness => "LYGIA Brightness",
            Self::LygiaContrast => "LYGIA Contrast",
            Self::LygiaPosterize => "LYGIA Posterize",
            Self::LygiaSaturation => "LYGIA Saturation",
            Self::LygiaGammaCorrect => "LYGIA Gamma Correct",
            Self::LygiaBlendScreen => "LYGIA Blend Screen",
            Self::LygiaBlendOverlay => "LYGIA Blend Overlay",
            Self::LygiaCosinePalette => "LYGIA Cosine Palette",
            Self::TextureSample => "Texture Sample",
            Self::FragmentOutput => "Fragment Output",
        }
    }

    pub fn input_count(&self) -> usize {
        match self {
            Self::Constant(_) | Self::Uniform(_) | Self::Uv | Self::Time => 0,
            Self::Module(module) => module.inputs.len(),
            Self::Sin
            | Self::Cos
            | Self::Abs
            | Self::Fract
            | Self::Normalize
            | Self::LygiaRandom
            | Self::LygiaValueNoise
            | Self::LygiaFbm
            | Self::LygiaPow2
            | Self::LygiaPow3
            | Self::LygiaSaturate
            | Self::LygiaCenter
            | Self::LygiaUncenter
            | Self::LygiaLuma
            | Self::LygiaInvert
            | Self::LygiaCosinePalette
            | Self::DecomposeVec2X
            | Self::DecomposeVec2Y
            | Self::DecomposeVec3X
            | Self::DecomposeVec3Y
            | Self::DecomposeVec3Z
            | Self::DecomposeVec4X
            | Self::DecomposeVec4Y
            | Self::DecomposeVec4Z
            | Self::DecomposeVec4W
            | Self::FragmentOutput
            | Self::TextureSample => 1,
            Self::Add
            | Self::Subtract
            | Self::Multiply
            | Self::Divide
            | Self::LygiaVoronoi
            | Self::LygiaScale2d
            | Self::LygiaRotate2d
            | Self::LygiaCircleSdf
            | Self::LygiaRectSdf
            | Self::LygiaBoxSdf
            | Self::LygiaBrightness
            | Self::LygiaContrast
            | Self::LygiaPosterize
            | Self::LygiaSaturation
            | Self::LygiaGammaCorrect
            | Self::LygiaBlendScreen
            | Self::LygiaBlendOverlay => 2,
            Self::ComposeVec3 => 3,
            Self::ComposeVec4 => 4,
            Self::ComposeVec2 => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Connection {
    pub node: NodeId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub name: String,
    pub position: [f32; 2],
    pub kind: NodeKind,
    pub inputs: Vec<Option<Connection>>,
}

impl Node {
    pub fn new(id: NodeId, kind: NodeKind, position: [f32; 2]) -> Self {
        let input_count = kind.input_count();
        Self {
            id,
            name: kind.title().to_owned(),
            position,
            kind,
            inputs: vec![None; input_count],
        }
    }

    pub fn connect_input(&mut self, input: usize, source: NodeId) {
        if let Some(socket) = self.inputs.get_mut(input) {
            *socket = Some(Connection { node: source });
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShaderGraph {
    pub format_version: u32,
    pub nodes: Vec<Node>,
}

impl Default for ShaderGraph {
    fn default() -> Self {
        Self {
            format_version: GRAPH_FORMAT_VERSION,
            nodes: Vec::new(),
        }
    }
}

impl ShaderGraph {
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == id)
    }

    pub fn fragment_output(&self) -> Option<&Node> {
        self.nodes
            .iter()
            .find(|node| matches!(node.kind, NodeKind::FragmentOutput))
    }

    pub fn example() -> Self {
        let color_id = NodeId(1);
        let strength_id = NodeId(2);
        let multiply_id = NodeId(3);
        let output_id = NodeId(4);

        let color = Node::new(
            color_id,
            NodeKind::Constant(Value::Vec4([0.12, 0.55, 1.0, 1.0])),
            [60.0, 80.0],
        );
        let strength = Node::new(
            strength_id,
            NodeKind::Constant(Value::F32(0.8)),
            [60.0, 260.0],
        );
        let mut multiply = Node::new(multiply_id, NodeKind::Multiply, [330.0, 150.0]);
        multiply.connect_input(0, color_id);
        multiply.connect_input(1, strength_id);

        let mut output = Node::new(output_id, NodeKind::FragmentOutput, [600.0, 150.0]);
        output.connect_input(0, multiply_id);

        Self {
            format_version: GRAPH_FORMAT_VERSION,
            nodes: vec![color, strength, multiply, output],
        }
    }
}
