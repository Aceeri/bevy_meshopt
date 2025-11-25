use std::{error::Error, fmt::Display};

use bevy::mesh::{Indices, Mesh, PrimitiveTopology, VertexAttributeValues};

pub use meshopt::SimplifyOptions;

pub trait MeshExt {
    /// Assert that the mesh has u32 indices, replaces if it is u16.
    fn assert_indices_u32(&mut self);
    /// [`meshopt::simplify`]
    fn simplify(&self, params: &SimplifyParams) -> Result<(Vec<u32>, f32), OptError>;
    /// [`meshopt::simplify`]
    fn simplify_in_place(&mut self, params: &SimplifyParams) -> Result<f32, OptError>;
    /// [`meshopt::optimize_vertex_fetch`]
    fn optimize_vertex_fetch_in_place(&mut self) -> Result<(), OptError>;
    /// [`meshopt::optimize_overdraw`]
    fn optimize_overdraw_in_place(&mut self, threshold: f32) -> Result<(), OptError>;
    /// [`meshopt::optimize_vertex_cache`]
    fn optimize_vertex_cache_in_place(&mut self) -> Result<(), OptError>;
}

#[derive(Debug, Copy, Clone)]
pub enum TargetIndices {
    Count(usize),
    Multiplier(f32),
}

impl Default for TargetIndices {
    fn default() -> Self {
        TargetIndices::Multiplier(0.5)
    }
}

impl TargetIndices {
    pub fn count(&self, current_count: usize) -> usize {
        let count = match self {
            TargetIndices::Count(count) => *count,
            TargetIndices::Multiplier(multiplier) => {
                ((current_count as f32 * multiplier) as usize / 3 * 3).max(3)
            }
        };

        count.min(current_count)
    }
}

#[derive(Debug, Copy, Clone)]
pub struct SimplifyParams<'a> {
    /// Maximum error allowed during simplification. This will be somewhat ignored if using sloppy mode.
    pub max_error: f32,
    /// Target index count for simplification.
    pub target_index_count: TargetIndices,
    pub options: SimplifyOptions,
    /// Note: Sloppy will ignore all `SimplifyOptions`.
    pub sloppy: bool,
    /// Lock specific vertices in place during simplification.
    pub vertex_locks: Option<&'a [bool]>,
}

impl Default for SimplifyParams<'_> {
    fn default() -> Self {
        SimplifyParams {
            max_error: 0.01,
            target_index_count: TargetIndices::default(),
            options: SimplifyOptions::None,
            sloppy: false,
            vertex_locks: None,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum OptError {
    MissingIndices,
    UnsupportedIndexFormat,
    MissingPositions,
    UnsupportedPrimitiveTopology(PrimitiveTopology),
    InvalidIndexCount(usize),
}

impl Display for OptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OptError::MissingIndices => write!(f, "Missing indices"),
            OptError::UnsupportedIndexFormat => write!(f, "Unsupported index format"),
            OptError::MissingPositions => write!(f, "Missing positions"),
            OptError::UnsupportedPrimitiveTopology(topology) => write!(
                f,
                "Unsupported topology: {:?}, bevy_meshopt currently only works with `TriangleList` topology,",
                topology
            ),
            OptError::InvalidIndexCount(count) => write!(f, "Invalid index count: {}", count),
        }
    }
}

impl Error for OptError {}

fn assert_u32_indices(indices: Option<&mut Indices>) {
    let new_indices = match indices {
        Some(Indices::U16(u16_indices)) => Some(Indices::U32(
            u16_indices.into_iter().map(|i| *i as u32).collect(),
        )),
        Some(_) => None,
        None => None,
    };

    if let Some(new_indices) = new_indices {
        if let Some(indices) = indices {
            *indices = new_indices;
        }
    }
}

fn mesh_indices(mesh: &Mesh) -> Result<&Vec<u32>, OptError> {
    let indices = match mesh.indices() {
        Some(Indices::U32(indices)) => indices,
        Some(_) => return Err(OptError::UnsupportedIndexFormat),
        None => return Err(OptError::MissingIndices),
    };

    if indices.len() % 3 != 0 || indices.len() == 0 {
        return Err(OptError::InvalidIndexCount(indices.len()));
    }

    return Ok(indices);
}

fn mesh_indices_mut(mesh: &mut Mesh) -> Result<&mut Vec<u32>, OptError> {
    let indices = match mesh.indices_mut() {
        Some(Indices::U32(indices)) => indices,
        Some(_) => return Err(OptError::UnsupportedIndexFormat),
        None => return Err(OptError::MissingIndices),
    };

    if indices.len() % 3 != 0 || indices.len() == 0 {
        return Err(OptError::InvalidIndexCount(indices.len()));
    }

    return Ok(indices);
}

fn take_mesh_indices_mut(mesh: &mut Mesh) -> Result<Vec<u32>, OptError> {
    let indices = match mesh.remove_indices() {
        Some(Indices::U32(indices)) => indices,
        Some(indices) => {
            mesh.insert_indices(indices);
            return Err(OptError::UnsupportedIndexFormat);
        }
        None => return Err(OptError::MissingIndices),
    };

    if indices.len() % 3 != 0 || indices.len() == 0 {
        return Err(OptError::InvalidIndexCount(indices.len()));
    }

    return Ok(indices);
}

fn mesh_positions(mesh: &Mesh) -> Result<&Vec<[f32; 3]>, OptError> {
    let PrimitiveTopology::TriangleList = mesh.primitive_topology() else {
        return Err(OptError::UnsupportedPrimitiveTopology(
            mesh.primitive_topology(),
        ));
    };

    // TODO: support other vertex formats
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return Err(OptError::MissingPositions);
    };

    Ok(positions)
}

impl MeshExt for Mesh {
    fn assert_indices_u32(&mut self) {
        assert_u32_indices(self.indices_mut());
    }

    fn simplify_in_place(&mut self, params: &SimplifyParams) -> Result<f32, OptError> {
        let (new_indices, error) = self.simplify(params)?;
        self.insert_indices(Indices::U32(new_indices));
        Ok(error)
    }

    fn simplify(&self, params: &SimplifyParams) -> Result<(Vec<u32>, f32), OptError> {
        let indices = mesh_indices(self)?;
        let positions = mesh_positions(self)?;

        let target_index_count = params.target_index_count.count(indices.len());

        let mut result_error = 0.0;
        let new_indices = if params.sloppy {
            if let Some(locks) = params.vertex_locks {
                meshopt::simplify_sloppy_with_locks_decoder(
                    indices,
                    &positions,
                    locks,
                    target_index_count,
                    params.max_error,
                    Some(&mut result_error),
                )
            } else {
                meshopt::simplify_sloppy_decoder(
                    indices,
                    positions.as_slice(),
                    target_index_count,
                    params.max_error,
                    Some(&mut result_error),
                )
            }
        } else {
            if let Some(locks) = params.vertex_locks {
                meshopt::simplify_with_locks_decoder(
                    indices,
                    positions.as_slice(),
                    locks,
                    target_index_count,
                    params.max_error,
                    params.options,
                    Some(&mut result_error),
                )
            } else {
                meshopt::simplify_decoder(
                    indices,
                    positions.as_slice(),
                    target_index_count,
                    params.max_error,
                    params.options,
                    Some(&mut result_error),
                )
            }
        };

        Ok((new_indices, result_error))
    }

    fn optimize_vertex_fetch_in_place(&mut self) -> Result<(), OptError> {
        let mut indices_mut = take_mesh_indices_mut(self)?;
        let Some(VertexAttributeValues::Float32x3(positions)) =
            self.attribute_mut(Mesh::ATTRIBUTE_POSITION)
        else {
            return Err(OptError::MissingPositions);
        };

        meshopt::optimize_vertex_fetch_in_place(&mut indices_mut, positions);
        self.insert_indices(Indices::U32(indices_mut));
        Ok(())
    }

    fn optimize_overdraw_in_place(&mut self, threshold: f32) -> Result<(), OptError> {
        let mut indices_mut = take_mesh_indices_mut(self)?;
        let positions = mesh_positions(self)?;
        meshopt::optimize_overdraw_in_place_decoder(&mut indices_mut, positions, threshold);
        self.insert_indices(Indices::U32(indices_mut));
        Ok(())
    }

    fn optimize_vertex_cache_in_place(&mut self) -> Result<(), OptError> {
        let positions_len = mesh_positions(self)?.len();
        let mut indices_mut = mesh_indices_mut(self)?;
        meshopt::optimize_vertex_cache_in_place(&mut indices_mut, positions_len);
        Ok(())
    }
}
