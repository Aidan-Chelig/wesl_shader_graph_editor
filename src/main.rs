use std::collections::HashMap;

use bevy::prelude::*;
use bevy_egui::{
    EguiContexts, EguiPlugin, EguiPrimaryContextPass, EguiStartupSet, PrimaryEguiContext, egui,
};
use tree_sitter::{Node as SyntaxNode, Parser};
use wesl_shader_graph_editor::{
    compiler::{CompiledShader, compile_with_preview_node},
    graph::{Node, NodeId, NodeKind, ShaderGraph, ShaderType, Value},
};

mod preview;

use preview::{
    PreviewPlugin, PreviewPrimitive, PreviewSettings, PreviewShaderSource, PreviewUniformValues,
};

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.025, 0.03, 0.04)))
        .insert_resource(EditorState::default())
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "WESL Shader Graph Editor".to_owned(),
                resolution: (1440, 900).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin::default())
        .add_plugins(PreviewPlugin)
        .add_systems(
            PreStartup,
            setup_camera.before(EguiStartupSet::InitContexts),
        )
        .add_systems(EguiPrimaryContextPass, editor_ui)
        .run();
}

#[derive(Resource)]
struct EditorState {
    graph: ShaderGraph,
    compilation: Result<CompiledShader, String>,
    selected: Option<NodeId>,
    pending_connection: Option<NodeId>,
    preview_node: Option<NodeId>,
    next_node_id: u64,
    spawn_offset: f32,
    source_view: SourceView,
}

#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum SourceView {
    #[default]
    Wesl,
    Wgsl,
    BevyWesl,
}

impl Default for EditorState {
    fn default() -> Self {
        let graph = ShaderGraph::example();
        let compilation =
            compile_with_preview_node(&graph, None).map_err(|error| error.to_string());
        let next_node_id = graph.nodes.iter().map(|node| node.id.0).max().unwrap_or(0) + 1;
        Self {
            graph,
            compilation,
            selected: None,
            pending_connection: None,
            preview_node: None,
            next_node_id,
            spawn_offset: 0.0,
            source_view: SourceView::Wesl,
        }
    }
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        PrimaryEguiContext,
        Transform::from_xyz(6.5, 3.8, 8.4).looking_at(Vec3::new(0.0, -0.35, 0.0), Vec3::Y),
    ));
}

fn editor_ui(
    mut contexts: EguiContexts,
    mut state: ResMut<EditorState>,
    mut preview: ResMut<PreviewSettings>,
    mut preview_shader: ResMut<PreviewShaderSource>,
    mut preview_uniforms: ResMut<PreviewUniformValues>,
    mut highlighter: Local<SourceHighlighter>,
) -> Result {
    let context = contexts.ctx_mut()?;
    let mut recompile_requested = false;
    let mut viewport_ui = egui::Ui::new(
        context.clone(),
        "shader_editor_viewport".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(context.viewport_rect()),
    );

    egui::Panel::top("toolbar").show_inside(&mut viewport_ui, |ui| {
        ui.horizontal(|ui| {
            ui.heading("WESL Shader Graph");
            ui.separator();
            if ui.button("Recompile").clicked() {
                recompile_requested = true;
            }
            ui.separator();
            ui.menu_button("Add Node", |ui| {
                if add_node_button(ui, &mut state, NodeKind::Uv) {
                    ui.close();
                }
                if add_node_button(ui, &mut state, NodeKind::Time) {
                    ui.close();
                    recompile_requested = true;
                }
                ui.separator();
                if add_node_button(ui, &mut state, NodeKind::Constant(Value::F32(1.0))) {
                    ui.close();
                    recompile_requested = true;
                }
                if add_node_button(
                    ui,
                    &mut state,
                    NodeKind::Constant(Value::Vec4([1.0, 1.0, 1.0, 1.0])),
                ) {
                    ui.close();
                    recompile_requested = true;
                }
                if add_node_button(ui, &mut state, NodeKind::Uniform(Value::F32(1.0))) {
                    ui.close();
                    recompile_requested = true;
                }
                if add_node_button(
                    ui,
                    &mut state,
                    NodeKind::Uniform(Value::Vec4([1.0, 1.0, 1.0, 1.0])),
                ) {
                    ui.close();
                    recompile_requested = true;
                }
                ui.separator();
                if add_node_button(ui, &mut state, NodeKind::Add) {
                    ui.close();
                }
                if add_node_button(ui, &mut state, NodeKind::Subtract) {
                    ui.close();
                }
                if add_node_button(ui, &mut state, NodeKind::Multiply) {
                    ui.close();
                }
                if add_node_button(ui, &mut state, NodeKind::Divide) {
                    ui.close();
                }
                if add_node_button(ui, &mut state, NodeKind::Sin) {
                    ui.close();
                }
                if add_node_button(ui, &mut state, NodeKind::Cos) {
                    ui.close();
                }
                if add_node_button(ui, &mut state, NodeKind::Abs) {
                    ui.close();
                }
                if add_node_button(ui, &mut state, NodeKind::Fract) {
                    ui.close();
                }
                if add_node_button(ui, &mut state, NodeKind::Normalize) {
                    ui.close();
                }
                ui.separator();
                if add_node_button(ui, &mut state, NodeKind::TextureSample) {
                    ui.close();
                }
            });
            ui.separator();
            ui.label("Preview");
            egui::ComboBox::from_id_salt("preview_primitive")
                .selected_text(preview.primitive.label())
                .show_ui(ui, |ui| {
                    for primitive in PreviewPrimitive::ALL {
                        ui.selectable_value(&mut preview.primitive, primitive, primitive.label());
                    }
                });
            match &state.compilation {
                Ok(compiled) => {
                    ui.colored_label(
                        egui::Color32::from_rgb(92, 210, 130),
                        format!(
                            "Valid WESL · linked WGSL · {} emitted nodes",
                            compiled.emitted_nodes.len()
                        ),
                    );
                }
                Err(error) => {
                    ui.colored_label(egui::Color32::from_rgb(240, 95, 95), error);
                }
            }
        });
    });

    egui::Panel::right("source")
        .default_size(480.0)
        .resizable(true)
        .show_inside(&mut viewport_ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.source_view, SourceView::Wesl, "Generated WESL");
                ui.selectable_value(&mut state.source_view, SourceView::Wgsl, "Linked WGSL");
                ui.selectable_value(
                    &mut state.source_view,
                    SourceView::BevyWesl,
                    "Bevy Preview WESL",
                );
            });
            ui.separator();
            match &state.compilation {
                Ok(compiled) => {
                    let mut source = match state.source_view {
                        SourceView::Wesl => compiled.wesl.clone(),
                        SourceView::Wgsl => compiled.wgsl.clone(),
                        SourceView::BevyWesl => compiled.bevy_wesl.clone(),
                    };
                    let mut layouter =
                        |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
                            let mut layout_job = highlighter.highlight(text.as_str());
                            layout_job.wrap.max_width = wrap_width;
                            ui.fonts_mut(|fonts| fonts.layout_job(layout_job))
                        };
                    ui.add(
                        egui::TextEdit::multiline(&mut source)
                            .code_editor()
                            .layouter(&mut layouter)
                            .desired_width(f32::INFINITY)
                            .desired_rows(40)
                            .interactive(false),
                    );
                }
                Err(error) => {
                    ui.colored_label(egui::Color32::from_rgb(240, 95, 95), error);
                }
            }
        });

    let mut graph_edit = GraphEdit::default();
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show_inside(&mut viewport_ui, |ui| {
            graph_edit = draw_graph(ui, &mut state);
        });

    if recompile_requested || graph_edit.source_changed || graph_edit.preview_changed {
        recompile_graph(&mut state, &mut preview_shader, &mut preview_uniforms);
    } else if graph_edit.uniform_values_changed {
        sync_preview_uniforms(&state, &mut preview_uniforms);
    }

    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HighlightKind {
    Keyword,
    Type,
    Function,
    Number,
    String,
    Comment,
    Attribute,
    Punctuation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HighlightSpan {
    start: usize,
    end: usize,
    kind: HighlightKind,
}

struct SourceHighlighter {
    parser: Parser,
    cached_source: String,
    cached_job: egui::text::LayoutJob,
}

impl Default for SourceHighlighter {
    fn default() -> Self {
        let mut parser = Parser::new();
        parser
            .set_language(tree_sitter_wgsl::language())
            .expect("tree-sitter WGSL language must load");
        Self {
            parser,
            cached_source: String::new(),
            cached_job: egui::text::LayoutJob::default(),
        }
    }
}

impl SourceHighlighter {
    fn highlight(&mut self, source: &str) -> egui::text::LayoutJob {
        if self.cached_source != source {
            self.cached_source.clear();
            self.cached_source.push_str(source);
            self.cached_job = self.build_layout_job(source);
        }
        self.cached_job.clone()
    }

    fn build_layout_job(&mut self, source: &str) -> egui::text::LayoutJob {
        let mut spans = Vec::new();
        if let Some(tree) = self.parser.parse(source, None) {
            collect_highlight_spans(tree.root_node(), source.as_bytes(), &mut spans);
        }
        spans.sort_by_key(|span| (span.start, span.end));
        spans.dedup_by_key(|span| (span.start, span.end));

        let mut job = egui::text::LayoutJob::default();
        let mut cursor = 0;
        for span in spans {
            if span.start < cursor || span.end > source.len() || span.start >= span.end {
                continue;
            }
            if cursor < span.start {
                append_source_text(&mut job, &source[cursor..span.start], None);
            }
            append_source_text(&mut job, &source[span.start..span.end], Some(span.kind));
            cursor = span.end;
        }
        if cursor < source.len() {
            append_source_text(&mut job, &source[cursor..], None);
        }
        job
    }
}

fn collect_highlight_spans(node: SyntaxNode, source: &[u8], spans: &mut Vec<HighlightSpan>) {
    if let Some(kind) = highlight_kind(node, source) {
        spans.push(HighlightSpan {
            start: node.start_byte(),
            end: node.end_byte(),
            kind,
        });
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_highlight_spans(child, source, spans);
    }
}

fn highlight_kind(node: SyntaxNode, source: &[u8]) -> Option<HighlightKind> {
    let kind = node.kind();
    if kind.contains("comment") {
        return Some(HighlightKind::Comment);
    }
    if kind.contains("attribute")
        || token_text(node, source).is_some_and(|text| text.starts_with('@'))
    {
        return Some(HighlightKind::Attribute);
    }
    if kind.contains("string") {
        return Some(HighlightKind::String);
    }
    if kind.contains("number")
        || kind.contains("float")
        || kind.contains("int")
        || token_text(node, source).is_some_and(is_number_token)
    {
        return Some(HighlightKind::Number);
    }

    let text = token_text(node, source)?;
    if is_keyword(text) {
        Some(HighlightKind::Keyword)
    } else if is_type_name(text) {
        Some(HighlightKind::Type)
    } else if is_builtin_function(text) {
        Some(HighlightKind::Function)
    } else if is_punctuation(text) {
        Some(HighlightKind::Punctuation)
    } else {
        None
    }
}

fn token_text<'a>(node: SyntaxNode, source: &'a [u8]) -> Option<&'a str> {
    if node.child_count() != 0 {
        return None;
    }
    std::str::from_utf8(&source[node.start_byte()..node.end_byte()]).ok()
}

fn append_source_text(
    job: &mut egui::text::LayoutJob,
    text: &str,
    highlight: Option<HighlightKind>,
) {
    job.append(text, 0.0, text_format(highlight));
}

fn text_format(highlight: Option<HighlightKind>) -> egui::TextFormat {
    let color = match highlight {
        Some(HighlightKind::Keyword) => egui::Color32::from_rgb(197, 134, 255),
        Some(HighlightKind::Type) => egui::Color32::from_rgb(92, 210, 210),
        Some(HighlightKind::Function) => egui::Color32::from_rgb(220, 205, 135),
        Some(HighlightKind::Number) => egui::Color32::from_rgb(245, 170, 105),
        Some(HighlightKind::String) => egui::Color32::from_rgb(165, 214, 167),
        Some(HighlightKind::Comment) => egui::Color32::from_rgb(115, 130, 145),
        Some(HighlightKind::Attribute) => egui::Color32::from_rgb(125, 175, 255),
        Some(HighlightKind::Punctuation) => egui::Color32::from_rgb(190, 200, 215),
        None => egui::Color32::from_rgb(218, 224, 235),
    };
    egui::TextFormat {
        font_id: egui::FontId::monospace(13.0),
        color,
        ..default()
    }
}

fn is_keyword(text: &str) -> bool {
    matches!(
        text,
        "alias"
            | "break"
            | "case"
            | "const"
            | "const_assert"
            | "continue"
            | "continuing"
            | "default"
            | "diagnostic"
            | "discard"
            | "else"
            | "enable"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "let"
            | "loop"
            | "override"
            | "requires"
            | "return"
            | "struct"
            | "switch"
            | "true"
            | "var"
            | "while"
    )
}

fn is_type_name(text: &str) -> bool {
    matches!(
        text,
        "array"
            | "atomic"
            | "bool"
            | "f16"
            | "f32"
            | "i32"
            | "mat2x2"
            | "mat2x3"
            | "mat2x4"
            | "mat3x2"
            | "mat3x3"
            | "mat3x4"
            | "mat4x2"
            | "mat4x3"
            | "mat4x4"
            | "ptr"
            | "sampler"
            | "sampler_comparison"
            | "texture_2d"
            | "texture_cube"
            | "u32"
            | "vec2"
            | "vec3"
            | "vec4"
    )
}

fn is_builtin_function(text: &str) -> bool {
    matches!(
        text,
        "abs"
            | "acos"
            | "asin"
            | "atan"
            | "ceil"
            | "clamp"
            | "cos"
            | "cross"
            | "distance"
            | "dot"
            | "exp"
            | "floor"
            | "fract"
            | "length"
            | "log"
            | "max"
            | "min"
            | "mix"
            | "normalize"
            | "pow"
            | "reflect"
            | "sin"
            | "smoothstep"
            | "sqrt"
            | "step"
            | "tan"
    )
}

fn is_punctuation(text: &str) -> bool {
    matches!(
        text,
        "{" | "}"
            | "("
            | ")"
            | "["
            | "]"
            | "<"
            | ">"
            | ":"
            | ";"
            | ","
            | "."
            | "="
            | "+"
            | "-"
            | "*"
            | "/"
    )
}

fn is_number_token(text: &str) -> bool {
    let mut chars = text.chars();
    matches!(chars.next(), Some(char) if char.is_ascii_digit())
}

fn add_node_button(ui: &mut egui::Ui, state: &mut EditorState, kind: NodeKind) -> bool {
    let label = match &kind {
        NodeKind::Constant(Value::F32(_)) => "Float Constant",
        NodeKind::Constant(Value::Vec4(_)) => "Color Constant",
        NodeKind::Uniform(Value::F32(_)) => "Float Uniform",
        NodeKind::Uniform(Value::Vec4(_)) => "Color Uniform",
        _ => kind.title(),
    };

    let clicked = ui.button(label).clicked();
    if clicked {
        spawn_node(state, kind);
    }
    clicked
}

fn spawn_node(state: &mut EditorState, kind: NodeKind) {
    let id = NodeId(state.next_node_id);
    state.next_node_id += 1;
    let position = [90.0 + state.spawn_offset, 430.0 + state.spawn_offset * 0.35];
    state.spawn_offset = (state.spawn_offset + 32.0) % 260.0;
    state.graph.nodes.push(Node::new(id, kind, position));
    state.selected = Some(id);
}

#[derive(Clone, Copy, Debug, Default)]
struct GraphEdit {
    source_changed: bool,
    preview_changed: bool,
    uniform_values_changed: bool,
}

fn draw_graph(ui: &mut egui::Ui, state: &mut EditorState) -> GraphEdit {
    let canvas = ui.max_rect();
    let painter = ui.painter_at(canvas);
    painter.rect_filled(
        canvas,
        0.0,
        egui::Color32::from_rgba_unmultiplied(8, 11, 18, 38),
    );
    draw_grid(&painter, canvas);

    let node_size = egui::vec2(210.0, 124.0);
    let origin = canvas.min + egui::vec2(24.0, 24.0);
    let node_types = match &state.compilation {
        Ok(compiled) => compiled.node_types.clone(),
        Err(_) => HashMap::new(),
    };

    for target in &state.graph.nodes {
        for (input_index, connection) in target.inputs.iter().enumerate() {
            let Some(connection) = connection else {
                continue;
            };
            let Some(source) = state.graph.node(connection.node) else {
                continue;
            };
            let source_rect = node_rect(origin, node_size, source.position);
            let target_rect = node_rect(origin, node_size, target.position);
            let from = output_socket_position(source_rect);
            let to = input_socket_position(target_rect, input_index);
            let connection_type = output_type(source, &node_types);
            painter.line_segment(
                [from, to],
                egui::Stroke::new(3.0, socket_color(connection_type)),
            );
        }
    }
    if let Some(source_id) = state.pending_connection
        && let Some(source) = state.graph.node(source_id)
        && let Some(pointer) = ui.input(|input| input.pointer.hover_pos())
    {
        let source_rect = node_rect(origin, node_size, source.position);
        painter.line_segment(
            [output_socket_position(source_rect), pointer],
            egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 196, 92)),
        );
    }

    let mut graph_edit = GraphEdit::default();
    let pointer_released = ui.input(|input| input.pointer.any_released());
    let pointer_position = ui.input(|input| input.pointer.hover_pos());
    for node in &mut state.graph.nodes {
        let rect = node_rect(origin, node_size, node.position);
        let header = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), 34.0));
        let response = ui.interact(
            header,
            egui::Id::new(("shader_node_header", node.id.0)),
            egui::Sense::click_and_drag(),
        );
        if response.dragged() {
            let delta = response.drag_delta();
            node.position[0] += delta.x;
            node.position[1] += delta.y;
        }
        if response.clicked() {
            state.selected = Some(node.id);
        }

        let selected = state.selected == Some(node.id);
        let border = if selected {
            egui::Color32::from_rgb(255, 190, 80)
        } else {
            socket_color(output_type(node, &node_types))
        };
        painter.rect_filled(rect, 8.0, egui::Color32::from_rgb(39, 45, 58));
        painter.rect_stroke(
            rect,
            8.0,
            egui::Stroke::new(if selected { 2.0 } else { 1.0 }, border),
            egui::StrokeKind::Inside,
        );
        painter.rect_filled(
            header,
            egui::CornerRadius {
                nw: 8,
                ne: 8,
                sw: 0,
                se: 0,
            },
            node_header_color(output_type(node, &node_types)),
        );
        painter.text(
            header.left_center() + egui::vec2(12.0, 0.0),
            egui::Align2::LEFT_CENTER,
            &node.name,
            egui::FontId::proportional(16.0),
            egui::Color32::WHITE,
        );
        let node_output_type = output_type(node, &node_types);
        if node_output_type == Some(ShaderType::Vec4) {
            let preview_rect = egui::Rect::from_min_size(
                header.right_center() - egui::vec2(86.0, 10.0),
                egui::vec2(20.0, 20.0),
            );
            let preview_response = ui.interact(
                preview_rect,
                egui::Id::new(("shader_node_preview", node.id.0)),
                egui::Sense::click(),
            );
            if preview_response.clicked() {
                state.preview_node = if state.preview_node == Some(node.id) {
                    None
                } else {
                    Some(node.id)
                };
                graph_edit.preview_changed = true;
            }

            let active = state.preview_node == Some(node.id);
            let preview_color = if active {
                egui::Color32::from_rgb(255, 215, 110)
            } else if preview_response.hovered() {
                egui::Color32::from_rgb(255, 235, 165)
            } else {
                egui::Color32::from_rgb(150, 160, 175)
            };
            painter.circle_stroke(
                preview_rect.center(),
                8.0,
                egui::Stroke::new(1.5, preview_color),
            );
            if active {
                painter.circle_filled(preview_rect.center(), 4.0, preview_color);
            }
            preview_response.on_hover_text("Preview this vec4 node on the 3D object");
        }
        if let Some(shader_type) = node_output_type {
            let badge_rect = egui::Rect::from_min_size(
                header.right_center() - egui::vec2(58.0, 10.0),
                egui::vec2(44.0, 20.0),
            );
            painter.rect_filled(
                badge_rect,
                10.0,
                socket_color(Some(shader_type)).gamma_multiply(0.22),
            );
            painter.rect_stroke(
                badge_rect,
                10.0,
                egui::Stroke::new(1.0, socket_color(Some(shader_type))),
                egui::StrokeKind::Inside,
            );
            painter.text(
                badge_rect.center(),
                egui::Align2::CENTER_CENTER,
                shader_type_label(shader_type),
                egui::FontId::monospace(11.0),
                egui::Color32::WHITE,
            );
        }
        let content_rect = egui::Rect::from_min_max(
            rect.left_top() + egui::vec2(12.0, 43.0),
            rect.right_bottom() - egui::vec2(12.0, 10.0),
        );
        ui.scope_builder(
            egui::UiBuilder::new()
                .id_salt(("shader_node_content", node.id.0))
                .max_rect(content_rect),
            |ui| match &mut node.kind {
                NodeKind::Constant(value) => {
                    graph_edit.source_changed |= edit_constant(ui, value);
                }
                NodeKind::Uniform(value) => {
                    graph_edit.uniform_values_changed |= edit_uniform(ui, value);
                }
                kind => {
                    ui.label(
                        egui::RichText::new(kind.title())
                            .monospace()
                            .color(egui::Color32::from_gray(190)),
                    );
                }
            },
        );
        let output_position = output_socket_position(rect);
        let output_response = ui.interact(
            socket_rect(output_position),
            egui::Id::new(("shader_node_output", node.id.0)),
            egui::Sense::click_and_drag(),
        );
        if output_response.is_pointer_button_down_on() || output_response.drag_started() {
            state.pending_connection = Some(node.id);
        }
        let output_color = if state.pending_connection == Some(node.id) {
            egui::Color32::from_rgb(255, 196, 92)
        } else if output_response.hovered() {
            egui::Color32::from_rgb(145, 205, 255)
        } else {
            socket_color(node_output_type)
        };
        painter.circle_filled(output_position, 6.0, output_color);
        if let Some(shader_type) = node_output_type {
            painter.text(
                output_position + egui::vec2(-10.0, 14.0),
                egui::Align2::RIGHT_CENTER,
                shader_type_label(shader_type),
                egui::FontId::monospace(10.0),
                socket_color(Some(shader_type)),
            );
        }
        for input_index in 0..node.inputs.len() {
            let input_position = input_socket_position(rect, input_index);
            let input_response = ui.interact(
                socket_rect(input_position),
                egui::Id::new(("shader_node_input", node.id.0, input_index)),
                egui::Sense::click(),
            );
            if input_response.secondary_clicked()
                && let Some(input) = node.inputs.get_mut(input_index)
            {
                *input = None;
                state.pending_connection = None;
                graph_edit.source_changed = true;
            }
            let input_type = input_socket_type(node, input_index, &node_types);
            let input_color = if input_response.hovered() && state.pending_connection.is_some() {
                egui::Color32::from_rgb(255, 196, 92)
            } else if input_response.hovered() {
                egui::Color32::from_rgb(145, 205, 255)
            } else {
                socket_color(input_type)
            };
            painter.circle_filled(input_position, 6.0, input_color);
            painter.text(
                input_position + egui::vec2(10.0, 0.0),
                egui::Align2::LEFT_CENTER,
                input_socket_label(&node.kind, input_index),
                egui::FontId::monospace(10.0),
                input_color,
            );
        }
    }

    if pointer_released {
        if let (Some(source_id), Some(pointer_position)) =
            (state.pending_connection, pointer_position)
            && let Some((target_id, input_index)) =
                hovered_input_socket(&state.graph, origin, node_size, pointer_position)
            && source_id != target_id
            && let Some(target) = state
                .graph
                .nodes
                .iter_mut()
                .find(|node| node.id == target_id)
        {
            target.connect_input(input_index, source_id);
            graph_edit.source_changed = true;
        }
        state.pending_connection = None;
    }

    graph_edit
}

fn hovered_input_socket(
    graph: &ShaderGraph,
    origin: egui::Pos2,
    node_size: egui::Vec2,
    pointer_position: egui::Pos2,
) -> Option<(NodeId, usize)> {
    graph.nodes.iter().find_map(|node| {
        let rect = node_rect(origin, node_size, node.position);
        (0..node.inputs.len()).find_map(|input_index| {
            let input_position = input_socket_position(rect, input_index);
            socket_rect(input_position)
                .contains(pointer_position)
                .then_some((node.id, input_index))
        })
    })
}

fn output_type(node: &Node, node_types: &HashMap<NodeId, ShaderType>) -> Option<ShaderType> {
    node_types
        .get(&node.id)
        .copied()
        .or_else(|| static_output_type(&node.kind))
}

fn static_output_type(kind: &NodeKind) -> Option<ShaderType> {
    match kind {
        NodeKind::Constant(value) | NodeKind::Uniform(value) => Some(value.shader_type()),
        NodeKind::Uv => Some(ShaderType::Vec2),
        NodeKind::Time => Some(ShaderType::F32),
        NodeKind::TextureSample => Some(ShaderType::Vec4),
        NodeKind::FragmentOutput => None,
        NodeKind::Add
        | NodeKind::Subtract
        | NodeKind::Multiply
        | NodeKind::Divide
        | NodeKind::Sin
        | NodeKind::Cos
        | NodeKind::Abs
        | NodeKind::Fract
        | NodeKind::Normalize => None,
    }
}

fn input_socket_type(
    node: &Node,
    input_index: usize,
    node_types: &HashMap<NodeId, ShaderType>,
) -> Option<ShaderType> {
    node.inputs
        .get(input_index)
        .and_then(|connection| connection.as_ref())
        .and_then(|connection| node_types.get(&connection.node).copied())
        .or_else(|| static_input_type(&node.kind, input_index))
}

fn static_input_type(kind: &NodeKind, input_index: usize) -> Option<ShaderType> {
    match (kind, input_index) {
        (NodeKind::FragmentOutput, 0) => Some(ShaderType::Vec4),
        (NodeKind::TextureSample, 0) => Some(ShaderType::Vec2),
        _ => None,
    }
}

fn input_socket_label(kind: &NodeKind, input_index: usize) -> &'static str {
    match kind {
        NodeKind::Add | NodeKind::Subtract | NodeKind::Multiply | NodeKind::Divide => {
            if input_index == 0 { "A" } else { "B" }
        }
        NodeKind::FragmentOutput => "Color",
        NodeKind::Sin | NodeKind::Cos | NodeKind::Abs | NodeKind::Fract | NodeKind::Normalize => {
            "Value"
        }
        NodeKind::TextureSample => "UV",
        _ => "",
    }
}

fn socket_color(shader_type: Option<ShaderType>) -> egui::Color32 {
    match shader_type {
        Some(ShaderType::F32) => egui::Color32::from_rgb(245, 190, 90),
        Some(ShaderType::Vec2) => egui::Color32::from_rgb(95, 210, 170),
        Some(ShaderType::Vec3) => egui::Color32::from_rgb(95, 170, 255),
        Some(ShaderType::Vec4) => egui::Color32::from_rgb(220, 115, 255),
        None => egui::Color32::from_rgb(120, 135, 155),
    }
}

fn node_header_color(shader_type: Option<ShaderType>) -> egui::Color32 {
    let color = socket_color(shader_type);
    egui::Color32::from_rgb(
        (color.r() / 3).saturating_add(28),
        (color.g() / 3).saturating_add(34),
        (color.b() / 3).saturating_add(48),
    )
}

fn shader_type_label(shader_type: ShaderType) -> &'static str {
    match shader_type {
        ShaderType::F32 => "f32",
        ShaderType::Vec2 => "vec2",
        ShaderType::Vec3 => "vec3",
        ShaderType::Vec4 => "vec4",
    }
}

fn edit_uniform(ui: &mut egui::Ui, value: &mut Value) -> bool {
    ui.label("Preview value");
    edit_constant(ui, value)
}

fn edit_constant(ui: &mut egui::Ui, value: &mut Value) -> bool {
    match value {
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
        Value::Vec2(values) => edit_vector(ui, values, &["X", "Y"]),
        Value::Vec3(values) => edit_vector(ui, values, &["X", "Y", "Z"]),
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
    }
}

fn edit_vector<const N: usize>(
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

fn recompile_graph(
    state: &mut EditorState,
    preview_shader: &mut PreviewShaderSource,
    preview_uniforms: &mut PreviewUniformValues,
) {
    state.compilation = compile_with_preview_node(&state.graph, state.preview_node)
        .map_err(|error| error.to_string());
    if let Ok(compiled) = &state.compilation {
        if preview_shader.wesl != compiled.bevy_wesl {
            preview_shader.wesl.clone_from(&compiled.bevy_wesl);
        }
        update_preview_uniforms(&state.graph, compiled, preview_uniforms);
    }
}

fn sync_preview_uniforms(state: &EditorState, preview_uniforms: &mut PreviewUniformValues) {
    if let Ok(compiled) = &state.compilation {
        update_preview_uniforms(&state.graph, compiled, preview_uniforms);
    }
}

fn update_preview_uniforms(
    graph: &ShaderGraph,
    compiled: &CompiledShader,
    preview_uniforms: &mut PreviewUniformValues,
) {
    let mut values = vec![[0.0; 4]; compiled.uniforms.len().max(1)];
    for uniform in &compiled.uniforms {
        let Some(node) = graph.node(uniform.node) else {
            continue;
        };
        let NodeKind::Uniform(value) = &node.kind else {
            continue;
        };
        if let Some(user_uniform) = values.get_mut(uniform.index) {
            *user_uniform = value_to_array(value);
        }
    }
    if preview_uniforms.values != values {
        preview_uniforms.values = values;
    }
}

fn value_to_array(value: &Value) -> [f32; 4] {
    match value {
        Value::F32(value) => [*value, 0.0, 0.0, 0.0],
        Value::Vec2(values) => [values[0], values[1], 0.0, 0.0],
        Value::Vec3(values) => [values[0], values[1], values[2], 0.0],
        Value::Vec4(values) => *values,
    }
}

fn node_rect(origin: egui::Pos2, size: egui::Vec2, position: [f32; 2]) -> egui::Rect {
    egui::Rect::from_min_size(origin + egui::vec2(position[0], position[1]), size)
}

fn output_socket_position(rect: egui::Rect) -> egui::Pos2 {
    rect.right_center()
}

fn input_socket_position(rect: egui::Rect, input_index: usize) -> egui::Pos2 {
    egui::pos2(rect.left(), rect.top() + 52.0 + input_index as f32 * 22.0)
}

fn socket_rect(position: egui::Pos2) -> egui::Rect {
    egui::Rect::from_center_size(position, egui::Vec2::splat(18.0))
}

fn draw_grid(painter: &egui::Painter, rect: egui::Rect) {
    let spacing = 24.0;
    let color = egui::Color32::from_rgba_unmultiplied(185, 205, 235, 24);
    let mut x = rect.left();
    while x < rect.right() {
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0, color),
        );
        x += spacing;
    }
    let mut y = rect.top();
    while y < rect.bottom() {
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(1.0, color),
        );
        y += spacing;
    }
}
