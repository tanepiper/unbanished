//! Each model's material list and picking flag, from the mesh it shows.

use std::collections::HashMap;

use unbanished_formats::pkg::Archive;

use super::*;

/// Every ModelComponent's materials and picking flag follow its mesh: with a mesh, the mesh's
/// first material set and picking on; without one (mesh −1), both as they were (none and off
/// for a new model).
/// `SetMesh` sets them, and `MeshInstance::Serialize` writes the materials (Runtime
/// `0x180101f10`); loading resizes the list to the saved count, so an empty list would leave
/// the model without its materials. *Observed* on every model of `Save3` and `Save83`
/// (17,191 models).
///
/// Original: `FUN_14012a040`.
pub(super) fn finish_models(archive: &Archive, g: &mut GameState, refs: &mut Refs) {
    let mut cache: HashMap<(String, i16, i16), Option<Vec<String>>> = HashMap::new();
    let ids: Vec<ObjId> = g
        .objects
        .iter()
        .filter(|(_, o)| o.type_name == "ModelComponent")
        .map(|(&id, _)| id)
        .collect();
    for id in ids {
        let f = &g.objects[&id].fields;
        let description = match f.at("base.description") {
            Some(Value::Ext(r)) => g.references.get(r).cloned(),
            Some(Value::I32(r)) => g.references.get(&(*r as u32)).cloned(),
            _ => None,
        };
        let Some(description) = description else {
            continue;
        };
        let variant = match f.at("model.variant") {
            Some(Value::I16(v)) => *v,
            _ => 0,
        };
        let mesh = match f.at("model.mesh") {
            Some(Value::I16(v)) => *v,
            _ => -1,
        };
        // Without a mesh the list stays as it is: a new model's is empty, and a site whose
        // mesh was cleared keeps its last materials (Save4).
        if mesh < 0 {
            continue;
        }
        let materials = cache
            .entry((description.clone(), variant, mesh))
            .or_insert_with(|| mesh_materials(archive, &description, variant, mesh))
            .clone();
        // A model whose mesh cannot be read keeps what it has.
        let Some(materials) = materials else {
            continue;
        };
        let list = Value::List(
            materials
                .iter()
                .map(|m| {
                    unbanished_formats::savegame::Fields(vec![(
                        "material",
                        Value::Ext(refs.id(g, m)),
                    )])
                })
                .collect(),
        );
        let f = &mut g.objects.get_mut(&id).unwrap().fields;
        f.set("model.materials", list);
        f.set_bool("model.picking", true);
    }
}

/// The first material set of the mesh that `variant` and `mesh` of a ModelDescription name.
fn mesh_materials(
    archive: &Archive,
    description: &str,
    variant: i16,
    mesh: i16,
) -> Option<Vec<String>> {
    let data = archive.read_ref(&description.replace('\\', "/")).ok()?;
    let desc = unbanished_formats::game::model_description(&data).ok()?;
    let path = desc
        .variants
        .get(variant as usize)?
        .meshes
        .get(mesh as usize)?
        .clone()?;
    let data = archive.read_ref(&path.replace('\\', "/")).ok()?;
    let mesh = unbanished_formats::types::graphics_mesh(&data).ok()?;
    let set = mesh.material_instances.first()?;
    Some(set.iter().map(|m| m.clone().unwrap_or_default()).collect())
}
