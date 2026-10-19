//! Citizens: the citizen window and status icons.

use super::*;

/// What CitizenUI's refresh (`FUN_140065440`) reads from the Citizen and CitizenAI components.
#[derive(Debug, Clone)]
pub struct CitizenInfo {
    /// `+0x28`, `+0x38`, `+0x3c`.
    pub name: String,
    pub male: bool,
    pub age: f32,
    /// `+0x210 >= D+0x118`.
    pub educated: bool,
    /// `+0x1e4`: index into `Dialog\StringTable.rsc:clothing`.
    pub clothing: i32,
    /// 0 none, 1 iron, 2 steel (`+0xac`, `+0xa8` vs `D+0x110`).
    pub tool: i32,
    /// Profession shown: `(string table, key)`.
    pub profession: Option<(Option<String>, String)>,
    pub health: f32,
    pub health_max: f32,
    pub happiness: f32,
    pub happiness_max: f32,
    /// CitizenAI `+0x28`, or 16 while working away from the citizen's workplace.
    pub activity: i32,
    /// Profession button enabled (`+0x248` work or `+0x208` school).
    pub has_work: bool,
    /// Home button enabled (`+0x238`).
    pub has_home: bool,
}

pub(crate) fn text_of(ui: &Ui, id: NodeId) -> String {
    match &ui.node(id).kind {
        Kind::Text(t) | Kind::Label { text: t, .. } => t.text.clone(),
        _ => String::new(),
    }
}

/// `System::String::operator<` / `StrCompare`: byte comparison on signed `char`.
pub(crate) fn str_less(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    for i in 0.. {
        let (x, y) = (
            a.get(i).map_or(0, |&c| c as i8),
            b.get(i).map_or(0, |&c| c as i8),
        );
        if x != y {
            return x < y;
        }
        if x == 0 {
            return false;
        }
    }
    false
}

/// `CitizenUI` (vtable `0x140161b30`): bind `FUN_140065b00`, refresh `FUN_140065440`, close
/// `FUN_140065a90`, events `FUN_140065a50`.
pub struct CitizenUi {
    male: Option<NodeId>,
    female: Option<NodeId>,
    name: Option<(NodeId, String)>,
    age: Option<NodeId>,
    profession: Option<NodeId>,
    educated_yes: Option<NodeId>,
    educated_no: Option<NodeId>,
    clothing: Option<NodeId>,
    tool: Option<NodeId>,
    name_is: Option<(NodeId, String)>,
    action: Option<NodeId>,
    hearts: Option<NodeId>,
    stars: Option<NodeId>,
    /// Host buttons (element ids 0x227, 0x226, 0x228).
    button_profession: Option<NodeId>,
    button_home: Option<NodeId>,
}

/// `StatusIconUI` (vtable `0x140163cd0`): bind `FUN_140080740` caches the `icons` children;
/// refresh `FUN_140080570` shows icon `i` when status bit `i` is set. Only a controller with a
/// config (`+0x10`) whose `+0x20` flag (`StatusIconUIConfig::_showOnMainPage`) is clear
/// additionally requires the host page to be 2 or 3; the crop field's, orchard's and others'
/// build-status icons have such a config (`CropField.rsc:titleBuildStatus`).
pub struct StatusIconUi {
    icons: Vec<NodeId>,
    always: bool,
}

impl CitizenUi {
    pub fn bind(ui: &Ui, element: NodeId, root: NodeId) -> CitizenUi {
        let f = |n: &str| ui.find(element, n);
        let param = |n: &str| f(n).map(|id| (id, text_of(ui, id)));
        CitizenUi {
            male: f("imageMale"),
            female: f("imageFemale"),
            name: param("textName"),
            age: f("numberAge"),
            profession: f("textProfession"),
            educated_yes: f("textEducatedYes"),
            educated_no: f("textEducatedNo"),
            clothing: f("textClothing"),
            tool: f("textTool"),
            name_is: param("textNameIs"),
            action: f("textAction"),
            hearts: f("groupHearts"),
            stars: f("groupStars"),
            button_profession: ui.find(root, "buttonProfession"),
            button_home: ui.find(root, "buttonHome"),
        }
    }

    pub fn refresh(&self, ui: &mut Ui, data: &EntityData) {
        let Some(c) = &data.citizen else { return };
        let show = |ui: &mut Ui, n: Option<NodeId>, v: bool| {
            if let Some(n) = n {
                ui.set_visible(n, v);
            }
        };
        show(ui, self.female, !c.male);
        show(ui, self.male, c.male);
        show(ui, self.educated_no, !c.educated);
        show(ui, self.educated_yes, c.educated);
        for (id, template) in [&self.name, &self.name_is].into_iter().flatten() {
            ui.set_text(*id, &template.replace("@0", &c.name));
        }
        // `DAT_140178898` = 5.
        if let Some(n) = self.age {
            ui.set_value(n, (c.age * 5.0) as i32 as f32);
        }
        if let Some(n) = self.clothing {
            let t = ui
                .res
                .string_at(r"Dialog\StringTable.rsc:clothing", c.clothing);
            ui.set_text(n, &t);
        }
        if let Some(n) = self.tool {
            let t = ui.res.string_at(r"Dialog\StringTable.rsc:toolType", c.tool);
            ui.set_text(n, &t);
        }
        if let (Some(n), Some((table, key))) = (self.profession, &c.profession) {
            let t = ui.res.string(table.as_deref(), key);
            ui.set_text(n, &t);
        }
        crate::ui::utility::StatusBar::set_meter(ui, self.hearts, c.health, c.health_max);
        crate::ui::utility::StatusBar::set_meter(ui, self.stars, c.happiness, c.happiness_max);
        // `FUN_140065440`: the buttons usable while there is a building to show.
        if let Some(b) = self.button_profession {
            ui.node_mut(b).enabled = c.has_work;
        }
        if let Some(b) = self.button_home {
            ui.node_mut(b).enabled = c.has_home;
        }
        if let Some(n) = self.action {
            let t = ui
                .res
                .string_at(r"Dialog\StringTable.rsc:citizenActions", c.activity);
            ui.set_text(n, &t);
        }
    }
}

impl StatusIconUi {
    pub fn bind(ui: &Ui, element: NodeId, always: bool) -> Option<StatusIconUi> {
        let icons = ui.find(element, "icons")?;
        Some(StatusIconUi {
            icons: ui.children(icons),
            always,
        })
    }

    pub fn refresh(&self, ui: &mut Ui, data: &EntityData) {
        let page_ok = self.always || matches!(data.active_page, 2 | 3);
        for (i, &icon) in self.icons.iter().enumerate() {
            let bit = 1u32.rotate_left(i as u32);
            ui.set_visible(icon, page_ok && data.status & bit != 0);
        }
    }
}
