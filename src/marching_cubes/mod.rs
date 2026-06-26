use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};

mod tables;

use tables::{CORNER_OFFSETS, EDGE_CONNECTIONS, EDGE_TABLE, TRIANGLE_TABLE};

pub struct MarchingCubesPlugin {
    pub settings: MarchingCubesSettings,
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct MarchingCubesSettings {
    /// Number of cubes, not sample points.
    pub chunk_size: UVec3,

    pub cell_size: f32,
    pub iso_level: f32,
}

impl Default for MarchingCubesPlugin {
    fn default() -> Self {
        Self {
            settings: MarchingCubesSettings {
                chunk_size: UVec3::splat(24),
                cell_size: 0.5,
                iso_level: 0.0,
            },
        }
    }
}

impl Plugin for MarchingCubesPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.settings)
            .add_systems(Startup, spawn_marching_cubes_chunk);
    }
}

#[derive(Component)]
pub struct MarchingCubesChunk {
    pub coordinate: IVec3,
}

#[derive(Default)]
struct MeshBuffers {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u32>,
}

fn spawn_marching_cubes_chunk(
    mut commands: Commands,
    settings: Res<MarchingCubesSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = generate_chunk_mesh(*settings);

    let mesh_handle = meshes.add(mesh);
    let material_handle = materials.add(StandardMaterial {
        base_color: Color::srgb(0.25, 0.65, 0.35),
        perceptual_roughness: 0.9,
        ..default()
    });

    // Center the generated chunk around the world origin.
    let extent = settings.chunk_size.as_vec3() * settings.cell_size;

    commands.spawn((
        MarchingCubesChunk {
            coordinate: IVec3::ZERO,
        },
        Mesh3d(mesh_handle),
        MeshMaterial3d(material_handle),
        Transform::from_translation(-extent * 0.5),
    ));
}

fn generate_chunk_mesh(settings: MarchingCubesSettings) -> Mesh {
    let sample_size = settings.chunk_size + UVec3::ONE;
    let densities = sample_density_field(sample_size, settings.cell_size);

    let mut buffers = MeshBuffers::default();

    for z in 0..settings.chunk_size.z {
        for y in 0..settings.chunk_size.y {
            for x in 0..settings.chunk_size.x {
                polygonize_cube(
                    UVec3::new(x, y, z),
                    sample_size,
                    &densities,
                    settings,
                    &mut buffers,
                );
            }
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, buffers.positions);

    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, buffers.normals);

    mesh.insert_indices(Indices::U32(buffers.indices));

    mesh
}

fn sample_density_field(sample_size: UVec3, cell_size: f32) -> Vec<f32> {
    let sample_count = sample_size.x as usize * sample_size.y as usize * sample_size.z as usize;

    let mut samples = Vec::with_capacity(sample_count);

    let world_extent = (sample_size - UVec3::ONE).as_vec3() * cell_size;

    let center = world_extent * 0.5;
    let radius = world_extent.min_element() * 0.35;

    for z in 0..sample_size.z {
        for y in 0..sample_size.y {
            for x in 0..sample_size.x {
                let position = UVec3::new(x, y, z).as_vec3() * cell_size;

                samples.push(sphere_density(position, center, radius));
            }
        }
    }

    samples
}

/// Signed-distance field for a sphere.
///
/// Negative: inside
/// Positive: outside
/// Zero: surface
fn sphere_density(position: Vec3, center: Vec3, radius: f32) -> f32 {
    position.distance(center) - radius
}

fn sample_index(position: UVec3, sample_size: UVec3) -> usize {
    position.x as usize
        + position.y as usize * sample_size.x as usize
        + position.z as usize * sample_size.x as usize * sample_size.y as usize
}

fn polygonize_cube(
    cube: UVec3,
    sample_size: UVec3,
    densities: &[f32],
    settings: MarchingCubesSettings,
    buffers: &mut MeshBuffers,
) {
    let mut corner_positions = [Vec3::ZERO; 8];
    let mut corner_values = [0.0_f32; 8];

    for corner_index in 0..8 {
        let offset = CORNER_OFFSETS[corner_index];
        let sample_position = cube + offset;

        corner_positions[corner_index] = sample_position.as_vec3() * settings.cell_size;

        corner_values[corner_index] = densities[sample_index(sample_position, sample_size)];
    }

    let mut cube_index = 0usize;

    for corner_index in 0..8 {
        if corner_values[corner_index] < settings.iso_level {
            cube_index |= 1 << corner_index;
        }
    }

    let edge_mask = EDGE_TABLE[cube_index];

    // Entirely inside or entirely outside.
    if edge_mask == 0 {
        return;
    }

    let mut edge_vertices = [Vec3::ZERO; 12];

    for edge_index in 0..12 {
        let edge_bit = 1_u16 << edge_index;

        if edge_mask & edge_bit == 0 {
            continue;
        }

        let [corner_a, corner_b] = EDGE_CONNECTIONS[edge_index];

        edge_vertices[edge_index] = interpolate_vertex(
            settings.iso_level,
            corner_positions[corner_a],
            corner_positions[corner_b],
            corner_values[corner_a],
            corner_values[corner_b],
        );
    }

    let triangle_edges = &TRIANGLE_TABLE[cube_index];

    for triangle in triangle_edges.chunks_exact(3) {
        if triangle[0] < 0 {
            break;
        }

        let a = edge_vertices[triangle[0] as usize];
        let b = edge_vertices[triangle[1] as usize];
        let c = edge_vertices[triangle[2] as usize];

        emit_triangle(a, b, c, buffers);
    }
}

fn interpolate_vertex(
    iso_level: f32,
    point_a: Vec3,
    point_b: Vec3,
    value_a: f32,
    value_b: f32,
) -> Vec3 {
    let difference = value_b - value_a;

    if difference.abs() < f32::EPSILON {
        return (point_a + point_b) * 0.5;
    }

    let t = ((iso_level - value_a) / difference).clamp(0.0, 1.0);

    point_a.lerp(point_b, t)
}

fn emit_triangle(a: Vec3, b: Vec3, c: Vec3, buffers: &mut MeshBuffers) {
    let normal = (b - a).cross(c - a).normalize_or_zero();

    // Ignore degenerate triangles.
    if normal == Vec3::ZERO {
        return;
    }

    let first_index = buffers.positions.len() as u32;

    buffers
        .positions
        .extend([a.to_array(), b.to_array(), c.to_array()]);

    buffers
        .normals
        .extend([normal.to_array(), normal.to_array(), normal.to_array()]);

    buffers
        .indices
        .extend([first_index, first_index + 1, first_index + 2]);
}
