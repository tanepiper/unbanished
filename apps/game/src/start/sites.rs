//! Building sites: their templates, adding a site and loading nomads.

use super::*;

impl StartingTown {
    /// The construction descriptions of a template (`None` without a BuildDescription).
    pub fn site_template(&mut self, archive: &Archive, template: usize) -> Option<usize> {
        let reference = self.templates[template].reference.clone();
        if let Some(i) = self
            .construction
            .templates
            .iter()
            .position(|t| t.reference == reference)
        {
            return Some(i);
        }
        let desc = component_description(&read(archive, &reference)?).ok()?;
        let mut t = SiteTemplate {
            reference: reference.clone(),
            ..Default::default()
        };
        let mut has_build = false;
        for d in desc.descriptions.iter().flatten() {
            let Some(data) = read(archive, d) else {
                continue;
            };
            match d.rsplit(':').next().unwrap_or("") {
                "cleararea" => {
                    t.controllers.push(Controller::ClearArea);
                    t.clear = unbanished_formats::game::clear_area_description(&data).ok();
                }
                "workplace" => {
                    t.controllers.push(Controller::WorkPlace);
                    t.profession = unbanished_formats::game::workplace_description(&data)
                        .ok()
                        .and_then(|w| w.profession)
                        .and_then(|p| {
                            let p = p.replace('/', "\\");
                            self.construction
                                .professions
                                .list
                                .iter()
                                .position(|x| x.reference.eq_ignore_ascii_case(&p))
                        });
                }
                "build" => {
                    has_build = true;
                    t.controllers.push(Controller::Build);
                    let b = unbanished_formats::game::build_description(&data).ok()?;
                    t.work = b.work;
                    for (n, m) in b.materials {
                        let m = m?;
                        let pile = self.load_template(archive, &m);
                        let m = material_index(archive, &mut self.materials, &m)?;
                        t.materials.push((n, m));
                        t.material_templates.push(pile);
                    }
                }
                "well" => t.controllers.push(Controller::Well),
                "trade" => {
                    if let Ok(d) = unbanished_formats::game::trade_description(&data) {
                        let boat = d.boat.as_deref().and_then(|r| self.load_boat(archive, r));
                        if let Some(boat) = boat {
                            self.load_trade_goods(archive);
                            t.trade = Some(unbanished_game::commerce::trade::TradeTemplate {
                                boat,
                                merchants: d.merchants.clone(),
                                rows: d.rows.clone(),
                                categories: d.categories.iter().map(|c| c.0).collect(),
                                labels: d
                                    .categories
                                    .iter()
                                    .map(|c| (c.1.clone(), c.2.clone()))
                                    .collect(),
                                sprites: d.sprites.clone(),
                                strings: d.strings.clone(),
                            });
                            t.trade_risk = Some((d.value_68, d.value_6c));
                            t.controllers.push(Controller::Trade);
                        }
                    }
                }
                "immigration" => {
                    if let Ok(d) = unbanished_formats::game::immigration_description(&data) {
                        let nomad = d.nomad.as_deref().and_then(|r| self.load_nomad(archive, r));
                        let required = d
                            .required
                            .iter()
                            .flatten()
                            .filter_map(|r| self.load_template(archive, r))
                            .collect();
                        if let Some(nomad) = nomad {
                            t.immigration =
                                Some(unbanished_game::people::immigration::ImmigrationTemplate {
                                    nomad,
                                    population: d.population,
                                    required,
                                    share: d.share,
                                    wait: d.wait,
                                    months: d.months,
                                    leave: 7,
                                    margin: 10,
                                    risk: d.risk,
                                    risk_months: d.risk_months,
                                });
                            t.controllers.push(Controller::Immigration);
                        }
                    }
                }
                name if name.starts_with("fire") => {
                    t.fire = unbanished_formats::game::fire_description(&data).ok();
                }
                "multiobject" => t.controllers.push(Controller::MultiObject),
                "destroy" => {
                    t.controllers.push(Controller::Destroy);
                    let d = unbanished_formats::game::destroy_description(&data).ok()?;
                    let replace = d
                        .replace
                        .as_deref()
                        .and_then(|r| self.load_template(archive, r));
                    // A replacement that is a site itself has its construction template ready
                    // for when it is placed.
                    if let Some(r) = replace {
                        if r != template {
                            self.site_template(archive, r);
                        }
                    }
                    t.destroy = Some(unbanished_game::work::destroy::DestroyTemplate {
                        replace,
                        material: d.material,
                        decal: d.decal,
                        min_mesh: d.min_mesh,
                    });
                }
                "attendance" => {
                    t.controllers.push(Controller::Attendance);
                    t.attendance = unbanished_formats::game::attendance_description(&data).ok();
                }
                "healer" => t.controllers.push(Controller::Healer),
                "residence" => {
                    t.controllers.push(Controller::Residence);
                    t.residence = unbanished_formats::game::residence_description(&data).ok();
                }
                "clearresource" => t.controllers.push(Controller::ClearResource),
                "herbalist" => {
                    let Ok(d) = unbanished_formats::game::herbalist_description(&data) else {
                        continue;
                    };
                    t.controllers.push(Controller::ResourceProduce);
                    let mut resources = Vec::new();
                    for r in d.resources.iter().flatten() {
                        resources.push(self.load_template(archive, r)?);
                    }
                    t.herbalist = Some(unbanished_game::production::herbalist::HerbalistTemplate {
                        resources,
                        group: d.group,
                    });
                }
                "radius" => {
                    t.radius = unbanished_formats::game::radius_description(&data)
                        .ok()
                        .map(|(r, _)| r);
                }
                "happiness" => {
                    if let Ok(d) = unbanished_formats::game::happiness_description(&data) {
                        t.controllers.push(Controller::Happiness);
                        t.happiness = Some(d);
                    }
                }
                "cropfield" => {
                    if let Ok((tend, group)) =
                        unbanished_formats::game::crop_field_description(&data)
                    {
                        t.controllers.push(Controller::CropField);
                        t.crop_field = Some(unbanished_game::production::farm::CropFieldTemplate {
                            tend,
                            group,
                        });
                    }
                }
                "absorbresource" => t.controllers.push(Controller::AbsorbResource),
                "marketlocation" => t.controllers.push(Controller::MarketLocation),
                "market" => {
                    if let Ok(d) = unbanished_formats::game::market_description(&data) {
                        t.controllers.push(Controller::ResourceProduce);
                        t.market = Some(d);
                    }
                }
                "statusicon" => {
                    t.status_icon = unbanished_formats::game::status_icon_description(&data).ok();
                }
                "cemetery" => {
                    let tombstone = unbanished_formats::game::cemetery_description(&data)
                        .ok()
                        .flatten()
                        .and_then(|r| {
                            let stone = self.load_template(archive, &r)?;
                            let life =
                                read(archive, &normalise(&r).replace(":resource", ":tombstone"))
                                    .and_then(|d| {
                                        unbanished_formats::game::tombstone_description(&d).ok()
                                    })
                                    .unwrap_or((60.0, 24.0));
                            Some((stone, life))
                        });
                    if let Some((stone, (life, range))) = tombstone {
                        t.cemetery = Some(unbanished_game::people::cemetery::CemeteryTemplate {
                            tombstone: stone,
                            life,
                            range,
                        });
                        t.controllers.push(Controller::Cemetery);
                    }
                }
                "storagelocation" => t.controllers.push(Controller::StorageLocation),
                "disease" => {
                    t.disease = unbanished_formats::game::disease_description(&data).ok();
                }
                "pasture" => {
                    if let Ok(group) = unbanished_formats::game::pasture_description(&data) {
                        t.controllers.push(Controller::Pasture);
                        t.pasture = Some(group);
                    }
                }
                "orchard" => {
                    if let Ok((spacing, tend, group)) =
                        unbanished_formats::game::orchard_description(&data)
                    {
                        t.controllers.push(Controller::Orchard);
                        t.orchard = Some(unbanished_game::production::orchard::OrchardTemplate {
                            spacing,
                            tend,
                            group,
                        });
                    }
                }
                "hunt" => {
                    if let Ok(d) = unbanished_formats::game::hunt_description(&data) {
                        t.controllers.push(Controller::ResourceProduce);
                        t.hunt = Some(d);
                    }
                }
                "fishing" => {
                    let Ok(d) = unbanished_formats::game::fishing_description(&data) else {
                        continue;
                    };
                    t.controllers.push(Controller::ResourceProduce);
                    let output = self.load_template(archive, d.output.as_deref()?)?;
                    t.fishing = Some(unbanished_game::production::fishing::FishingTemplate {
                        output,
                        mask: d.mask,
                        group: d.group,
                    });
                }
                "forester" => {
                    let Ok(d) = unbanished_formats::game::forester_description(&data) else {
                        continue;
                    };
                    t.controllers.push(Controller::ResourceProduce);
                    let tree = self.load_template(archive, d.tree.as_deref()?)?;
                    t.forester = Some(unbanished_game::production::forester::ForesterTemplate {
                        tree,
                        group: d.group,
                    });
                }
                "autopickup" => {
                    t.auto_pickup = unbanished_formats::game::auto_pickup_description(&data).ok();
                }
                "consumeproduce" => {
                    let Ok(d) = unbanished_formats::game::consume_produce_description(&data) else {
                        continue;
                    };
                    t.controllers.push(Controller::ResourceProduce);
                    let mut recipes = Vec::new();
                    for r in &d.recipes {
                        let mut inputs = Vec::new();
                        for (m, n) in &r.inputs {
                            let m = m
                                .as_deref()
                                .and_then(|m| material_index(archive, &mut self.materials, m))?;
                            inputs.push((m, *n));
                        }
                        let out = r.output.as_deref()?;
                        let output = self.load_template(archive, out)?;
                        let output_material = self.templates[output].pile_material?;
                        recipes.push(unbanished_game::work::construction::Recipe {
                            inputs,
                            output,
                            output_material,
                            group: r.group,
                        });
                    }
                    t.produce = Some(unbanished_game::work::construction::ProduceTemplate {
                        recipes,
                        cycles: d.cycles,
                        work_type: d.work_type,
                        message: (d.message_type, d.message_value),
                        duration: d.duration,
                        lifetime: d.lifetime,
                        selector: d.selector,
                    });
                }
                "work" => {
                    if let Ok(w) = unbanished_formats::game::work_description(&data) {
                        t.max_workers = w.max_workers;
                        t.per_area = w.per_area.then_some(w.area);
                        t.adjustable = w.flag_20;
                        t.min_jobs = w.value_28;
                        t.max_jobs = w.value_2c;
                    }
                }
                "storage" => {
                    t.storage = storage_description(&data)
                        .ok()
                        .map(|d| (d.accepts, d.capacity, d.flag_20))
                }
                _ => {}
            }
        }
        // Sites: buildings (BuildComponent), removal markers (ClearResourceComponent) and
        // fields (CropFieldComponent).
        if !has_build
            && !t.controllers.contains(&Controller::ClearResource)
            && !t.controllers.contains(&Controller::CropField)
            && !t.controllers.contains(&Controller::Orchard)
            && !t.controllers.contains(&Controller::StorageLocation)
        {
            return None;
        }
        t.meshes = self.templates[template]
            .model
            .as_ref()
            .map(|m| m.variants.iter().map(|v| v.meshes.len()).collect())
            .unwrap_or_default();
        // The finished mesh's height (the icons' anchor above the model).
        t.icon_height = self.templates[template]
            .model
            .as_ref()
            .and_then(|m| m.variants.first()?.meshes.last()?.clone())
            .and_then(|r| read(archive, &normalise(&r)))
            .and_then(|d| unbanished_formats::types::graphics_mesh(&d).ok())
            .map(|m| m.bounds_max[2] - m.bounds_min[2]);
        self.construction.templates.push(t);
        Some(self.construction.templates.len() - 1)
    }

    /// A placed building with a BuildComponent becomes a construction site: its own
    /// StorageComponent and WorkPlaceComponent; activated before the next iteration.
    pub fn add_site(&mut self, archive: &Archive, entity: usize) {
        let template = self.placed[entity].template;
        let Some(st) = self.site_template(archive, template) else {
            return;
        };
        self.add_site_from(st, entity);
    }

    /// The nomad template (`Template\\Nomad.rsc`): its CharacterDescription and its model's
    /// animation groups, for the nomads the town hall calls.
    pub(super) fn load_nomad(&mut self, archive: &Archive, reference: &str) -> Option<usize> {
        let template = self.load_template(archive, reference)?;
        let file = normalise(reference).replace(":resource", ":character");
        let character = read(archive, &file)
            .and_then(|d| unbanished_formats::game::character_description(&d).ok())?;
        for (v, g) in self.templates[template].animations.iter().enumerate() {
            if let Some(g) = g {
                self.wildlife.groups.insert((template, v), g.clone());
            }
        }
        self.wildlife.nomad_kinds.insert(
            template,
            unbanished_game::people::immigration::NomadKind { character },
        );
        let file = normalise(reference).replace(":resource", ":nomad");
        if let Some((_, materials)) =
            read(archive, &file).and_then(|d| unbanished_formats::game::nomad_description(&d).ok())
        {
            self.nomad_materials.insert(template, materials);
        }
        Some(template)
    }

    /// [`Self::add_site`] with its construction template loaded.
    pub(super) fn add_site_from(&mut self, st: usize, entity: usize) {
        if self.templates[self.placed[entity].template].tracker {
            self.construction.trackers.entry(entity).or_default();
        }
        let t = &self.construction.templates[st];
        let (w, h) = self.placed[entity].footprint;
        let storage = self.storages.len();
        self.storages.push(match t.storage {
            Some((accepts, capacity, per_cell)) => Storage {
                capacity: capacity * if per_cell { w * h } else { 1 },
                accepts,
                items: Vec::new(),
            },
            None => Storage::default(),
        });
        if t.controllers.contains(&Controller::WorkPlace) {
            self.construction.workforce.workplaces.insert(
                entity,
                WorkPlace::new(t.profession, t.default_max_workers((w, h))),
            );
        }
        let machine = unbanished_game::work::construction::Machine::new(t.controllers.clone());
        self.construction.sites.insert(
            entity,
            Site {
                entity,
                template: st,
                machine,
                progress: 0,
                stages: 1,
                work_id: -1,
                work_priority: None,
                storage,
                complete: false,
                clear_total: 0,
                destroyed: false,
                // `FUN_14011e3c0`: the selector from the description, every weight 1.
                produce: t
                    .produce
                    .as_ref()
                    .map(|p| unbanished_game::work::construction::ProduceState {
                        disabled: false,
                        selector: p.selector,
                        weights: vec![1.0; p.recipes.len()],
                        produced: 0,
                    })
                    .or_else(|| {
                        (t.herbalist.is_some()
                            || t.forester.is_some()
                            || t.fishing.is_some()
                            || t.hunt.is_some())
                        .then(unbanished_game::work::construction::ProduceState::default)
                    }),
                forester: t
                    .forester
                    .as_ref()
                    .map(|_| unbanished_game::production::forester::Forester::default()),
                happiness_registered: false,
                hunt_time: t.hunt.map(|_| 0),
                crop_field: t
                    .crop_field
                    .map(|_| unbanished_game::production::farm::CropField::default()),
                orchard: t
                    .orchard
                    .map(|_| unbanished_game::production::orchard::Orchard::default()),
                fishing: t
                    .fishing
                    .as_ref()
                    .map(|_| unbanished_game::production::fishing::Fishing::default()),
                auto_pickup: t.auto_pickup.map(|enabled| {
                    unbanished_game::production::herbalist::AutoPickup {
                        enabled,
                        works: Vec::new(),
                    }
                }),
                parts: Vec::new(),
                multi_stages: None,
                graves: Vec::new(),
                destroy: t
                    .destroy
                    .as_ref()
                    .map(unbanished_game::work::destroy::Destroy::new),
                fire: t
                    .fire
                    .map(|_| unbanished_game::disasters::fire::Fire::default()),
                // TradeComponent's activation names the merchants (`Sim::activate`).
                trade: None,
                immigration: t
                    .immigration
                    .as_ref()
                    .map(|_| unbanished_game::people::immigration::Immigration::default()),
                home: None,
                ui_settings: Vec::new(),
                // DiseaseComponent's setup (`FUN_1400495b0`): on a pasture or not.
                disease: t
                    .disease
                    .map(|_| unbanished_game::disasters::blight::Blight {
                        pasture: t.pasture.is_some(),
                        ..Default::default()
                    }),
            },
        );
        // PastureComponent's setup (`FUN_14012cf20`, no kind) over its MapComponent's area.
        if t.pasture.is_some() {
            let (x, y) = self.placed[entity].cell;
            self.wildlife.pastures.insert(
                entity,
                unbanished_game::nature::wildlife::Pasture {
                    area: [x as i16, y as i16, w as i16, h as i16],
                    ..Default::default()
                },
            );
        }
        self.construction.placed.push(entity);
    }
}
