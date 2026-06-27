use bevy::{
    asset::{RenderAssetUsages, uuid_handle},
    pbr::Material,
    prelude::*,
    reflect::TypePath,
    render::{
        render_resource::{
            AsBindGroup, Extent3d, Face, ShaderType, TextureDimension, TextureFormat,
        },
        storage::ShaderBuffer,
    },
    shader::{Shader, ShaderRef},
};
use wesl_shader_graph_editor::{compiler::compile, graph::ShaderGraph};

pub struct PreviewPlugin;

const GRAPH_SHADER_HANDLE: Handle<Shader> = uuid_handle!("70d9cf09-e18c-4f8d-ad44-a956859cb932");

impl Plugin for PreviewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PreviewSettings>()
            .init_resource::<PreviewShaderSource>()
            .init_resource::<PreviewUniformValues>()
            .init_resource::<PreviewTexture>()
            .add_plugins(MaterialPlugin::<GraphMaterial>::default())
            .add_systems(Startup, setup_preview)
            .add_systems(
                Update,
                (
                    apply_preview_primitive,
                    advance_preview_time,
                    apply_preview_uniforms,
                    apply_preview_texture,
                    update_preview_shader,
                    animate_preview,
                ),
            );
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PreviewPrimitive {
    #[default]
    Cube,
    Sphere,
    Torus,
    Capsule,
}

impl PreviewPrimitive {
    pub const ALL: [Self; 4] = [Self::Cube, Self::Sphere, Self::Torus, Self::Capsule];

    pub fn label(self) -> &'static str {
        match self {
            Self::Cube => "Cube",
            Self::Sphere => "Sphere",
            Self::Torus => "Torus",
            Self::Capsule => "Capsule",
        }
    }
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct PreviewSettings {
    pub primitive: PreviewPrimitive,
}

impl Default for PreviewSettings {
    fn default() -> Self {
        Self {
            primitive: PreviewPrimitive::Cube,
        }
    }
}

#[derive(Resource, Clone, Debug)]
pub struct PreviewShaderSource {
    pub wesl: String,
    pub wgsl: String,
}

impl Default for PreviewShaderSource {
    fn default() -> Self {
        let compiled = compile(&ShaderGraph::example()).expect("example graph must compile");
        Self {
            wesl: compiled.bevy_wesl,
            wgsl: compiled.bevy_wgsl,
        }
    }
}

#[derive(Resource, Clone, Debug)]
pub struct PreviewUniformValues {
    pub values: Vec<[f32; 4]>,
}

impl Default for PreviewUniformValues {
    fn default() -> Self {
        Self {
            values: vec![[0.0; 4]],
        }
    }
}

#[derive(Resource)]
struct PreviewMeshes {
    cube: Handle<Mesh>,
    sphere: Handle<Mesh>,
    torus: Handle<Mesh>,
    capsule: Handle<Mesh>,
}

impl PreviewMeshes {
    fn get(&self, primitive: PreviewPrimitive) -> Handle<Mesh> {
        match primitive {
            PreviewPrimitive::Cube => self.cube.clone(),
            PreviewPrimitive::Sphere => self.sphere.clone(),
            PreviewPrimitive::Torus => self.torus.clone(),
            PreviewPrimitive::Capsule => self.capsule.clone(),
        }
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub(crate) struct GraphMaterial {
    #[uniform(0)]
    uniforms: GraphUniforms,
    #[storage(1, read_only)]
    user_uniforms: Handle<ShaderBuffer>,
    #[texture(2)]
    #[sampler(3)]
    graph_texture: Handle<Image>,
}

#[derive(Clone, Copy, Debug, ShaderType)]
struct GraphUniforms {
    time: Vec4,
}

impl GraphUniforms {
    fn from_seconds(seconds: f32) -> Self {
        Self {
            time: Vec4::new(seconds, time_phase(seconds), 0.0, 0.0),
        }
    }
}

impl Material for GraphMaterial {
    fn fragment_shader() -> ShaderRef {
        GRAPH_SHADER_HANDLE.clone().into()
    }
}

#[derive(Component)]
struct PreviewObject;

#[derive(Component)]
struct PreviewMesh;

#[derive(Resource)]
pub struct PreviewGraphMaterial(pub Handle<GraphMaterial>);

#[derive(Resource)]
pub struct PreviewUniformBuffer(Handle<ShaderBuffer>);

#[derive(Resource, Clone, Debug, Default)]
pub struct PreviewTexture {
    pub handle: Handle<Image>,
    pub path: Option<String>,
}

fn setup_preview(
    mut commands: Commands,
    settings: Res<PreviewSettings>,
    source: Res<PreviewShaderSource>,
    uniforms: Res<PreviewUniformValues>,
    mut shaders: ResMut<Assets<Shader>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut graph_materials: ResMut<Assets<GraphMaterial>>,
    mut standard_materials: ResMut<Assets<StandardMaterial>>,
) {
    shaders
        .insert(
            GRAPH_SHADER_HANDLE.id(),
            Shader::from_wgsl(source.wgsl.clone(), "generated/preview.wgsl"),
        )
        .expect("UUID shader handles are always insertable");

    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.28, 0.36, 0.55),
        brightness: 180.0,
        ..default()
    });

    let preview_meshes = PreviewMeshes {
        cube: meshes.add(Cuboid::from_length(3.5)),
        sphere: meshes.add(Sphere::new(2.1).mesh().ico(6).expect("valid subdivision")),
        torus: meshes.add(Torus::new(1.1, 2.15)),
        capsule: meshes.add(Capsule3d::new(1.25, 2.5)),
    };

    let graph_texture = images.add(create_checker_texture());
    let user_uniforms = buffers.add(ShaderBuffer::from(uniforms.values.clone()));
    let graph_material = graph_materials.add(GraphMaterial {
        uniforms: GraphUniforms::from_seconds(0.0),
        user_uniforms: user_uniforms.clone(),
        graph_texture: graph_texture.clone(),
    });
    let outline_material = standard_materials.add(StandardMaterial {
        base_color: Color::srgb(0.08, 0.42, 0.72),
        emissive: LinearRgba::rgb(0.08, 0.42, 0.72),
        unlit: true,
        cull_mode: Some(Face::Front),
        ..default()
    });
    let preview_mesh = preview_meshes.get(settings.primitive);

    commands
        .spawn((
            PreviewObject,
            Transform::from_xyz(0.0, -0.35, 0.0).with_scale(Vec3::splat(0.78)),
            Visibility::default(),
        ))
        .with_children(|parent| {
            parent.spawn((
                Mesh3d(preview_mesh.clone()),
                MeshMaterial3d(graph_material.clone()),
                PreviewMesh,
            ));
            parent.spawn((
                Mesh3d(preview_mesh),
                MeshMaterial3d(outline_material),
                Transform::from_scale(Vec3::splat(1.025)),
                PreviewMesh,
            ));
        });
    commands.insert_resource(preview_meshes);
    commands.insert_resource(PreviewGraphMaterial(graph_material));
    commands.insert_resource(PreviewUniformBuffer(user_uniforms));
    commands.insert_resource(PreviewTexture {
        handle: graph_texture,
        path: None,
    });
}

fn apply_preview_primitive(
    settings: Res<PreviewSettings>,
    meshes: Res<PreviewMeshes>,
    mut preview_meshes: Query<&mut Mesh3d, With<PreviewMesh>>,
) {
    if settings.is_changed() {
        let mesh = meshes.get(settings.primitive);
        for mut preview_mesh in &mut preview_meshes {
            preview_mesh.0 = mesh.clone();
        }
    }
}

fn update_preview_shader(source: Res<PreviewShaderSource>, mut shaders: ResMut<Assets<Shader>>) {
    if !source.is_changed() {
        return;
    }

    shaders
        .insert(
            GRAPH_SHADER_HANDLE.id(),
            Shader::from_wgsl(source.wgsl.clone(), "generated/preview.wgsl"),
        )
        .expect("UUID shader handles are always insertable");
}

fn advance_preview_time(
    time: Res<Time>,
    material: Res<PreviewGraphMaterial>,
    mut materials: ResMut<Assets<GraphMaterial>>,
) {
    let Some(mut material) = materials.get_mut(&material.0) else {
        return;
    };
    material.uniforms = GraphUniforms::from_seconds(time.elapsed_secs());
}

fn apply_preview_texture(
    texture: Res<PreviewTexture>,
    material: Res<PreviewGraphMaterial>,
    mut materials: ResMut<Assets<GraphMaterial>>,
) {
    if !texture.is_changed() {
        return;
    }
    let Some(mut mat) = materials.get_mut(&material.0) else {
        return;
    };
    mat.graph_texture = texture.handle.clone();
}

fn apply_preview_uniforms(
    values: Res<PreviewUniformValues>,
    buffer: Res<PreviewUniformBuffer>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
) {
    if !values.is_changed() {
        return;
    }

    let Some(mut buffer) = buffers.get_mut(&buffer.0) else {
        return;
    };
    buffer.set_data(values.values.clone());
}

fn time_phase(seconds: f32) -> f32 {
    seconds.fract()
}

fn animate_preview(time: Res<Time>, mut preview: Single<&mut Transform, With<PreviewObject>>) {
    preview.rotate_y(0.16 * time.delta_secs());
    preview.rotate_x(0.045 * time.delta_secs());
}

fn create_checker_texture() -> Image {
    let width = 256u32;
    let height = 256u32;
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let is_white = ((x / 32) + (y / 32)) % 2 == 0;
            if is_white {
                data.extend_from_slice(&[255, 255, 255, 255]);
            } else {
                data.extend_from_slice(&[48, 48, 64, 255]);
            }
        }
    }
    Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
}
