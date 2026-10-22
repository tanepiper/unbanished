//! What an entity window shows: citizens, workplaces, construction, produce and residents.

use super::*;

/// The values CitizenUI (`FUN_140065440`) shows for citizen `i`.
pub(crate) fn citizen_info(
    town: &StartingTown,
    i: usize,
) -> Option<crate::ui::building_ui::CitizenInfo> {
    let c = town.citizens.get(i)?;
    let s = town.agents.states.get(i)?;
    let ai = town.agents.ais.get(i)?;
    let rules = town.agents.rules();
    let child = c.age < town.rules.adult_age || c.school.is_some();
    // Inferred from the resource names: ProfessionList `+0x40` child, `+0x48` student; adults
    // show their profession (`+0x1f0`).
    let key = if !child {
        "laborer"
    } else if c.school.is_some() {
        "student"
    } else {
        "child"
    };
    let assigned = (!child)
        .then(|| town.construction.professions.list.get(s.profession))
        .flatten()
        .map(|p| (p.string_table.clone(), p.name.clone()));
    let profession = assigned.or_else(|| {
        town.profession(key)
            .map(|p| (p.string_table.clone(), p.name.clone()))
    });
    let tool = if s.tool_uses < 1 {
        0
    } else {
        i32::from(s.tool_status != rules.tool_grade) + 1
    };
    // Activity 9 (working) shows 16 ("picking up resources") when the job's work
    // (`FUN_1400fef30`, by id `+0x30`) is for another profession (slot `+0x50`).
    let mut activity = ai.current as i32;
    if activity == 9 && s.job >= 0 {
        if let Some(work) = town.construction.jobs.works.get(&s.job) {
            if work.profession != s.profession {
                activity = 16;
            }
        }
    }
    Some(crate::ui::building_ui::CitizenInfo {
        name: c.name.clone(),
        male: c.male,
        age: c.age,
        educated: c.education >= town.rules.education,
        clothing: s.clothing,
        tool,
        profession,
        health: s.health,
        health_max: rules.health_max,
        happiness: s.happiness,
        happiness_max: rules.happiness,
        activity,
        has_work: s.workplace.is_some() || s.school.is_some(),
        has_home: s.home.is_some(),
    })
}

/// What WorkPlaceUI (`FUN_14008c550`) reads for building `p`; `None` without a profession.
pub(super) fn workplace_info(
    town: &StartingTown,
    p: usize,
) -> Option<crate::ui::building_ui::WorkPlaceInfo> {
    let c = &town.construction;
    let wp = c.workforce.workplaces.get(&p)?;
    let profession = wp.profession?;
    let desc = c.professions.list.get(profession)?;
    let (mut holders, mut laborers) = (0, 0);
    for (citizen, state) in town.citizens.iter().zip(&town.agents.states) {
        if citizen.alive && citizen.is_adult(&town.rules) && citizen.school.is_none() {
            holders += i32::from(state.profession == profession);
            laborers += i32::from(state.profession == c.professions.laborer);
        }
    }
    let site = c.sites.get(&p);
    let template = site.and_then(|s| c.templates.get(s.template));
    // Every site keeps its BuildComponent until it is built, and is the builder's until then.
    let build_points = (profession == c.professions.builder && site.is_some())
        .then(|| town.templates[town.placed[p].template].build_points.len() as i32);
    Some(crate::ui::building_ui::WorkPlaceInfo {
        entity: p,
        sprite: (desc.sprite_sheet.clone(), desc.name.clone()),
        workers: wp.workers.clone(),
        max: wp.max(),
        holders,
        laborers,
        adjustable: template.is_some_and(|t| t.adjustable),
        min_jobs: template.map_or(0, |t| t.min_jobs),
        max_jobs: template.map_or(0, |t| t.max_jobs),
        build_points,
    })
}

/// What BuildUI (`FUN_1400b6f20`) reads for building `p`; `None` unless it is a site.
pub(super) fn build_info(
    town: &StartingTown,
    p: usize,
) -> Option<crate::ui::building_ui::BuildInfo> {
    use unbanished_game::work::construction::Controller as C;
    let c = &town.construction;
    let site = c.sites.get(&p)?;
    let t = &c.templates[site.template];
    let storage = town.storages.get(site.storage);
    let build = t.controllers.contains(&C::Build).then(|| {
        let materials = t
            .materials
            .iter()
            .map(|&(count, m)| {
                // The first entry of that material.
                let have = storage
                    .and_then(|s| s.items.iter().find(|i| i.material == m))
                    .map_or(0, |i| i.count - i.reserved);
                (m, count * site.stages, have)
            })
            .collect();
        crate::ui::building_ui::BuildProgress {
            materials,
            work: t.work * site.stages,
            progress: site.progress,
        }
    });
    let has_clear = t.controllers.contains(&C::ClearArea);
    Some(crate::ui::building_ui::BuildInfo {
        entity: p,
        build,
        clear_total: if has_clear { site.clear_total } else { 0 },
        clear_remaining: if has_clear {
            unbanished_game::town::clear_remaining(
                &town.cells.objects,
                town.cells.size as i32,
                &town.placed[p],
                p,
            )
        } else {
            0
        },
        enabled: c.workforce.workplaces.get(&p).map(|w| w.enabled),
    })
}

/// DestroyUI's view of the building's DestroyComponent (`FUN_140066460`).
pub(super) fn destroy_info(
    town: &StartingTown,
    p: usize,
) -> Option<crate::ui::building_ui::DestroyInfo> {
    let site = town.construction.sites.get(&p)?;
    let d = site.destroy.as_ref()?;
    let state = if site.machine.pending < 0 {
        site.machine.state
    } else {
        site.machine.pending
    };
    let own = town.placed[p].template;
    let replace = town.construction.templates[site.template]
        .destroy
        .and_then(|t| t.replace);
    Some(crate::ui::building_ui::DestroyInfo {
        entity: p,
        is_damaged: matches!(state, 6 | 7),
        state,
        damaged: d.damaged,
        count: d.count,
        target: d.target,
        rebuild: d.rebuild == Some(own),
        upgrade: d.rebuild != Some(own) && d.rebuild != replace,
    })
}

/// ConsumeProduceUI's view: the template, the selector, productions made and the lifetime.
pub(super) fn produce_info(
    town: &StartingTown,
    p: usize,
) -> Option<(usize, String, i32, i32, i32)> {
    let site = town.construction.sites.get(&p)?;
    let state = site.produce.as_ref()?;
    let lifetime = town.construction.templates[site.template]
        .produce
        .as_ref()
        .map_or(-1, |t| t.lifetime);
    Some((
        p,
        town.templates[town.placed[p].template].reference.clone(),
        state.selector,
        state.produced,
        lifetime,
    ))
}

/// ImmigrationUI's view: nomads waiting (state 0x11) and how many came (`+0x60`).
pub(super) fn immigration_info(town: &StartingTown, p: usize) -> Option<(bool, i32)> {
    let site = town.construction.sites.get(&p)?;
    let im = site.immigration.as_ref()?;
    let m = &site.machine;
    let state = if m.pending < 0 { m.state } else { m.pending };
    Some((state == 0x11, im.arrived))
}

/// ResidenceUI's residents, family by family, each as CitizenUI reads it.
pub(super) fn residents(
    town: &StartingTown,
    p: usize,
) -> Option<Vec<(usize, crate::ui::building_ui::CitizenInfo)>> {
    let home = town.construction.homes.get(&p)?;
    Some(
        home.families
            .iter()
            .flatten()
            .filter_map(|&c| citizen_info(town, c).map(|info| (c, info)))
            .collect(),
    )
}
