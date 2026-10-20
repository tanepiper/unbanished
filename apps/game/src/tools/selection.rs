//! EditTool selection (140102500/140102880) and PickingDescription (14012e420).
//! All caches and selected references are presentation state; no game Random is accepted.
use crate::start::StartingTown;
use glam::{Mat4, Vec3};
use std::{collections::BTreeMap, sync::Arc};
use unbanished_formats::{
    picking::{self, PickingMesh},
    pkg::Archive,
    skeleton::Transform,
};

pub use unbanished_sim::tools::entity::Entity;

#[derive(Default)]
pub struct Selection {
    /// The selected entity; a MultiObject's parts are highlighted with it
    /// (`Highlight::geometry`, `FUN_140101eb0`).
    pub selected: Option<Entity>,
    descriptions: BTreeMap<String, Option<picking::PickingDescription>>,
    meshes: BTreeMap<String, Option<Arc<PickingMesh>>>,
}

impl Selection {
    fn mesh(
        &mut self,
        archive: &Archive,
        town: &StartingTown,
        entity: Entity,
    ) -> Option<Arc<PickingMesh>> {
        let key = entity.template(town).split(':').next()?.replace('\\', "/");
        let d = self
            .descriptions
            .entry(key.clone())
            .or_insert_with(|| {
                archive
                    .read_ref(&format!("{key}:picking"))
                    .ok()
                    .and_then(|b| picking::picking_description(&b).ok())
            })
            .as_ref()?;
        let (variant, stage) = entity.variant_stage(town);
        let reference = d.meshes.get(variant)?.get(stage)?.as_deref()?.to_owned();
        self.meshes
            .entry(reference.clone())
            .or_insert_with(|| {
                archive
                    .read_ref(&reference)
                    .ok()
                    .and_then(|b| picking::picking_mesh(&b).ok())
                    .map(Arc::new)
            })
            .clone()
    }

    /// Runtime AddMesh (180127b70): hierarchy is the picking mesh's own bind hierarchy.
    /// Picking transforms are rigid; model animation and visual scale are not applied here.
    fn transforms(mesh: &PickingMesh, root: Transform) -> Vec<Transform> {
        let mut world: Vec<Transform> = Vec::with_capacity(mesh.meshes.len());
        for (i, m) in mesh.meshes.iter().enumerate() {
            let t = if i == 0 {
                root
            } else {
                let [x, y, z, w] = m.rotation;
                world.get(m.node as usize).unwrap_or(&root).mul(&Transform {
                    rotation: [-x, -y, -z, w],
                    translation: m.translation,
                })
            };
            world.push(t);
        }
        world
    }

    /// Whether the segment from `a` to `b` passes through a placed object's picking mesh, cast
    /// from both ends (only front faces count, so an end inside the object still meets its
    /// outside). None when the object has no picking mesh. For captures' cinematic camera
    /// ([`crate::view::cinematic`]); not in the original.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn blocks(
        &mut self,
        archive: &Archive,
        town: &StartingTown,
        placed: usize,
        a: Vec3,
        b: Vec3,
    ) -> Option<bool> {
        let entity = Entity::Placed(placed);
        let mesh = self.mesh(archive, town, entity)?;
        let length = (b - a).length();
        if length <= 0.0 {
            return Some(false);
        }
        let world = Self::transforms(&mesh, entity.picking_transform(town));
        for (m, t) in mesh.meshes.iter().zip(world) {
            // Picking transforms are rigid: directions keep their length.
            let inverse = Mat4::from_cols_array(&t.to_matrix()).inverse();
            for (from, to) in [(a, b), (b, a)] {
                let o = inverse.transform_point3(from).to_array();
                let d = inverse.transform_vector3((to - from) / length).to_array();
                let hit = m.triangles.iter().any(|tri| {
                    let vertices = tri.vertices.map(|i| m.vertices[i as usize]);
                    unbanished_game::picking::triangle(o, d, vertices, length).is_some()
                });
                if hit {
                    return Some(true);
                }
            }
        }
        Some(false)
    }

    /// FUN_140102500: terrain entity first, then any nearest mesh hit replaces it.
    pub fn cast(
        &mut self,
        archive: &Archive,
        town: &StartingTown,
        origin: Vec3,
        direction: Vec3,
        maximum: f32,
        fallback: Option<Entity>,
    ) -> Option<Entity> {
        let mut result = fallback;
        let mut nearest = maximum;
        // A bridge piece's PickingComponent user data is the bridge (`FUN_1400ff1a0`, the only
        // caller of Picking::Scene::SetMeshUserData; the pieces are the parts with a floor).
        let parents: BTreeMap<usize, usize> = town
            .construction
            .sites
            .iter()
            .flat_map(|(&e, s)| s.parts.iter().map(move |&p| (p, e)))
            .filter(|&(p, _)| town.templates[town.placed[p].template].floor.is_some())
            .collect();
        let owner = |e: Entity| match e {
            Entity::Placed(i) => parents.get(&i).map_or(e, |&p| Entity::Placed(p)),
            other => other,
        };
        // Deviation: the spatial grid's linked candidate order is creation order here. Only
        // exactly coincident equal-distance hits can differ; the inclusive replacement is exact.
        let entities = town
            .placed
            .iter()
            .enumerate()
            .filter(|(_, p)| p.alive)
            .map(|(i, _)| Entity::Placed(i))
            .chain(
                (0..town.citizens.len())
                    .filter(|&i| town.citizens[i].alive)
                    .map(Entity::Citizen),
            );
        for entity in entities {
            let Some(mesh) = self.mesh(archive, town, entity) else {
                continue;
            };
            let world = Self::transforms(&mesh, entity.picking_transform(town));
            for (m, t) in mesh.meshes.iter().zip(world) {
                let matrix = Mat4::from_cols_array(&t.to_matrix()).inverse();
                let o = matrix.transform_point3(origin).to_array();
                let d = matrix.transform_vector3(direction).to_array();
                for tri in &m.triangles {
                    let vertices = tri.vertices.map(|i| m.vertices[i as usize]);
                    if let Some(distance) =
                        unbanished_game::picking::triangle(o, d, vertices, nearest)
                    {
                        nearest = distance;
                        result = Some(owner(entity));
                    }
                }
            }
        }
        result
    }

    pub fn bounds(
        &mut self,
        archive: &Archive,
        town: &StartingTown,
        entity: Entity,
    ) -> Option<[Vec3; 2]> {
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        if let Some(mesh) = self.mesh(archive, town, entity) {
            for (m, t) in mesh
                .meshes
                .iter()
                .zip(Self::transforms(&mesh, entity.picking_transform(town)))
            {
                for &v in &m.vertices {
                    let v = Vec3::from(t.apply(v));
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
            }
            if lo.x <= hi.x {
                return Some([lo, hi]);
            }
        }
        if let Entity::Placed(i) = entity {
            let p = &town.placed[i];
            let half = Vec3::new(p.footprint.0 as f32 * 0.5, p.footprint.1 as f32 * 0.5, 0.);
            return Some([Vec3::from(p.position) - half, Vec3::from(p.position) + half]);
        }
        None
    }
}
