//! Which animals a template describes: wildlife, livestock and their kinds.

use super::*;

/// An animal template's AnimalDescription, CharacterDescription, the raw materials it leaves
/// and its animation groups.
pub(super) fn animal_kind(
    archive: &Archive,
    reference: &str,
    template: usize,
    templates: &mut Vec<Template>,
    wildlife: &mut banished_game::nature::wildlife::Wildlife,
) {
    let refs = description_refs(archive, reference);
    let find = |k: &str| {
        refs.iter()
            .find(|(key, _)| key == k)
            .and_then(|(_, r)| read(archive, &normalise(r)))
    };
    let ad = find("animal").and_then(|d| banished_formats::game::animal_description(&d).ok());
    let cd = find("character").and_then(|d| banished_formats::game::character_description(&d).ok());
    let (Some(ad), Some(cd)) = (ad, cd) else {
        return;
    };
    let materials = ad
        .materials
        .iter()
        .flatten()
        .filter_map(|m| load_template(archive, m, templates))
        .collect();
    for (v, g) in templates[template].animations.iter().enumerate() {
        if let Some(g) = g {
            wildlife.groups.insert((template, v), g.clone());
        }
    }
    wildlife.animal_kinds.insert(
        template,
        banished_game::nature::wildlife::AnimalKind {
            desc: ad,
            materials,
            character: cd,
        },
    );
}

/// A livestock template: its animal and its LivestockDescription (the product's pile
/// template resolved).
pub(super) fn livestock_kind(
    archive: &Archive,
    reference: &str,
    templates: &mut Vec<Template>,
    wildlife: &mut banished_game::nature::wildlife::Wildlife,
) -> Option<usize> {
    let t = load_template(archive, reference, templates)?;
    if wildlife.livestock_kinds.contains_key(&t) {
        return Some(t);
    }
    animal_kind(archive, reference, t, templates, wildlife);
    let refs = description_refs(archive, reference);
    let d = refs
        .iter()
        .find(|(key, _)| key == "livestock")
        .and_then(|(_, r)| read(archive, &normalise(r)))
        .and_then(|d| banished_formats::game::livestock_description(&d).ok())?;
    let product = d
        .product
        .as_deref()
        .and_then(|p| load_template(archive, p, templates));
    wildlife.livestock_kinds.insert(
        t,
        banished_game::nature::wildlife::LivestockKind {
            product,
            interval: d.interval,
            group: d.group,
        },
    );
    Some(t)
}

/// `FUN_1400a5eb0`'s templates: each population's, its herd's and its animal's, with their
/// descriptions. Returns the population templates.
pub(super) fn wildlife_kinds(
    archive: &Archive,
    populations: &[String],
    templates: &mut Vec<Template>,
    wildlife: &mut banished_game::nature::wildlife::Wildlife,
) -> Vec<usize> {
    use banished_game::nature::wildlife::{HerdKind, PopulationKind};
    let find = |refs: &[(String, String)], k: &str| {
        refs.iter()
            .find(|(key, _)| key == k)
            .and_then(|(_, r)| read(archive, &normalise(r)))
    };
    let mut out = Vec::new();
    for r in populations {
        let Some(pt) = load_template(archive, r, templates) else {
            continue;
        };
        let refs = description_refs(archive, r);
        let Some(pd) = find(&refs, "population")
            .and_then(|d| banished_formats::game::population_description(&d).ok())
        else {
            continue;
        };
        let herd = pd
            .herd
            .as_deref()
            .and_then(|h| load_template(archive, h, templates).map(|t| (h.to_string(), t)));
        if let Some((href, ht)) = &herd {
            let hrefs = description_refs(archive, href);
            let hd = find(&hrefs, "herd")
                .and_then(|d| banished_formats::game::herd_description(&d).ok());
            if let Some(hd) = hd {
                let animal = hd
                    .animal
                    .as_deref()
                    .and_then(|a| load_template(archive, a, templates).map(|t| (a.to_string(), t)));
                if let Some((aref, at)) = animal {
                    animal_kind(archive, &aref, at, templates, wildlife);
                    wildlife.herd_kinds.insert(
                        *ht,
                        HerdKind {
                            desc: hd,
                            animal: at,
                        },
                    );
                }
            }
        }
        wildlife.population_kinds.insert(
            pt,
            PopulationKind {
                desc: pd,
                herd: herd.map(|(_, t)| t),
            },
        );
        out.push(pt);
    }
    out
}
