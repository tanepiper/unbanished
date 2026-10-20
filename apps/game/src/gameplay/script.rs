//! Capture script commands (`--script`, `--event` and the probe's `script`): speed, pause, saving,
//! windows, tools, placing, selecting and events.

use super::*;

impl GameplayLoop {
    /// `GameLoop::script`: run one capture script command; false when it is not one this loop
    /// takes.
    pub(super) fn run_script(&mut self, ctx: &mut Ctx, command: &str) -> bool {
        let mut words = command.split_whitespace();
        match words.next() {
            Some("event") => {}
            // `save PATH`: write the game (one loaded from a save) to `PATH` once the next frame
            // has drawn its preview.
            #[cfg(not(target_arch = "wasm32"))]
            Some("save") => {
                let Some(path) = words.next() else {
                    return false;
                };
                self.pending_save = Some((SaveTarget::File(path.into()), None, None));
                return true;
            }
            // `toggle_pause`: the TogglePause key (captures).
            Some("toggle_pause") => {
                self.execute(ctx.archive, PlayerCommand::SetPaused(!self.paused));
                return true;
            }
            // `speed N`: the game speed, one of the toolbar's levels (1, 2, 5, 10; captures and
            // profiling).
            Some("speed") => {
                let level = words.next().and_then(|w| w.parse::<u32>().ok());
                let Some(i) = SPEED_LEVELS.iter().position(|&l| Some(l) == level) else {
                    return false;
                };
                self.execute(ctx.archive, PlayerCommand::SetSpeed(i as u8));
                return true;
            }
            // `pause`: open the pause menu (captures).
            Some("pause") => {
                self.open_pause_menu(ctx);
                return self.pause_menu.is_some();
            }
            Some("open") => {
                let (Some(template), Some(screen)) = (words.next(), self.screen.as_mut()) else {
                    return false;
                };
                let map_size = self.town.as_ref().map_or(0, |t| t.cells.size as u32);
                self.utility.toggle(
                    ctx.ui,
                    screen,
                    ctx.archive,
                    template,
                    Some(&self.events),
                    map_size,
                );
                return true;
            }
            // `place TEMPLATE`: place a building on the first clear spot near the start.
            Some("place") => {
                let (Some(template), true, true) =
                    (words.next(), self.town.is_some(), self.view.is_some())
                else {
                    return false;
                };
                let start = PlayerCommand::StartTool {
                    template: template.to_string(),
                };
                let Outcome::Tool(Some(Tool::Placement(mut tool))) =
                    self.execute(ctx.archive, start)
                else {
                    return false;
                };
                let (Some(town), Some(heights)) = (self.town.as_ref(), self.heights.as_ref())
                else {
                    return false;
                };
                let grid = heights.grid();
                let near = town.start;
                if tool.find_clear_spot(town, &grid, near).is_none() {
                    return false;
                }
                let placed = match tool.placement(0) {
                    Some(p) => match self.execute(ctx.archive, PlayerCommand::Place(p)) {
                        Outcome::Placed(id) => Some(id),
                        _ => None,
                    },
                    None => None,
                };
                let Some(town) = self.town.as_ref() else {
                    return false;
                };
                let cell = placed.map(|e| (town.placed[e].cell, town.start));
                log::info!("script placed {template} as {placed:?} (cell, start: {cell:?})");
                if let Some(models) = self.view.as_mut().and_then(|v| v.models.as_mut()) {
                    if let Some(town) = &self.town {
                        self.town_view
                            .as_ref()
                            .expect("the town view comes with the town")
                            .refresh_models(town, ctx.archive, models);
                    }
                }
                return placed.is_some();
            }
            // `remove TEMPLATE RADIUS`: drag a removal tool over the square around the start.
            Some("remove") => {
                let (Some(template), Some(r), true, true) = (
                    words.next(),
                    words.next().and_then(|w| w.parse::<i32>().ok()),
                    self.town.is_some(),
                    self.view.is_some(),
                ) else {
                    return false;
                };
                let start = PlayerCommand::StartTool {
                    template: template.to_string(),
                };
                let Outcome::Tool(Some(Tool::Removal(tool))) = self.execute(ctx.archive, start)
                else {
                    return false;
                };
                let Some(town) = self.town.as_ref() else {
                    return false;
                };
                let n = town.cells.size as i32;
                let (x, y) = (town.start.0 as i32, town.start.1 as i32);
                let clamp = |v: i32| v.clamp(0, n - 1);
                let a = clamp(y - r) * n + clamp(x - r);
                let b = clamp(y + r) * n + clamp(x + r);
                let drag = crate::gameplay::commands::Drag {
                    start: a,
                    end: b,
                    ..tool.drag_command()
                };
                self.execute(ctx.archive, PlayerCommand::Drag(drag));
                let Some(town) = self.town.as_ref() else {
                    return false;
                };
                let roads: Vec<i32> = (0..town.cells.road_kind.len() as i32)
                    .filter(|&i| town.cells.road_kind[i as usize] != 0xff)
                    .collect();
                log::info!(
                    "script dragged {template} from {a} to {b} (start {:?}); {} road cells: {:?}",
                    town.start,
                    roads.len(),
                    &roads[..roads.len().min(8)]
                );
                return true;
            }
            // `tool TEMPLATE`: start a template's tool, as its toolbar button does.
            // `screenshot [ui]`: the screenshot actions (`0x21`, `0x22` with `ui`).
            Some("screenshot") => {
                self.screenshot = Some(words.next() == Some("ui"));
                return true;
            }
            // `rotate`: StructureRight (T), one step clockwise.
            Some("rotate") => {
                self.rotation = (self.rotation + 1) % 4;
                self.refresh_placement(ctx);
                return true;
            }
            Some("tool") => {
                let Some(template) = words.next() else {
                    return false;
                };
                self.start_tool(ctx, template);
                return self.placement.is_some()
                    || self.removal.is_some()
                    || self.path_tool.is_some();
            }
            // `select`: select the most recently placed entity (EditTool::Select).
            Some("select") => {
                let (Some(town), Some(screen)) = (self.town.as_ref(), self.screen.as_mut()) else {
                    return false;
                };
                // `select citizen N`: the N-th living citizen; `select profession NAME N`: the
                // N-th living citizen whose profession's name contains NAME (`fish`, `hunt`),
                // for captures that follow one. Plain `select`: the last placed entity.
                let kind = words.next();
                let entity = if matches!(kind, Some("citizen" | "profession")) {
                    let wanted: Option<Vec<usize>> = (kind == Some("profession")).then(|| {
                        let name = words.next().unwrap_or("").to_lowercase();
                        let list = &town.construction.professions.list;
                        (0..list.len())
                            .filter(|&p| list[p].name.to_lowercase().contains(&name))
                            .collect()
                    });
                    let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
                    let Some(c) = (0..town.citizens.len())
                        .filter(|&i| town.citizens[i].alive)
                        .filter(|&i| {
                            wanted.as_ref().is_none_or(|w| {
                                town.agents
                                    .states
                                    .get(i)
                                    .is_some_and(|s| w.contains(&s.profession))
                            })
                        })
                        .nth(n)
                    else {
                        return false;
                    };
                    Entity::Citizen(c)
                } else {
                    let Some(p) = town.placed.iter().rposition(|p| p.alive) else {
                        return false;
                    };
                    Entity::Placed(p)
                };
                self.selection.selected = Some(entity);
                self.entities
                    .select(ctx.ui, screen, ctx.archive, town, entity);
                return true;
            }
            // `profession INDEX COUNT`: the ProfessionUI spinner.
            Some("profession") => {
                let (Some(profession), Some(count), Some(_)) = (
                    words.next().and_then(|w| w.parse().ok()),
                    words.next().and_then(|w| w.parse().ok()),
                    self.town.as_ref(),
                ) else {
                    return false;
                };
                let set =
                    unbanished_game::work::professions::Command::SetCount { profession, count };
                self.execute(ctx.archive, PlayerCommand::Town(set));
                return true;
            }
            _ => return false,
        }
        let Some(kind) = words.next().and_then(|k| k.parse().ok()) else {
            return false;
        };
        let cell = words.next().and_then(|c| c.parse().ok()).unwrap_or(-1);
        let (month, year) = (self.month(), self.calendar.year as i32);
        self.events
            .add(kind, 3, cell, month, year, Default::default(), false);
        true
    }
}
