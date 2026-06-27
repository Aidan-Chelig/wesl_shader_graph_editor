use std::borrow::Cow;

use bevy_egui::egui;
use egui_graph_edit::{
    AnyParameterId, CategoryTrait, DataTypeTrait, Graph, GraphEditorState, InputParamKind,
    NodeDataTrait, NodeId, NodeResponse, NodeTemplateIter, NodeTemplateTrait, UserResponseTrait,
    WidgetValueTrait,
};
use wesl_shader_graph_editor::graph::{
    Connection, GRAPH_FORMAT_VERSION, Node, NodeId as ShaderNodeId, NodeKind, ShaderGraph,
    ShaderType, Value,
};

pub type ShaderGraphEditorState =
    GraphEditorState<GraphNodeData, GraphDataType, GraphValue, GraphNodeTemplate, GraphUiState>;

#[derive(Clone, Debug)]
pub struct GraphUiState {
    pub next_shader_node_id: u64,
    pub preview_node: Option<ShaderNodeId>,
    pub texture_path: Option<String>,
    pub connection_context: Option<ConnectionContext>,
    pub value_changed: bool,
    pub preview_changed: bool,
}

impl GraphUiState {
    pub fn from_graph(graph: &ShaderGraph) -> Self {
        Self {
            next_shader_node_id: graph.nodes.iter().map(|node| node.id.0).max().unwrap_or(0) + 1,
            preview_node: None,
            texture_path: None,
            connection_context: None,
            value_changed: false,
            preview_changed: false,
        }
    }

    fn allocate_shader_node_id(&mut self) -> ShaderNodeId {
        let id = ShaderNodeId(self.next_shader_node_id);
        self.next_shader_node_id += 1;
        id
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ConnectionContext {
    pub origin: AnyParameterId,
    pub data_type: GraphDataType,
}

impl ConnectionContext {
    fn needs_template_input(self) -> bool {
        matches!(self.origin, AnyParameterId::Output(_))
    }
}

#[derive(Clone, Debug)]
pub struct GraphNodeData {
    pub shader_id: ShaderNodeId,
    pub kind: NodeKind,
}

#[derive(Clone, Debug, Default)]
pub enum GraphValue {
    #[default]
    None,
}

#[derive(Clone, Copy, Debug, Eq)]
pub enum GraphDataType {
    Any,
    F32,
    Vec2,
    Vec3,
    Vec4,
}

impl PartialEq for GraphDataType {
    fn eq(&self, other: &Self) -> bool {
        matches!((self, other), (Self::Any, _) | (_, Self::Any)) || self.same_exact_type(*other)
    }
}

impl GraphDataType {
    fn same_exact_type(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::F32, Self::F32)
                | (Self::Vec2, Self::Vec2)
                | (Self::Vec3, Self::Vec3)
                | (Self::Vec4, Self::Vec4)
                | (Self::Any, Self::Any)
        )
    }

    fn from_shader_type(shader_type: ShaderType) -> Self {
        match shader_type {
            ShaderType::F32 => Self::F32,
            ShaderType::Vec2 => Self::Vec2,
            ShaderType::Vec3 => Self::Vec3,
            ShaderType::Vec4 => Self::Vec4,
        }
    }

    fn output_for_kind(kind: &NodeKind) -> Option<Self> {
        match kind {
            NodeKind::Constant(value) | NodeKind::Uniform(value) => {
                Some(Self::from_shader_type(value.shader_type()))
            }
            NodeKind::Module(module) => Some(Self::from_shader_type(module.output.shader_type)),
            NodeKind::Uv => Some(Self::Vec2),
            NodeKind::Time => Some(Self::F32),
            NodeKind::FragmentOutput => None,
            NodeKind::Add
            | NodeKind::Subtract
            | NodeKind::Multiply
            | NodeKind::Divide
            | NodeKind::Sin
            | NodeKind::Cos
            | NodeKind::Abs
            | NodeKind::Fract
            | NodeKind::Normalize
            | NodeKind::LygiaPow2
            | NodeKind::LygiaPow3
            | NodeKind::LygiaSaturate
            | NodeKind::LygiaCenter
            | NodeKind::LygiaUncenter => Some(Self::Any),
            NodeKind::LygiaRandom
            | NodeKind::LygiaValueNoise
            | NodeKind::LygiaFbm
            | NodeKind::LygiaVoronoi
            | NodeKind::LygiaCircleSdf
            | NodeKind::LygiaRectSdf
            | NodeKind::LygiaBoxSdf
            | NodeKind::LygiaLuma => Some(Self::F32),
            NodeKind::LygiaScale2d | NodeKind::LygiaRotate2d => Some(Self::Vec2),
            NodeKind::LygiaCosinePalette => Some(Self::Vec3),
            NodeKind::LygiaInvert
            | NodeKind::LygiaBrightness
            | NodeKind::LygiaContrast
            | NodeKind::LygiaPosterize
            | NodeKind::LygiaSaturation
            | NodeKind::LygiaGammaCorrect
            | NodeKind::LygiaBlendScreen
            | NodeKind::LygiaBlendOverlay => Some(Self::Any),
            NodeKind::TextureSample => Some(Self::Vec4),
        }
    }
}

impl DataTypeTrait<GraphUiState> for GraphDataType {
    fn data_type_color(&self, _user_state: &mut GraphUiState) -> egui::Color32 {
        match self {
            Self::F32 => egui::Color32::from_rgb(245, 190, 90),
            Self::Vec2 => egui::Color32::from_rgb(95, 210, 170),
            Self::Vec3 => egui::Color32::from_rgb(95, 170, 255),
            Self::Vec4 => egui::Color32::from_rgb(220, 115, 255),
            Self::Any => egui::Color32::from_rgb(120, 135, 155),
        }
    }

    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed(match self {
            Self::Any => "value",
            Self::F32 => "f32",
            Self::Vec2 => "vec2",
            Self::Vec3 => "vec3",
            Self::Vec4 => "vec4",
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub enum GraphValueChangeKind {
    Source,
    Uniform,
}

#[derive(Clone, Debug)]
pub enum GraphResponse {
    ValueChanged {
        node: ShaderNodeId,
        value: Value,
        kind: GraphValueChangeKind,
    },
    RenameNode {
        node: ShaderNodeId,
        name: String,
    },
    LoadTextureRequested,
    PreviewChanged,
}

impl UserResponseTrait for GraphResponse {}

impl WidgetValueTrait for GraphValue {
    type Response = GraphResponse;
    type UserState = GraphUiState;
    type NodeData = GraphNodeData;

    fn value_widget(
        &mut self,
        param_name: &str,
        _node_id: NodeId,
        ui: &mut egui::Ui,
        _user_state: &mut Self::UserState,
        _node_data: &Self::NodeData,
    ) -> Vec<Self::Response> {
        ui.label(param_name);
        Vec::new()
    }
}

impl NodeDataTrait for GraphNodeData {
    type Response = GraphResponse;
    type UserState = GraphUiState;
    type DataType = GraphDataType;
    type ValueType = GraphValue;

    fn bottom_ui(
        &self,
        ui: &mut egui::Ui,
        node_id: NodeId,
        graph: &Graph<Self, Self::DataType, Self::ValueType>,
        user_state: &mut Self::UserState,
    ) -> Vec<NodeResponse<Self::Response, Self>> {
        let mut responses = Vec::new();
        ui.push_id(("bottom", self.shader_id.0), |ui| match &self.kind {
            NodeKind::Constant(value) => {
                draw_value(
                    ui,
                    self.shader_id,
                    value.clone(),
                    GraphValueChangeKind::Source,
                    &mut responses,
                    user_state,
                );
            }
            NodeKind::Uniform(value) => {
                let mut name = graph.nodes[node_id].label.clone();
                ui.horizontal(|ui| {
                    ui.label("Name");
                    if ui.text_edit_singleline(&mut name).changed() {
                        responses.push(NodeResponse::User(GraphResponse::RenameNode {
                            node: self.shader_id,
                            name: name.trim().to_owned(),
                        }));
                    }
                });
                ui.label("Preview value");
                draw_value(
                    ui,
                    self.shader_id,
                    value.clone(),
                    GraphValueChangeKind::Uniform,
                    &mut responses,
                    user_state,
                );
            }
            NodeKind::TextureSample => {
                let texture_label = user_state
                    .texture_path
                    .as_deref()
                    .and_then(|path| std::path::Path::new(path).file_name())
                    .and_then(|file_name| file_name.to_str())
                    .unwrap_or("No texture selected");
                ui.label(
                    egui::RichText::new(texture_label)
                        .monospace()
                        .color(egui::Color32::from_gray(210)),
                )
                .on_hover_text(
                    user_state
                        .texture_path
                        .as_deref()
                        .unwrap_or("No texture selected"),
                );
                if ui.button("Load Texture...").clicked() {
                    responses.push(NodeResponse::User(GraphResponse::LoadTextureRequested));
                }
            }
            kind => {
                ui.label(
                    egui::RichText::new(kind.title())
                        .monospace()
                        .color(egui::Color32::from_gray(190)),
                );
            }
        });
        responses
    }

    fn top_bar_ui(
        &self,
        ui: &mut egui::Ui,
        _node_id: NodeId,
        _graph: &Graph<Self, Self::DataType, Self::ValueType>,
        user_state: &mut Self::UserState,
    ) -> Vec<NodeResponse<Self::Response, Self>> {
        let mut responses = Vec::new();
        ui.push_id(("top", self.shader_id.0), |ui| {
            if GraphDataType::output_for_kind(&self.kind) == Some(GraphDataType::Vec4) {
                let active = user_state.preview_node == Some(self.shader_id);
                let label = if active { "◉" } else { "○" };
                if ui
                    .small_button(label)
                    .on_hover_text("Preview this vec4 node")
                    .clicked()
                {
                    user_state.preview_node = if active { None } else { Some(self.shader_id) };
                    user_state.preview_changed = true;
                    responses.push(NodeResponse::User(GraphResponse::PreviewChanged));
                }
            }
        });
        responses
    }

    fn titlebar_color(
        &self,
        _ui: &egui::Ui,
        _node_id: NodeId,
        _graph: &Graph<Self, Self::DataType, Self::ValueType>,
        _user_state: &mut Self::UserState,
    ) -> Option<egui::Color32> {
        let color = GraphDataType::output_for_kind(&self.kind)?.data_type_color(_user_state);
        Some(egui::Color32::from_rgb(
            (color.r() / 3).saturating_add(28),
            (color.g() / 3).saturating_add(34),
            (color.b() / 3).saturating_add(48),
        ))
    }
}

fn draw_value(
    ui: &mut egui::Ui,
    shader_id: ShaderNodeId,
    mut value: Value,
    change_kind: GraphValueChangeKind,
    responses: &mut Vec<NodeResponse<GraphResponse, GraphNodeData>>,
    user_state: &mut GraphUiState,
) {
    let changed = match &mut value {
        Value::F32(value) => {
            ui.horizontal(|ui| {
                ui.label("Value");
                ui.add(
                    egui::DragValue::new(value)
                        .speed(0.01)
                        .max_decimals(4)
                        .min_decimals(1),
                )
                .changed()
            })
            .inner
        }
        Value::Vec2(values) => draw_vector(ui, values, &["X", "Y"]),
        Value::Vec3(values) => draw_vector(ui, values, &["X", "Y", "Z"]),
        Value::Vec4(values) => {
            ui.horizontal(|ui| {
                ui.label("Color");
                let mut color =
                    egui::Rgba::from_rgba_unmultiplied(values[0], values[1], values[2], values[3]);
                let changed = egui::widgets::color_picker::color_edit_button_rgba(
                    ui,
                    &mut color,
                    egui::widgets::color_picker::Alpha::BlendOrAdditive,
                )
                .changed();
                if changed {
                    *values = color.to_array();
                }
                changed
            })
            .inner
        }
    };
    if changed {
        user_state.value_changed = true;
        responses.push(NodeResponse::User(GraphResponse::ValueChanged {
            node: shader_id,
            value,
            kind: change_kind,
        }));
    }
}

fn draw_vector<const N: usize>(
    ui: &mut egui::Ui,
    values: &mut [f32; N],
    labels: &[&str; N],
) -> bool {
    ui.vertical(|ui| {
        let mut changed = false;
        for (label, value) in labels.iter().zip(values.iter_mut()) {
            ui.horizontal(|ui| {
                ui.label(*label);
                changed |= ui
                    .add(
                        egui::DragValue::new(value)
                            .speed(0.01)
                            .max_decimals(4)
                            .min_decimals(1),
                    )
                    .changed();
            });
        }
        changed
    })
    .inner
}

#[derive(Clone, Debug)]
pub enum GraphNodeTemplate {
    Uv,
    Time,
    F32Constant,
    Vec2Constant,
    Vec3Constant,
    Vec4Constant,
    F32Uniform,
    Vec2Uniform,
    Vec3Uniform,
    Vec4Uniform,
    Add,
    Subtract,
    Multiply,
    Divide,
    Sin,
    Cos,
    Abs,
    Fract,
    Normalize,
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
}

impl GraphNodeTemplate {
    pub const TOOLBAR: [Self; 44] = [
        Self::Uv,
        Self::Time,
        Self::F32Constant,
        Self::Vec2Constant,
        Self::Vec3Constant,
        Self::Vec4Constant,
        Self::F32Uniform,
        Self::Vec2Uniform,
        Self::Vec3Uniform,
        Self::Vec4Uniform,
        Self::Add,
        Self::Subtract,
        Self::Multiply,
        Self::Divide,
        Self::Sin,
        Self::Cos,
        Self::Abs,
        Self::Fract,
        Self::Normalize,
        Self::LygiaRandom,
        Self::LygiaValueNoise,
        Self::LygiaFbm,
        Self::LygiaVoronoi,
        Self::LygiaPow2,
        Self::LygiaPow3,
        Self::LygiaSaturate,
        Self::LygiaCenter,
        Self::LygiaUncenter,
        Self::LygiaScale2d,
        Self::LygiaRotate2d,
        Self::LygiaCircleSdf,
        Self::LygiaRectSdf,
        Self::LygiaBoxSdf,
        Self::LygiaLuma,
        Self::LygiaInvert,
        Self::LygiaBrightness,
        Self::LygiaContrast,
        Self::LygiaPosterize,
        Self::LygiaSaturation,
        Self::LygiaGammaCorrect,
        Self::LygiaBlendScreen,
        Self::LygiaBlendOverlay,
        Self::LygiaCosinePalette,
        Self::TextureSample,
    ];

    fn kind(&self) -> NodeKind {
        match self {
            Self::Uv => NodeKind::Uv,
            Self::Time => NodeKind::Time,
            Self::F32Constant => NodeKind::Constant(Value::F32(1.0)),
            Self::Vec2Constant => NodeKind::Constant(Value::Vec2([1.0, 1.0])),
            Self::Vec3Constant => NodeKind::Constant(Value::Vec3([1.0, 1.0, 1.0])),
            Self::Vec4Constant => NodeKind::Constant(Value::Vec4([1.0, 1.0, 1.0, 1.0])),
            Self::F32Uniform => NodeKind::Uniform(Value::F32(1.0)),
            Self::Vec2Uniform => NodeKind::Uniform(Value::Vec2([1.0, 1.0])),
            Self::Vec3Uniform => NodeKind::Uniform(Value::Vec3([1.0, 1.0, 1.0])),
            Self::Vec4Uniform => NodeKind::Uniform(Value::Vec4([1.0, 1.0, 1.0, 1.0])),
            Self::Add => NodeKind::Add,
            Self::Subtract => NodeKind::Subtract,
            Self::Multiply => NodeKind::Multiply,
            Self::Divide => NodeKind::Divide,
            Self::Sin => NodeKind::Sin,
            Self::Cos => NodeKind::Cos,
            Self::Abs => NodeKind::Abs,
            Self::Fract => NodeKind::Fract,
            Self::Normalize => NodeKind::Normalize,
            Self::LygiaRandom => NodeKind::LygiaRandom,
            Self::LygiaValueNoise => NodeKind::LygiaValueNoise,
            Self::LygiaFbm => NodeKind::LygiaFbm,
            Self::LygiaVoronoi => NodeKind::LygiaVoronoi,
            Self::LygiaPow2 => NodeKind::LygiaPow2,
            Self::LygiaPow3 => NodeKind::LygiaPow3,
            Self::LygiaSaturate => NodeKind::LygiaSaturate,
            Self::LygiaCenter => NodeKind::LygiaCenter,
            Self::LygiaUncenter => NodeKind::LygiaUncenter,
            Self::LygiaScale2d => NodeKind::LygiaScale2d,
            Self::LygiaRotate2d => NodeKind::LygiaRotate2d,
            Self::LygiaCircleSdf => NodeKind::LygiaCircleSdf,
            Self::LygiaRectSdf => NodeKind::LygiaRectSdf,
            Self::LygiaBoxSdf => NodeKind::LygiaBoxSdf,
            Self::LygiaLuma => NodeKind::LygiaLuma,
            Self::LygiaInvert => NodeKind::LygiaInvert,
            Self::LygiaBrightness => NodeKind::LygiaBrightness,
            Self::LygiaContrast => NodeKind::LygiaContrast,
            Self::LygiaPosterize => NodeKind::LygiaPosterize,
            Self::LygiaSaturation => NodeKind::LygiaSaturation,
            Self::LygiaGammaCorrect => NodeKind::LygiaGammaCorrect,
            Self::LygiaBlendScreen => NodeKind::LygiaBlendScreen,
            Self::LygiaBlendOverlay => NodeKind::LygiaBlendOverlay,
            Self::LygiaCosinePalette => NodeKind::LygiaCosinePalette,
            Self::TextureSample => NodeKind::TextureSample,
        }
    }

    fn compatible_with_connection(&self, filter: ConnectionContext) -> bool {
        if filter.needs_template_input() {
            self.input_types()
                .into_iter()
                .any(|input_type| input_type == filter.data_type)
        } else {
            GraphDataType::output_for_kind(&self.kind())
                .is_some_and(|output_type| output_type == filter.data_type)
        }
    }

    fn input_types(&self) -> Vec<GraphDataType> {
        let kind = self.kind();
        (0..kind.input_count())
            .map(|input_index| input_type(&kind, input_index))
            .collect()
    }
}

#[derive(Clone, Debug, Default)]
pub struct GraphNodeTemplates {
    filter: Option<ConnectionContext>,
}

impl GraphNodeTemplates {
    pub fn compatible_with(filter: Option<ConnectionContext>) -> Self {
        Self { filter }
    }
}

impl NodeTemplateIter for GraphNodeTemplates {
    type Item = GraphNodeTemplate;

    fn all_kinds(&self) -> Vec<Self::Item> {
        let templates = vec![
            GraphNodeTemplate::Uv,
            GraphNodeTemplate::Time,
            GraphNodeTemplate::F32Constant,
            GraphNodeTemplate::Vec2Constant,
            GraphNodeTemplate::Vec3Constant,
            GraphNodeTemplate::Vec4Constant,
            GraphNodeTemplate::F32Uniform,
            GraphNodeTemplate::Vec2Uniform,
            GraphNodeTemplate::Vec3Uniform,
            GraphNodeTemplate::Vec4Uniform,
            GraphNodeTemplate::Add,
            GraphNodeTemplate::Subtract,
            GraphNodeTemplate::Multiply,
            GraphNodeTemplate::Divide,
            GraphNodeTemplate::Sin,
            GraphNodeTemplate::Cos,
            GraphNodeTemplate::Abs,
            GraphNodeTemplate::Fract,
            GraphNodeTemplate::Normalize,
            GraphNodeTemplate::LygiaRandom,
            GraphNodeTemplate::LygiaValueNoise,
            GraphNodeTemplate::LygiaFbm,
            GraphNodeTemplate::LygiaVoronoi,
            GraphNodeTemplate::LygiaPow2,
            GraphNodeTemplate::LygiaPow3,
            GraphNodeTemplate::LygiaSaturate,
            GraphNodeTemplate::LygiaCenter,
            GraphNodeTemplate::LygiaUncenter,
            GraphNodeTemplate::LygiaScale2d,
            GraphNodeTemplate::LygiaRotate2d,
            GraphNodeTemplate::LygiaCircleSdf,
            GraphNodeTemplate::LygiaRectSdf,
            GraphNodeTemplate::LygiaBoxSdf,
            GraphNodeTemplate::LygiaLuma,
            GraphNodeTemplate::LygiaInvert,
            GraphNodeTemplate::LygiaBrightness,
            GraphNodeTemplate::LygiaContrast,
            GraphNodeTemplate::LygiaPosterize,
            GraphNodeTemplate::LygiaSaturation,
            GraphNodeTemplate::LygiaGammaCorrect,
            GraphNodeTemplate::LygiaBlendScreen,
            GraphNodeTemplate::LygiaBlendOverlay,
            GraphNodeTemplate::LygiaCosinePalette,
            GraphNodeTemplate::TextureSample,
        ];
        if let Some(filter) = self.filter {
            templates
                .into_iter()
                .filter(|template| template.compatible_with_connection(filter))
                .collect()
        } else {
            templates
        }
    }
}

impl CategoryTrait for GraphNodeTemplate {
    fn name(&self) -> String {
        "Nodes".to_owned()
    }
}

impl NodeTemplateTrait for GraphNodeTemplate {
    type NodeData = GraphNodeData;
    type DataType = GraphDataType;
    type ValueType = GraphValue;
    type UserState = GraphUiState;
    type CategoryType = &'static str;

    fn node_finder_label(&self, _user_state: &mut Self::UserState) -> Cow<'_, str> {
        Cow::Owned(template_label(self).to_owned())
    }

    fn node_finder_categories(&self, _user_state: &mut Self::UserState) -> Vec<Self::CategoryType> {
        vec![template_category(self)]
    }

    fn node_graph_label(&self, _user_state: &mut Self::UserState) -> String {
        self.kind().title().to_owned()
    }

    fn user_data(&self, user_state: &mut Self::UserState) -> Self::NodeData {
        GraphNodeData {
            shader_id: user_state.allocate_shader_node_id(),
            kind: self.kind(),
        }
    }

    fn build_node(
        &self,
        graph: &mut Graph<Self::NodeData, Self::DataType, Self::ValueType>,
        _user_state: &mut Self::UserState,
        node_id: NodeId,
    ) {
        add_ports_for_kind(graph, node_id, &self.kind());
    }
}

pub fn template_label(template: &GraphNodeTemplate) -> &'static str {
    match template {
        GraphNodeTemplate::F32Constant => "f32 Constant",
        GraphNodeTemplate::Vec2Constant => "vec2 Constant",
        GraphNodeTemplate::Vec3Constant => "vec3 Constant",
        GraphNodeTemplate::Vec4Constant => "vec4 Constant",
        GraphNodeTemplate::F32Uniform => "f32 Uniform",
        GraphNodeTemplate::Vec2Uniform => "vec2 Uniform",
        GraphNodeTemplate::Vec3Uniform => "vec3 Uniform",
        GraphNodeTemplate::Vec4Uniform => "vec4 Uniform",
        _ => template.kind().title(),
    }
}

fn template_category(template: &GraphNodeTemplate) -> &'static str {
    match template {
        GraphNodeTemplate::Uv | GraphNodeTemplate::Time => "01 Inputs",
        GraphNodeTemplate::F32Constant
        | GraphNodeTemplate::Vec2Constant
        | GraphNodeTemplate::Vec3Constant
        | GraphNodeTemplate::Vec4Constant => "02 Constants",
        GraphNodeTemplate::F32Uniform
        | GraphNodeTemplate::Vec2Uniform
        | GraphNodeTemplate::Vec3Uniform
        | GraphNodeTemplate::Vec4Uniform => "03 Uniforms",
        GraphNodeTemplate::Add
        | GraphNodeTemplate::Subtract
        | GraphNodeTemplate::Multiply
        | GraphNodeTemplate::Divide
        | GraphNodeTemplate::Sin
        | GraphNodeTemplate::Cos
        | GraphNodeTemplate::Abs
        | GraphNodeTemplate::Fract
        | GraphNodeTemplate::Normalize => "04 Math",
        GraphNodeTemplate::LygiaRandom
        | GraphNodeTemplate::LygiaValueNoise
        | GraphNodeTemplate::LygiaFbm
        | GraphNodeTemplate::LygiaVoronoi => "05 LYGIA Generative",
        GraphNodeTemplate::LygiaPow2
        | GraphNodeTemplate::LygiaPow3
        | GraphNodeTemplate::LygiaSaturate => "06 LYGIA Math",
        GraphNodeTemplate::LygiaCenter
        | GraphNodeTemplate::LygiaUncenter
        | GraphNodeTemplate::LygiaScale2d
        | GraphNodeTemplate::LygiaRotate2d => "07 LYGIA Space",
        GraphNodeTemplate::LygiaCircleSdf
        | GraphNodeTemplate::LygiaRectSdf
        | GraphNodeTemplate::LygiaBoxSdf => "08 LYGIA SDF",
        GraphNodeTemplate::LygiaLuma
        | GraphNodeTemplate::LygiaInvert
        | GraphNodeTemplate::LygiaBrightness
        | GraphNodeTemplate::LygiaContrast
        | GraphNodeTemplate::LygiaPosterize
        | GraphNodeTemplate::LygiaSaturation
        | GraphNodeTemplate::LygiaGammaCorrect
        | GraphNodeTemplate::LygiaBlendScreen
        | GraphNodeTemplate::LygiaBlendOverlay
        | GraphNodeTemplate::LygiaCosinePalette => "09 LYGIA Color",
        GraphNodeTemplate::TextureSample => "10 Textures",
    }
}

pub fn editor_from_shader_graph(graph: &ShaderGraph) -> (ShaderGraphEditorState, GraphUiState) {
    let mut editor = ShaderGraphEditorState::new(1.0);
    let ui_state = GraphUiState::from_graph(graph);
    let mut node_map = std::collections::HashMap::new();

    for node in &graph.nodes {
        let ui_node = editor.graph.add_node(
            node.name.clone(),
            GraphNodeData {
                shader_id: node.id,
                kind: node.kind.clone(),
            },
            |graph, node_id| add_ports_for_kind(graph, node_id, &node.kind),
        );
        editor.node_order.push(ui_node);
        editor
            .node_positions
            .insert(ui_node, egui::pos2(node.position[0], node.position[1]));
        editor
            .node_orientations
            .insert(ui_node, egui_graph_edit::NodeOrientation::LeftToRight);
        node_map.insert(node.id, ui_node);
    }

    for node in &graph.nodes {
        let Some(ui_target) = node_map.get(&node.id).copied() else {
            continue;
        };
        for (input_index, connection) in node.inputs.iter().enumerate() {
            let Some(connection) = connection else {
                continue;
            };
            let Some(ui_source) = node_map.get(&connection.node).copied() else {
                continue;
            };
            let Some((_, input_id)) = editor.graph.nodes[ui_target].inputs.get(input_index) else {
                continue;
            };
            let Some((_, output_id)) = editor.graph.nodes[ui_source].outputs.first() else {
                continue;
            };
            editor.graph.add_connection(*output_id, *input_id);
        }
    }

    (editor, ui_state)
}

pub fn add_template_to_editor(
    editor: &mut ShaderGraphEditorState,
    ui_state: &mut GraphUiState,
    template: GraphNodeTemplate,
    position: egui::Pos2,
) -> ShaderNodeId {
    let data = template.user_data(ui_state);
    let shader_id = data.shader_id;
    let label = template.node_graph_label(ui_state);
    let node_id = editor.graph.add_node(label, data, |graph, node_id| {
        template.build_node(graph, ui_state, node_id)
    });
    editor.node_order.push(node_id);
    editor.node_positions.insert(node_id, position);
    editor
        .node_orientations
        .insert(node_id, egui_graph_edit::NodeOrientation::LeftToRight);
    shader_id
}

pub fn add_kind_to_editor(
    editor: &mut ShaderGraphEditorState,
    ui_state: &mut GraphUiState,
    kind: NodeKind,
    label: String,
    position: egui::Pos2,
) -> (NodeId, ShaderNodeId) {
    let shader_id = ui_state.allocate_shader_node_id();
    let data = GraphNodeData {
        shader_id,
        kind: kind.clone(),
    };
    let node_id = editor.graph.add_node(label, data, |graph, node_id| {
        add_ports_for_kind(graph, node_id, &kind)
    });
    editor.node_order.push(node_id);
    editor.node_positions.insert(node_id, position);
    editor
        .node_orientations
        .insert(node_id, egui_graph_edit::NodeOrientation::LeftToRight);
    (node_id, shader_id)
}

pub fn shader_graph_from_editor(editor: &ShaderGraphEditorState) -> ShaderGraph {
    let mut nodes = Vec::new();
    for ui_node in editor.node_order.iter().copied() {
        let graph_node = &editor.graph.nodes[ui_node];
        let mut node = Node::new(
            graph_node.user_data.shader_id,
            graph_node.user_data.kind.clone(),
            editor
                .node_positions
                .get(ui_node)
                .map(|position| [position.x, position.y])
                .unwrap_or([0.0, 0.0]),
        );
        node.name.clone_from(&graph_node.label);
        for (input_index, (_, input_id)) in graph_node.inputs.iter().enumerate() {
            let Some(output_id) = editor.graph.connection(*input_id) else {
                continue;
            };
            let source_node = editor.graph.outputs[output_id].node;
            let source_shader_id = editor.graph.nodes[source_node].user_data.shader_id;
            if let Some(input) = node.inputs.get_mut(input_index) {
                *input = Some(Connection {
                    node: source_shader_id,
                });
            }
        }
        nodes.push(node);
    }

    ShaderGraph {
        format_version: GRAPH_FORMAT_VERSION,
        nodes,
    }
}

pub fn begin_connection_context(
    editor: &ShaderGraphEditorState,
    ui_state: &mut GraphUiState,
    origin: AnyParameterId,
) {
    if let Ok(data_type) = editor.graph.any_param_type(origin) {
        ui_state.connection_context = Some(ConnectionContext {
            origin,
            data_type: *data_type,
        });
    }
}

pub fn begin_disconnected_input_context(
    editor: &ShaderGraphEditorState,
    ui_state: &mut GraphUiState,
    output: egui_graph_edit::OutputId,
) {
    begin_connection_context(editor, ui_state, AnyParameterId::Output(output));
}

pub fn connect_new_node_to_context(
    editor: &mut ShaderGraphEditorState,
    ui_state: &mut GraphUiState,
    new_node: NodeId,
) -> bool {
    let Some(context) = ui_state.connection_context else {
        return false;
    };

    let connected = match context.origin {
        AnyParameterId::Output(output) => {
            let Some((_, input)) = editor.graph.nodes[new_node]
                .inputs
                .iter()
                .find(|(_, input)| {
                    editor.graph.connection(*input).is_none()
                        && editor
                            .graph
                            .any_param_type(AnyParameterId::Input(*input))
                            .is_ok_and(|input_type| *input_type == context.data_type)
                })
            else {
                return false;
            };
            editor.graph.add_connection(output, *input);
            true
        }
        AnyParameterId::Input(input) => {
            let Some((_, output)) =
                editor.graph.nodes[new_node]
                    .outputs
                    .iter()
                    .find(|(_, output)| {
                        editor
                            .graph
                            .any_param_type(AnyParameterId::Output(*output))
                            .is_ok_and(|output_type| *output_type == context.data_type)
                    })
            else {
                return false;
            };
            editor.graph.add_connection(*output, input);
            true
        }
    };

    if connected {
        ui_state.connection_context = None;
    }
    connected
}

fn add_ports_for_kind(
    graph: &mut Graph<GraphNodeData, GraphDataType, GraphValue>,
    node_id: NodeId,
    kind: &NodeKind,
) {
    for input_index in 0..kind.input_count() {
        graph.add_input_param(
            node_id,
            input_label(kind, input_index).to_owned(),
            input_type(kind, input_index),
            GraphValue::None,
            InputParamKind::ConnectionOnly,
            true,
        );
    }
    if let Some(output_type) = GraphDataType::output_for_kind(kind) {
        graph.add_output_param(node_id, output_label(kind).to_owned(), output_type);
    }
}

fn input_type(kind: &NodeKind, input_index: usize) -> GraphDataType {
    match (kind, input_index) {
        (NodeKind::Module(module), index) => module
            .inputs
            .get(index)
            .map(|input| GraphDataType::from_shader_type(input.shader_type))
            .unwrap_or(GraphDataType::Any),
        (NodeKind::FragmentOutput, 0) => GraphDataType::Vec4,
        (NodeKind::TextureSample, 0) => GraphDataType::Vec2,
        (NodeKind::LygiaValueNoise | NodeKind::LygiaFbm | NodeKind::LygiaVoronoi, 0) => {
            GraphDataType::Vec2
        }
        (NodeKind::LygiaVoronoi, 1) => GraphDataType::F32,
        (NodeKind::LygiaScale2d, 0) => GraphDataType::Vec2,
        (NodeKind::LygiaScale2d, 1) => GraphDataType::Vec2,
        (NodeKind::LygiaRotate2d, 0) => GraphDataType::Vec2,
        (NodeKind::LygiaRotate2d, 1) => GraphDataType::F32,
        (NodeKind::LygiaCircleSdf, 0) => GraphDataType::Vec2,
        (NodeKind::LygiaCircleSdf, 1) => GraphDataType::F32,
        (NodeKind::LygiaRectSdf, 0) => GraphDataType::Vec2,
        (NodeKind::LygiaRectSdf, 1) => GraphDataType::Vec2,
        (NodeKind::LygiaBoxSdf, 0) => GraphDataType::Vec3,
        (NodeKind::LygiaBoxSdf, 1) => GraphDataType::Vec3,
        (NodeKind::LygiaLuma, 0) => GraphDataType::Vec3,
        (
            NodeKind::LygiaBrightness
            | NodeKind::LygiaContrast
            | NodeKind::LygiaPosterize
            | NodeKind::LygiaSaturation
            | NodeKind::LygiaGammaCorrect,
            1,
        ) => GraphDataType::F32,
        (NodeKind::LygiaCosinePalette, 0) => GraphDataType::F32,
        _ => GraphDataType::Any,
    }
}

fn input_label(kind: &NodeKind, input_index: usize) -> String {
    if let NodeKind::Module(module) = kind {
        return module
            .inputs
            .get(input_index)
            .map(|input| input.name.clone())
            .unwrap_or_default();
    }

    match kind {
        NodeKind::Add | NodeKind::Subtract | NodeKind::Multiply | NodeKind::Divide => {
            if input_index == 0 { "A" } else { "B" }
        }
        NodeKind::LygiaRandom => "Seed",
        NodeKind::LygiaValueNoise => "Position",
        NodeKind::LygiaFbm => "Position",
        NodeKind::LygiaVoronoi => {
            if input_index == 0 {
                "Position"
            } else {
                "Time"
            }
        }
        NodeKind::LygiaPow2 | NodeKind::LygiaPow3 | NodeKind::LygiaSaturate => "Value",
        NodeKind::LygiaCenter | NodeKind::LygiaUncenter => "Coordinate",
        NodeKind::LygiaScale2d => {
            if input_index == 0 {
                "UV"
            } else {
                "Scale"
            }
        }
        NodeKind::LygiaRotate2d => {
            if input_index == 0 {
                "Vector"
            } else {
                "Angle"
            }
        }
        NodeKind::LygiaCircleSdf => {
            if input_index == 0 {
                "UV"
            } else {
                "Radius"
            }
        }
        NodeKind::LygiaRectSdf => {
            if input_index == 0 {
                "UV"
            } else {
                "Size"
            }
        }
        NodeKind::LygiaBoxSdf => {
            if input_index == 0 {
                "Position"
            } else {
                "Bounds"
            }
        }
        NodeKind::LygiaLuma => "Color",
        NodeKind::LygiaInvert => "Color",
        NodeKind::LygiaBrightness => {
            if input_index == 0 {
                "Color"
            } else {
                "Amount"
            }
        }
        NodeKind::LygiaContrast => {
            if input_index == 0 {
                "Color"
            } else {
                "Amount"
            }
        }
        NodeKind::LygiaPosterize => {
            if input_index == 0 {
                "Color"
            } else {
                "Steps"
            }
        }
        NodeKind::LygiaSaturation => {
            if input_index == 0 {
                "Color"
            } else {
                "Amount"
            }
        }
        NodeKind::LygiaGammaCorrect => {
            if input_index == 0 {
                "Color"
            } else {
                "Gamma"
            }
        }
        NodeKind::LygiaBlendScreen | NodeKind::LygiaBlendOverlay => {
            if input_index == 0 {
                "Base"
            } else {
                "Blend"
            }
        }
        NodeKind::LygiaCosinePalette => "T",
        NodeKind::FragmentOutput => "Color",
        NodeKind::TextureSample => "UV",
        NodeKind::Sin | NodeKind::Cos | NodeKind::Abs | NodeKind::Fract | NodeKind::Normalize => {
            "Value"
        }
        _ => "",
    }
    .to_owned()
}

fn output_label(kind: &NodeKind) -> String {
    if let NodeKind::Module(module) = kind {
        return module.output.name.clone();
    }

    match GraphDataType::output_for_kind(kind) {
        Some(GraphDataType::F32) => "f32",
        Some(GraphDataType::Vec2) => "vec2",
        Some(GraphDataType::Vec3) => "vec3",
        Some(GraphDataType::Vec4) => "vec4",
        Some(GraphDataType::Any) => "value",
        None => "",
    }
    .to_owned()
}
