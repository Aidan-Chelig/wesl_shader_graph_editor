use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use bevy_egui::{EguiContexts, EguiPlugin, EguiStartupSet, PrimaryEguiContext, egui};
use egui_graph_edit::{InputId as UiInputId, NodeId as UiNodeId, NodeResponse};
use serde::{Deserialize, Serialize};
use tree_sitter::{Node as SyntaxNode, Parser};
use wesl_shader_graph_editor::{
    compiler::{CompiledShader, compile_preview_graph, compile_with_preview_node},
    graph::{ModuleDefinition, ModulePort, NodeId, NodeKind, ShaderGraph, ShaderType, Value},
};

mod graph_editor;
mod preview;

use image::GenericImageView;

use graph_editor::{
    GraphNodeTemplate, GraphNodeTemplates, GraphResponse as ShaderGraphResponse, GraphUiState,
    GraphValueChangeKind, ShaderGraphEditorState, add_kind_to_editor, add_template_to_editor,
    begin_connection_context, begin_disconnected_input_context, connect_new_node_to_context,
    editor_from_shader_graph, resolve_ambiguous_output_types,
    resolve_ambiguous_output_types_checked, shader_graph_from_editor, template_label,
};
use preview::{
    PreviewPlugin, PreviewPrimitive, PreviewSettings, PreviewShaderSource, PreviewTexture,
    PreviewUniformValues,
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
        .add_plugins(EguiPlugin {
            #[allow(deprecated)]
            enable_multipass_for_primary_context: false,
            ..default()
        })
        .add_plugins(PreviewPlugin)
        .add_systems(
            PreStartup,
            setup_camera.before(EguiStartupSet::InitContexts),
        )
        .add_systems(Update, editor_ui)
        .run();
}

#[derive(Resource)]
struct EditorState {
    graph: ShaderGraph,
    graph_editor: ShaderGraphEditorState,
    graph_ui: GraphUiState,
    module_tabs: Vec<ModuleEditorTab>,
    active_tab: EditorTab,
    compilation: Result<CompiledShader, String>,
    selected: Option<NodeId>,
    hovered: Option<NodeId>,
    preview_node: Option<NodeId>,
    spawn_offset: f32,
    source_view: SourceView,
    texture_path: Option<String>,
    pending_texture_path: Option<String>,
    export_status: Option<String>,
    node_context_menu: Option<NodeContextMenu>,
    module_prompt: Option<ModulePrompt>,
    modules: Vec<ModuleDefinition>,
    global_modules: Vec<ModuleDefinition>,
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
        let (graph_editor, graph_ui) = editor_from_shader_graph(&graph);
        let compilation =
            compile_with_preview_node(&graph, None).map_err(|error| error.to_string());
        Self {
            graph,
            graph_editor,
            graph_ui,
            module_tabs: Vec::new(),
            active_tab: EditorTab::Main,
            compilation,
            selected: None,
            hovered: None,
            preview_node: None,
            spawn_offset: 0.0,
            source_view: SourceView::Wesl,
            texture_path: None,
            pending_texture_path: None,
            export_status: None,
            node_context_menu: None,
            module_prompt: None,
            modules: Vec::new(),
            global_modules: load_user_modules().unwrap_or_default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EditorTab {
    Main,
    Module(u64),
}

#[derive(Clone)]
struct ModuleEditorTab {
    module_id: u64,
    title: String,
    module: ModuleDefinition,
    graph: ShaderGraph,
    graph_editor: ShaderGraphEditorState,
    graph_ui: GraphUiState,
    preview_node: Option<NodeId>,
}

#[derive(Clone, Debug)]
struct NodeContextMenu {
    node: UiNodeId,
    position: egui::Pos2,
}

#[derive(Clone, Debug)]
struct ModulePrompt {
    root: UiNodeId,
    position: egui::Pos2,
    module_name: String,
    save_to_user_library: bool,
}

#[derive(Deserialize, Serialize)]
struct ProjectFile {
    graph: ShaderGraph,
    preview_node: Option<NodeId>,
    texture_path: Option<String>,
    modules: Vec<ModuleDefinition>,
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        PrimaryEguiContext,
        Transform::from_xyz(6.5, 3.8, 8.4).looking_at(Vec3::new(0.0, -0.35, 0.0), Vec3::Y),
    ));
}

fn draw_editor_tabs(ui: &mut egui::Ui, state: &mut EditorState) {
    ui.horizontal(|ui| {
        ui.selectable_value(&mut state.active_tab, EditorTab::Main, "Main Graph");
        let mut close_module = None;
        for tab in &state.module_tabs {
            ui.selectable_value(
                &mut state.active_tab,
                EditorTab::Module(tab.module_id),
                &tab.title,
            );
            if ui
                .small_button("x")
                .on_hover_text("Close module tab")
                .clicked()
            {
                close_module = Some(tab.module_id);
            }
        }
        if let Some(module_id) = close_module {
            state.module_tabs.retain(|tab| tab.module_id != module_id);
            if state.active_tab == EditorTab::Module(module_id) {
                state.active_tab = EditorTab::Main;
            }
        }
    });
}

fn open_module_editor_tab(state: &mut EditorState, module: &ModuleDefinition) {
    if state
        .module_tabs
        .iter()
        .any(|tab| tab.module_id == module.id)
    {
        state.active_tab = EditorTab::Module(module.id);
        return;
    }

    let graph = (*module.graph).clone();
    let mut module = module.clone();
    module.root = module_root_from_shader_graph(&graph, module.root);
    module.output.node = Some(module.root);
    let (graph_editor, graph_ui) = editor_from_shader_graph(&graph);
    let preview_node = Some(module.root);
    let module_id = module.id;
    state.module_tabs.push(ModuleEditorTab {
        module_id,
        title: module.name.clone(),
        module,
        graph,
        graph_editor,
        graph_ui,
        preview_node,
    });
    sync_module_tab(state, module_id, true);
    state.active_tab = EditorTab::Module(module_id);
}

fn sync_module_tab(state: &mut EditorState, module_id: u64, update_signature: bool) {
    let Some(tab_index) = state
        .module_tabs
        .iter()
        .position(|tab| tab.module_id == module_id)
    else {
        return;
    };

    let mut module = state.module_tabs[tab_index].module.clone();
    let graph = shader_graph_from_editor(&state.module_tabs[tab_index].graph_editor);
    if update_signature {
        module.root = module_root_from_shader_graph(&graph, module.root);
        module.output.node = Some(module.root);
        if let Ok(compiled) = compile_preview_graph(&graph, module.root)
            && let Some(shader_type) = compiled.node_types.get(&module.root).copied()
        {
            module.output.shader_type = shader_type;
        }
    }
    module.graph = Box::new(graph.clone());
    state.module_tabs[tab_index].graph = graph;
    state.module_tabs[tab_index].module = module.clone();

    if let Some(existing) = state
        .modules
        .iter_mut()
        .find(|existing| existing.id == module.id)
    {
        *existing = module.clone();
    }

    for (_, ui_node) in &mut state.graph_editor.graph.nodes {
        if let NodeKind::Module(existing) = &mut ui_node.user_data.kind
            && existing.id == module.id
        {
            *existing = Box::new(module.clone());
        }
    }
    state.graph = shader_graph_from_editor(&state.graph_editor);
}

fn active_graph_and_preview(state: &EditorState) -> (&ShaderGraph, Option<NodeId>) {
    match state.active_tab {
        EditorTab::Main => (&state.graph, state.preview_node),
        EditorTab::Module(module_id) => state
            .module_tabs
            .iter()
            .find(|tab| tab.module_id == module_id)
            .map(|tab| (&tab.graph, tab.preview_node))
            .unwrap_or((&state.graph, state.preview_node)),
    }
}

fn active_graph(state: &EditorState) -> &ShaderGraph {
    active_graph_and_preview(state).0
}

fn editor_ui(
    mut contexts: EguiContexts,
    mut state: ResMut<EditorState>,
    mut preview: ResMut<PreviewSettings>,
    mut preview_shader: ResMut<PreviewShaderSource>,
    mut preview_uniforms: ResMut<PreviewUniformValues>,
    mut images: ResMut<Assets<Image>>,
    mut preview_texture: ResMut<PreviewTexture>,
    mut highlighter: Local<SourceHighlighter>,
) -> Result {
    let context = contexts.ctx_mut()?;
    let state = state.as_mut();
    let mut recompile_requested = false;
    let active_tab_before_ui = state.active_tab;
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
            draw_file_menu(ui, state);
            ui.separator();
            ui.menu_button("Add Node", |ui| {
                for template in GraphNodeTemplate::TOOLBAR {
                    if ui.button(template_label(&template)).clicked() {
                        let position = egui::pos2(
                            90.0 + state.spawn_offset,
                            430.0 + state.spawn_offset * 0.35,
                        );
                        state.spawn_offset = (state.spawn_offset + 32.0) % 260.0;
                        match state.active_tab {
                            EditorTab::Main => {
                                add_template_to_editor(
                                    &mut state.graph_editor,
                                    &mut state.graph_ui,
                                    template,
                                    position,
                                );
                                state.graph = shader_graph_from_editor(&state.graph_editor);
                            }
                            EditorTab::Module(module_id) => {
                                let mut added = false;
                                if let Some(tab) = state
                                    .module_tabs
                                    .iter_mut()
                                    .find(|tab| tab.module_id == module_id)
                                {
                                    add_template_to_editor(
                                        &mut tab.graph_editor,
                                        &mut tab.graph_ui,
                                        template,
                                        position,
                                    );
                                    added = true;
                                }
                                if added {
                                    sync_module_tab(state, module_id, true);
                                }
                            }
                        }
                        recompile_requested = true;
                        ui.close();
                    }
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
            if let Some(status) = &state.export_status {
                ui.separator();
                ui.label(status);
            }
        });
    });

    if !state.module_tabs.is_empty() {
        egui::Panel::top("tabs").show_inside(&mut viewport_ui, |ui| {
            draw_editor_tabs(ui, state);
        });
    }
    if state.active_tab != active_tab_before_ui {
        recompile_graph(state, &mut preview_shader, &mut preview_uniforms);
    }

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
                    let mut layouter = |ui: &egui::Ui,
                                        text: &dyn egui::TextBuffer,
                                        wrap_width: f32| {
                        let mut layout_job = highlighter.highlight(text.as_str(), state.hovered);
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
            if let EditorTab::Module(module_id) = state.active_tab {
                draw_module_editor_tab(ui, state, module_id, &mut graph_edit);
                return;
            }

            let had_connection_in_progress = state.graph_editor.connection_in_progress.is_some();
            let had_node_finder = state.graph_editor.node_finder.is_some();
            let editor_before_draw = state.graph_editor.clone();
            let node_templates = GraphNodeTemplates::compatible_with(
                state.graph_ui.connection_context,
                &state.modules,
                &state.global_modules,
            );
            let response = state.graph_editor.draw_graph_editor(
                ui,
                node_templates,
                &mut state.graph_ui,
                Vec::new(),
            );

            state.hovered = None;
            for node_response in response.node_responses {
                match node_response {
                    NodeResponse::CreatedNode(node_id) => {
                        match connect_new_node_to_context(
                            &mut state.graph_editor,
                            &mut state.graph_ui,
                            node_id,
                        ) {
                            Ok(true) => {
                                state.graph_ui.conflict_nodes.clear();
                            }
                            Ok(false) => {}
                            Err(error) => {
                                state.graph_ui.conflict_nodes = error.nodes;
                                state.export_status = Some(error.message);
                            }
                        }
                        graph_edit.source_changed = true;
                    }
                    NodeResponse::ConnectEventEnded { input, output } => {
                        let mut candidate = editor_before_draw.clone();
                        candidate.graph.add_connection(output, input);
                        match resolve_ambiguous_output_types_checked(&mut candidate) {
                            Ok(()) => {
                                candidate.connection_in_progress = None;
                                state.graph_editor = candidate;
                                state.graph_ui.conflict_nodes.clear();
                            }
                            Err(error) => {
                                let mut restored = editor_before_draw.clone();
                                restored.connection_in_progress = None;
                                state.graph_editor = restored;
                                state.graph_ui.conflict_nodes = error.nodes;
                                state.export_status = Some(error.message);
                            }
                        }
                        state.graph_ui.connection_context = None;
                        graph_edit.source_changed = true;
                    }
                    NodeResponse::DeleteNodeFull { .. } | NodeResponse::DeleteNodeUi(_) => {
                        resolve_ambiguous_output_types(&mut state.graph_editor);
                        state.graph_ui.connection_context = None;
                        graph_edit.source_changed = true;
                    }
                    NodeResponse::DisconnectEvent { output, .. } => {
                        let mut candidate = state.graph_editor.clone();
                        match resolve_ambiguous_output_types_checked(&mut candidate) {
                            Ok(()) => {
                                state.graph_editor = candidate;
                                state.graph_ui.conflict_nodes.clear();
                            }
                            Err(error) => {
                                let mut restored = editor_before_draw.clone();
                                restored.connection_in_progress = None;
                                state.graph_editor = restored;
                                state.graph_ui.conflict_nodes = error.nodes;
                                state.export_status = Some(error.message);
                            }
                        }
                        begin_disconnected_input_context(
                            &state.graph_editor,
                            &mut state.graph_ui,
                            output,
                        );
                        graph_edit.source_changed = true;
                    }
                    NodeResponse::MoveNode { .. } => {
                        graph_edit.position_changed = true;
                    }
                    NodeResponse::HoverNode(node_id) => {
                        if let Some(node) = state.graph_editor.graph.nodes.get(node_id) {
                            state.hovered = Some(node.user_data.shader_id);
                        }
                    }
                    NodeResponse::SelectConnectedNode(node_id) => {
                        select_contiguous_nodes(&mut state.graph_editor, node_id);
                        if let Some(node) = state.graph_editor.graph.nodes.get(node_id) {
                            state.selected = Some(node.user_data.shader_id);
                        }
                    }
                    NodeResponse::ContextNode(node_id, position) => {
                        state.node_context_menu = Some(NodeContextMenu {
                            node: node_id,
                            position,
                        });
                    }
                    NodeResponse::SelectNode(node_id) => {
                        if let Some(node) = state.graph_editor.graph.nodes.get(node_id) {
                            state.selected = Some(node.user_data.shader_id);
                        }
                    }
                    NodeResponse::User(ShaderGraphResponse::ValueChanged { node, value, kind }) => {
                        apply_graph_editor_value_change(&mut state.graph_editor, node, value);
                        match kind {
                            GraphValueChangeKind::Source => graph_edit.source_changed = true,
                            GraphValueChangeKind::Uniform => {
                                graph_edit.uniform_values_changed = true;
                            }
                        }
                    }
                    NodeResponse::User(ShaderGraphResponse::RenameNode { node, name }) => {
                        if !name.is_empty() {
                            rename_graph_editor_node(&mut state.graph_editor, node, &name);
                            graph_edit.source_changed = true;
                        }
                    }
                    NodeResponse::User(ShaderGraphResponse::PreviewChanged) => {
                        graph_edit.preview_changed = true;
                    }
                    NodeResponse::User(ShaderGraphResponse::LoadTextureRequested) => {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Images", &["png", "jpg", "jpeg", "bmp", "tga", "hdr"])
                            .pick_file()
                        {
                            state.pending_texture_path = Some(path.display().to_string());
                        }
                    }
                    NodeResponse::ConnectEventStarted(_, port) => {
                        begin_connection_context(&state.graph_editor, &mut state.graph_ui, port);
                    }
                    NodeResponse::RaiseNode(_) => {}
                }
            }

            draw_node_context_menu(ui, state);
            draw_module_prompt(ui, state, &mut graph_edit);

            let has_connection_in_progress = state.graph_editor.connection_in_progress.is_some();
            let has_node_finder = state.graph_editor.node_finder.is_some();
            let opened_finder_from_connection =
                !had_node_finder && has_node_finder && had_connection_in_progress;
            if state.graph_ui.connection_context.is_some()
                && !has_connection_in_progress
                && (!has_node_finder || (!had_node_finder && !opened_finder_from_connection))
            {
                state.graph_ui.connection_context = None;
            }

            if graph_edit.source_changed
                || graph_edit.uniform_values_changed
                || graph_edit.position_changed
            {
                state.graph = shader_graph_from_editor(&state.graph_editor);
            }
            if graph_edit.preview_changed {
                state.preview_node = state.graph_ui.preview_node;
            }
        });

    if let EditorTab::Module(module_id) = state.active_tab
        && let Some(tab) = state
            .module_tabs
            .iter_mut()
            .find(|tab| tab.module_id == module_id)
    {
        tab.preview_node = tab.graph_ui.preview_node;
    }

    if let Some(path) = state.pending_texture_path.take() {
        match load_texture_image(&path, &mut images) {
            Ok(handle) => {
                state.texture_path = Some(path);
                state.graph_ui.texture_path.clone_from(&state.texture_path);
                preview_texture.handle = handle;
                preview_texture.path = state.texture_path.clone();
                recompile_requested = true;
            }
            Err(error) => {
                tracing::error!("Failed to load texture {path}: {error}");
            }
        }
    }

    if recompile_requested || graph_edit.source_changed || graph_edit.preview_changed {
        recompile_graph(state, &mut preview_shader, &mut preview_uniforms);
    } else if graph_edit.uniform_values_changed {
        sync_preview_uniforms(state, &mut preview_uniforms);
    }

    Ok(())
}

fn draw_file_menu(ui: &mut egui::Ui, state: &mut EditorState) {
    ui.menu_button("File", |ui| {
        if ui.button("Export Project...").clicked() {
            state.export_status = Some(match export_project(state) {
                Ok(path) => format!("Exported project to {}", path.display()),
                Err(error) => format!("Project export failed: {error}"),
            });
            ui.close();
        }

        ui.separator();
        match state.compilation.clone() {
            Ok(compiled) => {
                if ui.button("Export Generated WESL...").clicked() {
                    state.export_status = Some(
                        match export_text_file(
                            "Export Generated WESL",
                            "shader_graph.wesl",
                            "WESL",
                            &["wesl"],
                            &compiled.wesl,
                        ) {
                            Ok(path) => format!("Exported WESL to {}", path.display()),
                            Err(error) => format!("WESL export failed: {error}"),
                        },
                    );
                    ui.close();
                }
                if ui.button("Export Linked WGSL...").clicked() {
                    state.export_status = Some(
                        match export_text_file(
                            "Export Linked WGSL",
                            "shader_graph.wgsl",
                            "WGSL",
                            &["wgsl"],
                            &compiled.wgsl,
                        ) {
                            Ok(path) => format!("Exported WGSL to {}", path.display()),
                            Err(error) => format!("WGSL export failed: {error}"),
                        },
                    );
                    ui.close();
                }
                if ui.button("Export Bevy Preview WESL...").clicked() {
                    state.export_status = Some(
                        match export_text_file(
                            "Export Bevy Preview WESL",
                            "shader_graph_preview.wesl",
                            "WESL",
                            &["wesl"],
                            &compiled.bevy_wesl,
                        ) {
                            Ok(path) => format!("Exported Bevy preview WESL to {}", path.display()),
                            Err(error) => format!("Bevy preview WESL export failed: {error}"),
                        },
                    );
                    ui.close();
                }
                if ui.button("Export Bevy Preview WGSL...").clicked() {
                    state.export_status = Some(
                        match export_text_file(
                            "Export Bevy Preview WGSL",
                            "shader_graph_preview.wgsl",
                            "WGSL",
                            &["wgsl"],
                            &compiled.bevy_wgsl,
                        ) {
                            Ok(path) => format!("Exported Bevy preview WGSL to {}", path.display()),
                            Err(error) => format!("Bevy preview WGSL export failed: {error}"),
                        },
                    );
                    ui.close();
                }
                if ui.button("Export All Shader Files...").clicked() {
                    state.export_status = Some(match export_all_shader_files(&compiled) {
                        Ok(path) => format!("Exported shader files to {}", path.display()),
                        Err(error) => format!("Shader export failed: {error}"),
                    });
                    ui.close();
                }
            }
            Err(error) => {
                ui.add_enabled_ui(false, |ui| {
                    let _ = ui.button("Export Generated WESL...");
                    let _ = ui.button("Export Linked WGSL...");
                    let _ = ui.button("Export Bevy Preview WESL...");
                    let _ = ui.button("Export Bevy Preview WGSL...");
                    let _ = ui.button("Export All Shader Files...");
                });
                ui.colored_label(egui::Color32::from_rgb(240, 95, 95), error);
            }
        }
    });
}

fn export_project(state: &EditorState) -> Result<std::path::PathBuf, String> {
    let path = rfd::FileDialog::new()
        .set_title("Export Project")
        .add_filter("RON", &["ron"])
        .set_file_name("shader_graph_project.ron")
        .save_file()
        .ok_or_else(|| "cancelled".to_owned())?;
    let project = ProjectFile {
        graph: state.graph.clone(),
        preview_node: state.preview_node,
        texture_path: state.texture_path.clone(),
        modules: state.modules.clone(),
    };
    let source = ron::ser::to_string_pretty(&project, ron::ser::PrettyConfig::new())
        .map_err(|error| error.to_string())?;
    std::fs::write(&path, source).map_err(|error| error.to_string())?;
    Ok(path)
}

fn export_text_file(
    title: &str,
    file_name: &str,
    filter_name: &str,
    extensions: &[&str],
    source: &str,
) -> Result<std::path::PathBuf, String> {
    let path = rfd::FileDialog::new()
        .set_title(title)
        .add_filter(filter_name, extensions)
        .set_file_name(file_name)
        .save_file()
        .ok_or_else(|| "cancelled".to_owned())?;
    std::fs::write(&path, source).map_err(|error| error.to_string())?;
    Ok(path)
}

fn export_all_shader_files(compiled: &CompiledShader) -> Result<std::path::PathBuf, String> {
    let directory = rfd::FileDialog::new()
        .set_title("Export All Shader Files")
        .pick_folder()
        .ok_or_else(|| "cancelled".to_owned())?;
    let files = [
        ("shader_graph.wesl", compiled.wesl.as_str()),
        ("shader_graph.wgsl", compiled.wgsl.as_str()),
        ("shader_graph_preview.wesl", compiled.bevy_wesl.as_str()),
        ("shader_graph_preview.wgsl", compiled.bevy_wgsl.as_str()),
    ];
    for (file_name, source) in files {
        std::fs::write(directory.join(file_name), source).map_err(|error| error.to_string())?;
    }
    Ok(directory)
}

fn select_contiguous_nodes(editor: &mut ShaderGraphEditorState, start: UiNodeId) {
    let selected = contiguous_ui_nodes(editor, start);
    editor.selected_nodes = selected;
}

fn contiguous_ui_nodes(editor: &ShaderGraphEditorState, start: UiNodeId) -> Vec<UiNodeId> {
    let mut selected = Vec::new();
    let mut stack = vec![start];
    while let Some(node) = stack.pop() {
        if selected.contains(&node) {
            continue;
        }
        selected.push(node);

        for (input, output) in editor.graph.iter_connections() {
            let input_node = editor.graph.get_input(input).node;
            let output_node = editor.graph.get_output(output).node;
            if input_node == node && !selected.contains(&output_node) {
                stack.push(output_node);
            }
            if output_node == node && !selected.contains(&input_node) {
                stack.push(input_node);
            }
        }
    }
    selected
}

fn draw_node_context_menu(ui: &mut egui::Ui, state: &mut EditorState) {
    let Some(menu) = state.node_context_menu.clone() else {
        return;
    };

    let mut close = false;
    egui::Area::new("node_context_menu".into())
        .order(egui::Order::Foreground)
        .fixed_pos(menu.position)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                let module = state
                    .graph_editor
                    .graph
                    .nodes
                    .get(menu.node)
                    .and_then(|node| match &node.user_data.kind {
                        NodeKind::Module(module) => Some((**module).clone()),
                        _ => None,
                    });
                if let Some(module) = module {
                    if ui.button("Edit Module").clicked() {
                        open_module_editor_tab(state, &module);
                        close = true;
                    }
                    ui.separator();
                }
                if ui.button("Create Module Node").clicked() {
                    state.module_prompt = Some(ModulePrompt {
                        root: menu.node,
                        position: menu.position,
                        module_name: format!("Module {}", state.modules.len() + 1),
                        save_to_user_library: false,
                    });
                    close = true;
                }
            });
        });

    if close || ui.input(|i| i.pointer.primary_clicked() || i.key_pressed(egui::Key::Escape)) {
        state.node_context_menu = None;
    }
}

fn draw_module_prompt(ui: &mut egui::Ui, state: &mut EditorState, graph_edit: &mut GraphEdit) {
    let Some(mut prompt) = state.module_prompt.clone() else {
        return;
    };

    let mut close = false;
    egui::Window::new("Create Module Node")
        .id("create_module_prompt".into())
        .collapsible(false)
        .resizable(false)
        .default_pos(prompt.position)
        .show(ui.ctx(), |ui| {
            ui.label("Node name");
            ui.text_edit_singleline(&mut prompt.module_name);
            ui.checkbox(
                &mut prompt.save_to_user_library,
                "Also save to user module library",
            );
            ui.horizontal(|ui| {
                if ui.button("Create").clicked() {
                    match create_module_from_selection(state, prompt.root, &prompt.module_name) {
                        Ok(module) => {
                            let summary = module_summary(&module);
                            let save_status = if prompt.save_to_user_library {
                                match save_user_module(&module) {
                                    Ok(path) => {
                                        upsert_module(&mut state.global_modules, module.clone());
                                        format!(" · saved to {}", path.display())
                                    }
                                    Err(error) => format!(" · local save failed: {error}"),
                                }
                            } else {
                                " · packed into project".to_owned()
                            };
                            match replace_selection_with_module(state, prompt.root, &module) {
                                Ok(()) => {
                                    upsert_module(&mut state.modules, module);
                                    state.export_status = Some(format!("{summary}{save_status}"));
                                    graph_edit.source_changed = true;
                                    graph_edit.preview_changed = true;
                                    close = true;
                                }
                                Err(error) => {
                                    state.export_status =
                                        Some(format!("Create module failed: {error}"));
                                }
                            }
                        }
                        Err(error) => {
                            state.export_status = Some(format!("Create module failed: {error}"));
                        }
                    }
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });

    if close || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.module_prompt = None;
    } else if let Some(current) = &mut state.module_prompt {
        *current = prompt;
    }
}

fn draw_module_editor_tab(
    ui: &mut egui::Ui,
    state: &mut EditorState,
    module_id: u64,
    graph_edit: &mut GraphEdit,
) {
    let Some(tab_index) = state
        .module_tabs
        .iter()
        .position(|tab| tab.module_id == module_id)
    else {
        state.active_tab = EditorTab::Main;
        return;
    };

    let mut source_changed = false;
    let mut position_changed = false;
    let mut preview_changed = false;
    let mut pending_texture = None;
    let mut export_status = None;
    let mut hovered = None;

    {
        let tab = &mut state.module_tabs[tab_index];
        let had_connection_in_progress = tab.graph_editor.connection_in_progress.is_some();
        let had_node_finder = tab.graph_editor.node_finder.is_some();
        let editor_before_draw = tab.graph_editor.clone();
        let node_templates = GraphNodeTemplates::compatible_with(
            tab.graph_ui.connection_context,
            &state.modules,
            &state.global_modules,
        );
        let response =
            tab.graph_editor
                .draw_graph_editor(ui, node_templates, &mut tab.graph_ui, Vec::new());

        for node_response in response.node_responses {
            match node_response {
                NodeResponse::CreatedNode(node_id) => {
                    match connect_new_node_to_context(
                        &mut tab.graph_editor,
                        &mut tab.graph_ui,
                        node_id,
                    ) {
                        Ok(true) => tab.graph_ui.conflict_nodes.clear(),
                        Ok(false) => {}
                        Err(error) => {
                            tab.graph_ui.conflict_nodes = error.nodes;
                            export_status = Some(error.message);
                        }
                    }
                    source_changed = true;
                }
                NodeResponse::ConnectEventEnded { input, output } => {
                    let mut candidate = editor_before_draw.clone();
                    candidate.graph.add_connection(output, input);
                    match resolve_ambiguous_output_types_checked(&mut candidate) {
                        Ok(()) => {
                            candidate.connection_in_progress = None;
                            tab.graph_editor = candidate;
                            tab.graph_ui.conflict_nodes.clear();
                        }
                        Err(error) => {
                            let mut restored = editor_before_draw.clone();
                            restored.connection_in_progress = None;
                            tab.graph_editor = restored;
                            tab.graph_ui.conflict_nodes = error.nodes;
                            export_status = Some(error.message);
                        }
                    }
                    tab.graph_ui.connection_context = None;
                    source_changed = true;
                }
                NodeResponse::DeleteNodeFull { .. } | NodeResponse::DeleteNodeUi(_) => {
                    resolve_ambiguous_output_types(&mut tab.graph_editor);
                    tab.graph_ui.connection_context = None;
                    source_changed = true;
                }
                NodeResponse::DisconnectEvent { output, .. } => {
                    let mut candidate = tab.graph_editor.clone();
                    match resolve_ambiguous_output_types_checked(&mut candidate) {
                        Ok(()) => {
                            tab.graph_editor = candidate;
                            tab.graph_ui.conflict_nodes.clear();
                        }
                        Err(error) => {
                            let mut restored = editor_before_draw.clone();
                            restored.connection_in_progress = None;
                            tab.graph_editor = restored;
                            tab.graph_ui.conflict_nodes = error.nodes;
                            export_status = Some(error.message);
                        }
                    }
                    begin_disconnected_input_context(&tab.graph_editor, &mut tab.graph_ui, output);
                    source_changed = true;
                }
                NodeResponse::MoveNode { .. } => {
                    position_changed = true;
                }
                NodeResponse::HoverNode(node_id) => {
                    if let Some(node) = tab.graph_editor.graph.nodes.get(node_id) {
                        hovered = Some(node.user_data.shader_id);
                    }
                }
                NodeResponse::SelectConnectedNode(node_id) => {
                    select_contiguous_nodes(&mut tab.graph_editor, node_id);
                }
                NodeResponse::SelectNode(_) | NodeResponse::ContextNode(_, _) => {}
                NodeResponse::User(ShaderGraphResponse::ValueChanged { node, value, kind }) => {
                    apply_graph_editor_value_change(&mut tab.graph_editor, node, value);
                    match kind {
                        GraphValueChangeKind::Source => source_changed = true,
                        GraphValueChangeKind::Uniform => source_changed = true,
                    }
                }
                NodeResponse::User(ShaderGraphResponse::RenameNode { node, name }) => {
                    if !name.is_empty() {
                        rename_graph_editor_node(&mut tab.graph_editor, node, &name);
                        source_changed = true;
                    }
                }
                NodeResponse::User(ShaderGraphResponse::PreviewChanged) => {
                    preview_changed = true;
                }
                NodeResponse::User(ShaderGraphResponse::LoadTextureRequested) => {
                    pending_texture = rfd::FileDialog::new()
                        .add_filter("Images", &["png", "jpg", "jpeg", "bmp", "tga", "hdr"])
                        .pick_file()
                        .map(|path| path.display().to_string());
                }
                NodeResponse::ConnectEventStarted(_, port) => {
                    begin_connection_context(&tab.graph_editor, &mut tab.graph_ui, port);
                }
                NodeResponse::RaiseNode(_) => {}
            }
        }

        let has_connection_in_progress = tab.graph_editor.connection_in_progress.is_some();
        let has_node_finder = tab.graph_editor.node_finder.is_some();
        let opened_finder_from_connection =
            !had_node_finder && has_node_finder && had_connection_in_progress;
        if tab.graph_ui.connection_context.is_some()
            && !has_connection_in_progress
            && (!has_node_finder || (!had_node_finder && !opened_finder_from_connection))
        {
            tab.graph_ui.connection_context = None;
        }
    }

    state.hovered = hovered;
    if let Some(status) = export_status {
        state.export_status = Some(status);
    }
    if let Some(path) = pending_texture {
        state.pending_texture_path = Some(path);
    }
    if source_changed {
        sync_module_tab(state, module_id, true);
        graph_edit.source_changed = true;
    }
    if position_changed {
        sync_module_tab(state, module_id, false);
        graph_edit.position_changed = true;
    }
    if preview_changed {
        graph_edit.preview_changed = true;
    }
}

fn create_module_from_selection(
    state: &EditorState,
    root_ui_node: UiNodeId,
    module_name: &str,
) -> Result<ModuleDefinition, String> {
    let selected_ui_nodes = if state.graph_editor.selected_nodes.contains(&root_ui_node) {
        state.graph_editor.selected_nodes.clone()
    } else {
        contiguous_ui_nodes(&state.graph_editor, root_ui_node)
    };
    if selected_ui_nodes.is_empty() {
        return Err("no nodes selected".to_owned());
    }

    let mut shader_nodes = selected_ui_nodes
        .iter()
        .filter_map(|node| state.graph_editor.graph.nodes.get(*node))
        .map(|node| node.user_data.shader_id)
        .collect::<Vec<_>>();
    shader_nodes.sort_by_key(|id| id.0);
    shader_nodes.dedup();

    let root = state
        .graph_editor
        .graph
        .nodes
        .get(root_ui_node)
        .map(|node| node.user_data.shader_id)
        .ok_or_else(|| "context node no longer exists".to_owned())?;
    let root = module_root_from_shader_graph(&state.graph, root);

    let input_node_set = shader_nodes
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    let mut inputs = state
        .graph
        .nodes
        .iter()
        .filter(|node| input_node_set.contains(&node.id))
        .filter_map(|node| {
            if let NodeKind::Uniform(value) = &node.kind {
                Some(ModulePort {
                    name: node.name.clone(),
                    shader_type: value.shader_type(),
                    node: Some(node.id),
                })
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    inputs.sort_by_key(|input| input.node.map_or(0, |node| node.0));

    let output_type = state
        .compilation
        .as_ref()
        .ok()
        .and_then(|compiled| compiled.node_types.get(&root).copied())
        .unwrap_or(ShaderType::Vec4);
    let graph = module_subgraph_from_nodes(&state.graph, &shader_nodes);
    let root = module_root_from_shader_graph(&graph, root);

    Ok(ModuleDefinition {
        id: state.modules.len() as u64 + 1,
        name: sanitized_module_display_name(module_name, state.modules.len() + 1),
        root,
        nodes: shader_nodes,
        inputs,
        output: ModulePort {
            name: "Output".to_owned(),
            shader_type: output_type,
            node: Some(root),
        },
        graph: Box::new(graph),
    })
}

fn replace_selection_with_module(
    state: &mut EditorState,
    root_ui_node: UiNodeId,
    module: &ModuleDefinition,
) -> Result<(), String> {
    let selected_ui_nodes = if state.graph_editor.selected_nodes.contains(&root_ui_node) {
        state.graph_editor.selected_nodes.clone()
    } else {
        contiguous_ui_nodes(&state.graph_editor, root_ui_node)
    };
    if selected_ui_nodes.is_empty() {
        return Err("no nodes selected".to_owned());
    }

    let selected_set = selected_ui_nodes
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    let root_shader = module.root;
    let cloned_inputs = module
        .inputs
        .iter()
        .map(|input| {
            let value = input
                .node
                .and_then(|node_id| state.graph.node(node_id))
                .and_then(|node| match &node.kind {
                    NodeKind::Uniform(value) => Some(value.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| default_value_for_shader_type(input.shader_type));
            (input.name.clone(), value)
        })
        .collect::<Vec<_>>();

    let outgoing_targets = state
        .graph_editor
        .graph
        .iter_connections()
        .filter_map(|(input_id, output_id)| {
            let input_node = state.graph_editor.graph.get_input(input_id).node;
            let output_node = state.graph_editor.graph.get_output(output_id).node;
            let output_shader = state
                .graph_editor
                .graph
                .nodes
                .get(output_node)
                .map(|node| node.user_data.shader_id)?;
            (selected_set.contains(&output_node)
                && !selected_set.contains(&input_node)
                && output_shader == root_shader)
                .then_some(input_id)
        })
        .collect::<Vec<UiInputId>>();

    let position = average_node_position(&state.graph_editor, &selected_ui_nodes)
        .unwrap_or(egui::pos2(240.0, 240.0));

    for node_id in selected_ui_nodes {
        if state.graph_editor.graph.nodes.contains_key(node_id) {
            state.graph_editor.graph.remove_node(node_id);
        }
        state
            .graph_editor
            .node_order
            .retain(|node| *node != node_id);
        state.graph_editor.node_positions.remove(node_id);
        state.graph_editor.node_orientations.remove(node_id);
        state
            .graph_editor
            .selected_nodes
            .retain(|node| *node != node_id);
    }

    let module_kind = NodeKind::Module(Box::new(module.clone()));
    let (module_ui_node, module_shader_id) = add_kind_to_editor(
        &mut state.graph_editor,
        &mut state.graph_ui,
        module_kind,
        module.name.clone(),
        position,
    );

    let mut selected_nodes = vec![module_ui_node];
    for (index, (name, value)) in cloned_inputs.into_iter().enumerate() {
        let input_position = egui::pos2(
            position.x - 280.0,
            position.y + index as f32 * 110.0 - module.inputs.len() as f32 * 55.0 + 55.0,
        );
        let (uniform_ui_node, _) = add_kind_to_editor(
            &mut state.graph_editor,
            &mut state.graph_ui,
            NodeKind::Uniform(value),
            name,
            input_position,
        );
        if let (Some(uniform_output), Some(module_input)) = (
            state.graph_editor.graph.nodes[uniform_ui_node]
                .outputs
                .first()
                .map(|(_, output)| *output),
            state.graph_editor.graph.nodes[module_ui_node]
                .inputs
                .get(index)
                .map(|(_, input)| *input),
        ) {
            state
                .graph_editor
                .graph
                .add_connection(uniform_output, module_input);
        }
        selected_nodes.push(uniform_ui_node);
    }

    if let Some(module_output) = state.graph_editor.graph.nodes[module_ui_node]
        .outputs
        .first()
        .map(|(_, output)| *output)
    {
        let (output_ui_node, _) = add_kind_to_editor(
            &mut state.graph_editor,
            &mut state.graph_ui,
            NodeKind::FragmentOutput,
            "Fragment Output".to_owned(),
            egui::pos2(position.x + 320.0, position.y),
        );
        if let Some(output_input) = state.graph_editor.graph.nodes[output_ui_node]
            .inputs
            .first()
            .map(|(_, input)| *input)
        {
            state
                .graph_editor
                .graph
                .add_connection(module_output, output_input);
        }
        selected_nodes.push(output_ui_node);

        for input_id in outgoing_targets {
            if state.graph_editor.graph.inputs.contains_key(input_id) {
                state
                    .graph_editor
                    .graph
                    .add_connection(module_output, input_id);
            }
        }
    }

    state.graph_editor.selected_nodes = selected_nodes;
    state.selected = Some(module_shader_id);
    if state
        .preview_node
        .is_some_and(|preview_node| module.nodes.contains(&preview_node))
    {
        state.preview_node = Some(module_shader_id);
        state.graph_ui.preview_node = Some(module_shader_id);
    }
    state.graph = shader_graph_from_editor(&state.graph_editor);
    Ok(())
}

fn default_value_for_shader_type(shader_type: ShaderType) -> Value {
    match shader_type {
        ShaderType::F32 => Value::F32(1.0),
        ShaderType::Vec2 => Value::Vec2([1.0, 1.0]),
        ShaderType::Vec3 => Value::Vec3([1.0, 1.0, 1.0]),
        ShaderType::Vec4 => Value::Vec4([1.0, 1.0, 1.0, 1.0]),
    }
}

fn average_node_position(
    editor: &ShaderGraphEditorState,
    nodes: &[UiNodeId],
) -> Option<egui::Pos2> {
    let positions = nodes
        .iter()
        .filter_map(|node| editor.node_positions.get(*node))
        .collect::<Vec<_>>();
    if positions.is_empty() {
        return None;
    }
    let sum = positions
        .iter()
        .fold(egui::Vec2::ZERO, |sum, position| sum + position.to_vec2());
    Some(egui::pos2(
        sum.x / positions.len() as f32,
        sum.y / positions.len() as f32,
    ))
}

fn rename_graph_editor_node(editor: &mut ShaderGraphEditorState, shader_node: NodeId, name: &str) {
    for (_, node) in &mut editor.graph.nodes {
        if node.user_data.shader_id == shader_node {
            node.label = name.to_owned();
            break;
        }
    }
}

fn save_user_module(module: &ModuleDefinition) -> Result<std::path::PathBuf, String> {
    let directory = user_modules_dir()?;
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let file_name = format!("{}.ron", module_file_stem(&module.name));
    let path = unique_module_path(&directory, &file_name);
    let project = ProjectFile {
        graph: (*module.graph).clone(),
        preview_node: Some(module.root),
        texture_path: None,
        modules: vec![module.clone()],
    };
    let source = ron::ser::to_string_pretty(&project, ron::ser::PrettyConfig::new())
        .map_err(|error| error.to_string())?;
    std::fs::write(&path, source).map_err(|error| error.to_string())?;
    Ok(path)
}

fn load_user_modules() -> Result<Vec<ModuleDefinition>, String> {
    let directory = user_modules_dir()?;
    if !directory.exists() {
        return Ok(Vec::new());
    }

    let mut modules = Vec::new();
    for entry in std::fs::read_dir(&directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("ron") {
            continue;
        }
        let source = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let project = ron::from_str::<ProjectFile>(&source).map_err(|error| error.to_string())?;
        modules.extend(project.modules);
    }
    modules.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
    modules.dedup_by(|left, right| left.id == right.id && left.name == right.name);
    Ok(modules)
}

fn upsert_module(modules: &mut Vec<ModuleDefinition>, module: ModuleDefinition) {
    if let Some(existing) = modules
        .iter_mut()
        .find(|existing| existing.id == module.id && existing.name == module.name)
    {
        *existing = module;
    } else {
        modules.push(module);
    }
}

fn module_subgraph_from_nodes(graph: &ShaderGraph, nodes: &[NodeId]) -> ShaderGraph {
    let node_set = nodes
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    let mut nodes = graph
        .nodes
        .iter()
        .filter(|node| node_set.contains(&node.id))
        .cloned()
        .collect::<Vec<_>>();
    for node in &mut nodes {
        for input in &mut node.inputs {
            if input.is_some_and(|connection| !node_set.contains(&connection.node)) {
                *input = None;
            }
        }
    }
    ShaderGraph {
        format_version: graph.format_version,
        nodes,
    }
}

fn module_root_from_shader_graph(graph: &ShaderGraph, fallback: NodeId) -> NodeId {
    graph
        .fragment_output()
        .and_then(|output| output.inputs.first())
        .and_then(|connection| *connection)
        .map(|connection| connection.node)
        .unwrap_or(fallback)
}

fn user_modules_dir() -> Result<std::path::PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or_else(|| "HOME is not set".to_owned())?;
    Ok(std::path::PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("wesl_shader_graph")
        .join("user_modules"))
}

fn sanitized_module_display_name(name: &str, fallback_index: usize) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        format!("Module {fallback_index}")
    } else {
        trimmed.to_owned()
    }
}

fn module_file_stem(name: &str) -> String {
    let stem = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_owned();
    if stem.is_empty() {
        "module".to_owned()
    } else {
        stem
    }
}

fn unique_module_path(directory: &std::path::Path, file_name: &str) -> std::path::PathBuf {
    let initial = directory.join(file_name);
    if !initial.exists() {
        return initial;
    }
    let stem = file_name.trim_end_matches(".ron");
    for index in 2.. {
        let path = directory.join(format!("{stem}_{index}.ron"));
        if !path.exists() {
            return path;
        }
    }
    unreachable!("unbounded loop must return a non-existing module path")
}

fn module_summary(module: &ModuleDefinition) -> String {
    let input_summary = if module.inputs.is_empty() {
        "no inputs".to_owned()
    } else {
        module
            .inputs
            .iter()
            .map(|input| format!("{}: {}", input.name, input.shader_type.wgsl()))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "Created {} #{} from {} nodes · inputs [{}] · output {}: {} from node {}",
        module.name,
        module.id,
        module.nodes.len(),
        input_summary,
        module.output.name,
        module.output.shader_type.wgsl(),
        module.root.0,
    )
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NodeHighlightRange {
    start: usize,
    end: usize,
}

struct SourceHighlighter {
    parser: Parser,
    cached_source: String,
    cached_hovered: Option<NodeId>,
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
            cached_hovered: None,
            cached_job: egui::text::LayoutJob::default(),
        }
    }
}

impl SourceHighlighter {
    fn highlight(&mut self, source: &str, hovered: Option<NodeId>) -> egui::text::LayoutJob {
        if self.cached_source != source || self.cached_hovered != hovered {
            self.cached_source.clear();
            self.cached_source.push_str(source);
            self.cached_hovered = hovered;
            self.cached_job = self.build_layout_job(source, hovered);
        }
        self.cached_job.clone()
    }

    fn build_layout_job(&mut self, source: &str, hovered: Option<NodeId>) -> egui::text::LayoutJob {
        let mut spans = Vec::new();
        if let Some(tree) = self.parser.parse(source, None) {
            collect_highlight_spans(tree.root_node(), source.as_bytes(), &mut spans);
        }
        spans.sort_by_key(|span| (span.start, span.end));
        spans.dedup_by_key(|span| (span.start, span.end));

        let node_highlights = node_highlight_ranges(source, hovered);
        let mut job = egui::text::LayoutJob::default();
        let mut cursor = 0;
        for span in spans {
            if span.start < cursor || span.end > source.len() || span.start >= span.end {
                continue;
            }
            if cursor < span.start {
                append_source_text(&mut job, source, cursor, span.start, None, &node_highlights);
            }
            append_source_text(
                &mut job,
                source,
                span.start,
                span.end,
                Some(span.kind),
                &node_highlights,
            );
            cursor = span.end;
        }
        if cursor < source.len() {
            append_source_text(
                &mut job,
                source,
                cursor,
                source.len(),
                None,
                &node_highlights,
            );
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

fn node_highlight_ranges(source: &str, hovered: Option<NodeId>) -> Vec<NodeHighlightRange> {
    let mut ranges = Vec::new();
    if let Some(node_id) = hovered {
        ranges.extend(
            source_ranges_for_node(source, node_id)
                .into_iter()
                .map(|(start, end)| NodeHighlightRange { start, end }),
        );
    }
    ranges
}

fn source_ranges_for_node(source: &str, node_id: NodeId) -> Vec<(usize, usize)> {
    let marker = format!("// node: {} ", node_id.0);
    source
        .match_indices(&marker)
        .filter_map(|(marker_start, _)| {
            let line_start = source[..marker_start]
                .rfind('\n')
                .map_or(0, |index| index + 1);
            let comment_line_end = source[marker_start..]
                .find('\n')
                .map_or(source.len(), |offset| marker_start + offset + 1);
            let next_line_end = if comment_line_end < source.len() {
                source[comment_line_end..]
                    .find('\n')
                    .map_or(source.len(), |offset| comment_line_end + offset + 1)
            } else {
                comment_line_end
            };
            (line_start < next_line_end).then_some((line_start, next_line_end))
        })
        .collect()
}

fn append_source_text(
    job: &mut egui::text::LayoutJob,
    source: &str,
    start: usize,
    end: usize,
    highlight: Option<HighlightKind>,
    node_highlights: &[NodeHighlightRange],
) {
    let mut cursor = start;
    while cursor < end {
        let next = next_node_highlight_boundary(cursor, end, node_highlights);
        let strength = node_highlight_at(cursor, node_highlights);
        job.append(&source[cursor..next], 0.0, text_format(highlight, strength));
        cursor = next;
    }
}

fn next_node_highlight_boundary(
    cursor: usize,
    end: usize,
    node_highlights: &[NodeHighlightRange],
) -> usize {
    node_highlights
        .iter()
        .flat_map(|range| [range.start, range.end])
        .filter(|boundary| *boundary > cursor && *boundary < end)
        .min()
        .unwrap_or(end)
}

fn node_highlight_at(position: usize, node_highlights: &[NodeHighlightRange]) -> bool {
    node_highlights
        .iter()
        .any(|range| position >= range.start && position < range.end)
}

fn text_format(highlight: Option<HighlightKind>, node_highlight: bool) -> egui::TextFormat {
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
        background: if node_highlight {
            egui::Color32::from_rgba_unmultiplied(125, 190, 255, 88)
        } else {
            egui::Color32::TRANSPARENT
        },
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

#[derive(Clone, Copy, Debug, Default)]
struct GraphEdit {
    source_changed: bool,
    preview_changed: bool,
    uniform_values_changed: bool,
    position_changed: bool,
}

fn recompile_graph(
    state: &mut EditorState,
    preview_shader: &mut PreviewShaderSource,
    preview_uniforms: &mut PreviewUniformValues,
) {
    state.compilation = match state.active_tab {
        EditorTab::Main => compile_with_preview_node(&state.graph, state.preview_node)
            .map_err(|error| error.to_string()),
        EditorTab::Module(module_id) => {
            if let Some(tab) = state
                .module_tabs
                .iter()
                .find(|tab| tab.module_id == module_id)
            {
                let preview_node = tab.preview_node.unwrap_or(tab.module.root);
                compile_preview_graph(&tab.graph, preview_node).map_err(|error| error.to_string())
            } else {
                compile_with_preview_node(&state.graph, state.preview_node)
                    .map_err(|error| error.to_string())
            }
        }
    };
    if let Ok(compiled) = &state.compilation {
        if preview_shader.wesl != compiled.bevy_wesl || preview_shader.wgsl != compiled.bevy_wgsl {
            preview_shader.wesl.clone_from(&compiled.bevy_wesl);
            preview_shader.wgsl.clone_from(&compiled.bevy_wgsl);
        }
        update_preview_uniforms(active_graph(state), compiled, preview_uniforms);
    }
}

fn apply_graph_editor_value_change(
    editor: &mut ShaderGraphEditorState,
    shader_node_id: NodeId,
    value: Value,
) {
    for (_, node) in editor.graph.nodes.iter_mut() {
        if node.user_data.shader_id == shader_node_id {
            match &mut node.user_data.kind {
                NodeKind::Constant(current) | NodeKind::Uniform(current) => {
                    *current = value;
                }
                _ => {}
            }
            break;
        }
    }
}

fn sync_preview_uniforms(state: &EditorState, preview_uniforms: &mut PreviewUniformValues) {
    if let Ok(compiled) = &state.compilation {
        update_preview_uniforms(active_graph(state), compiled, preview_uniforms);
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

fn load_texture_image(path: &str, images: &mut Assets<Image>) -> Result<Handle<Image>, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
    let (width, height) = img.dimensions();
    let rgba = img.to_rgba8().into_raw();
    let image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    Ok(images.add(image))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_names_are_sanitized_for_storage() {
        assert_eq!(sanitized_module_display_name("", 3), "Module 3");
        assert_eq!(
            sanitized_module_display_name("  Fancy Noise  ", 1),
            "Fancy Noise"
        );
        assert_eq!(module_file_stem("Fancy Noise!"), "fancy_noise");
        assert_eq!(module_file_stem("???"), "module");
    }
}
