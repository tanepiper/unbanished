//! Autosave: `Dialog/AutoSave.rsc` (vtable `0x140161250`), installed with the other in-game
//! dialogs (`FUN_14009d0d0`). Native builds.
//!
//! - **Timer** (`FUN_14005ea80`, on opening and when Options is applied): timer 0 restarts with
//!   the game option's interval (`+4`, `StringTable.rsc:autoSave`): 1, 5, 10, 15, 20, 30, 45 or
//!   60 minutes; index 8 (or any other) leaves it stopped. `DialogInstance::Update` counts it
//!   down by the frame time and, at zero, posts event `0x13` and starts it again.
//! - **Event `0x13`** (`FUN_14005ea50` → `FUN_14005e820`): unless a dialog pauses the game, the
//!   game is a tutorial (flag bit 10) or a save operation runs, it saves (operation 4 with
//!   argument −4: the file `GetAutoSaveName` "Auto", named by the game's `Autosave` string,
//!   at least 1 s; the preview is the game's save sprite).
//! - **Each frame** (`FUN_14005e9f0`, `FUN_14005e920`): `savingImage` turns 90° a second
//!   (wrapped at 360) and it and `animatedImage` show while a save runs (`SaveLoad` operation
//!   4, also one from the Save dialog).

use unbanished_ui::{NodeId, Ui};

use crate::loops::Screen;

const DIALOG: &str = "Dialog/AutoSave.rsc:resource";
/// `DAT_14017884c` … `DAT_1401788f0`: the intervals in minutes, times `DAT_1401788f0` (60).
const MINUTES: [f32; 8] = [1.0, 5.0, 10.0, 15.0, 20.0, 30.0, 45.0, 60.0];
/// `DAT_140178904`, `DAT_140178938`.
const SPIN: f32 = 90.0;
const TURN: f32 = 360.0;
/// `DAT_14017884c`: the autosave's shortest operation time.
const SAVE_TIME: f32 = 1.0;

pub struct AutoSave {
    image: Option<NodeId>,
    animated: Option<NodeId>,
    /// `+200`.
    angle: f32,
    /// Timer 0: seconds left and its period.
    timer: Option<(f32, f32)>,
    /// The interval it was started with.
    interval: i32,
    /// Seconds left of the autosave's own operation.
    saving: f32,
}

impl AutoSave {
    pub fn open(ui: &mut Ui, screen: &mut Screen, interval: i32) -> Option<Self> {
        let index = screen.add_dialog(ui, DIALOG)?;
        let root = screen.dialogs[index].root;
        let mut a = AutoSave {
            image: ui.find(root, "savingImage"),
            animated: ui.find(root, "animatedImage"),
            angle: 0.0,
            timer: None,
            interval,
            saving: 0.0,
        };
        a.arm(interval);
        a.refresh(ui, false);
        Some(a)
    }

    /// `FUN_14005ea80`.
    fn arm(&mut self, interval: i32) {
        self.interval = interval;
        self.timer = usize::try_from(interval)
            .ok()
            .and_then(|i| MINUTES.get(i))
            .map(|m| (m * 60.0, m * 60.0));
    }

    /// A frame: `interval` is the option now, `can_save` whether event `0x13` may save, and
    /// `other_save` whether the Save dialog is saving. Returns true when an autosave starts.
    pub fn update(
        &mut self,
        ui: &mut Ui,
        dt: f32,
        interval: i32,
        can_save: bool,
        other_save: bool,
    ) -> bool {
        if interval != self.interval {
            self.arm(interval);
        }
        // `FUN_14005e9f0`.
        self.angle += dt * SPIN;
        while self.angle > TURN {
            self.angle -= TURN;
        }
        self.saving = (self.saving - dt).max(0.0);
        let mut start = false;
        if let Some((left, period)) = &mut self.timer {
            *left -= dt;
            if *left <= 0.0 {
                *left = *period;
                if can_save && self.saving <= 0.0 && !other_save {
                    self.saving = SAVE_TIME;
                    start = true;
                }
            }
        }
        self.refresh(ui, self.saving > 0.0 || other_save);
        start
    }

    /// `FUN_14005e920`.
    fn refresh(&self, ui: &mut Ui, saving: bool) {
        for n in [self.image, self.animated].into_iter().flatten() {
            ui.set_visible(n, saving);
        }
        if let Some(n) = self.image {
            ui.node_mut(n).rotation = self.angle;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Index 1 (five minutes) fires every five minutes; index 8 never.
    #[test]
    fn the_timer_repeats_at_the_interval() {
        let mut a = AutoSave {
            image: None,
            animated: None,
            angle: 0.0,
            timer: None,
            interval: 1,
            saving: 0.0,
        };
        a.arm(1);
        let mut ui = Ui::new(std::rc::Rc::new(Default::default()));
        let mut fired = Vec::new();
        for frame in 0..(30 * 60 * 11) {
            if a.update(&mut ui, 1.0 / 30.0, 1, true, false) {
                fired.push(frame / 30);
            }
        }
        assert_eq!(fired, vec![299, 599]);
        a.arm(8);
        assert!(a.timer.is_none());
    }
}
