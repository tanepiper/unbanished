//! What the town makes and uses: the stock manager's counters for each slot (used and made since
//! the last sample) and each workplace's tracker (what it made this year and last). See
//! docs/TOWN-INFO.md.
//!
//! Original: the trackers, `FUN_140059930`.
// Game +0xf0 → +0x170; +0xc; +0x10

use crate::town::Sim;

// +0x28; +0x38; +0x48
/// A workplace's tracker: what it made this year (material and amount), what it made last year,
/// and the year these belong to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tracker {
    /// What it made this year: `(raw material, amount)`.
    pub this_year: Vec<(usize, i32)>,
    /// What it made last year.
    pub last_year: Vec<(usize, i32)>,
    /// The year these belong to.
    pub year: u32,
}

impl Tracker {
    /// In a new year this year's list becomes last year's; then the material's amount grows, or it
    /// joins the list.
    ///
    /// Original: `FUN_140059930`.
    pub fn add(&mut self, year: u32, material: usize, amount: i32) {
        if year != self.year {
            self.last_year = std::mem::take(&mut self.this_year);
            self.year = year;
        }
        if let Some(e) = self.this_year.iter_mut().find(|e| e.0 == material) {
            e.1 += amount;
            return;
        }
        self.this_year.push((material, amount));
    }
}

// +0xc; +0x10
/// The stock manager's slot counters since the last sample (of each slot).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StockCounters {
    /// The amount used, per slot.
    pub used: [i32; 21],
    /// The amount made, per slot.
    pub made: [i32; 21],
}

impl Sim<'_, '_> {
    /// `FUN_14008e3c0(stock, owner, update, material, amount)`: the owner's TrackerComponent
    /// (when `update`) and the material's slot count what was made.
    pub(crate) fn count_made(
        &mut self,
        owner: Option<usize>,
        update: bool,
        material: usize,
        n: i32,
    ) {
        if let (Some(o), true) = (owner, update) {
            let year = self.year;
            if let Some(t) = self.trackers.get_mut(&o) {
                t.add(year, material, n);
            }
        }
        if let Some(slot) = crate::work::produce::production_group(self.materials[material].flags) {
            self.counters.made[slot] += n;
        }
    }

    /// The `+0xc` count of the material's slot (`FUN_14008e460`): eaten, burned, used to build,
    /// visited or traded away.
    pub(crate) fn count_used(&mut self, material: usize, n: i32) {
        if let Some(slot) = crate::work::produce::production_group(self.materials[material].flags) {
            self.counters.used[slot] += n;
        }
    }
}

// +0x150
/// The series the stock manager samples: 8 of the citizens, then per stock slot its count, used and
/// made.
pub const SERIES: usize = 8 + 3 * 21;
/// A sample every 75 s.
///
/// Original: `FUN_140091780`.
pub const SAMPLE_SECONDS: f32 = 75.0;

// +0x3c; +0x44; +0x208; +0x50; +0x80; +0x210; +0x118; +0x1e4
/// What a sample reads of each citizen (age against, school, health, happiness, education against,
/// coat).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CitizenSample {
    /// Whether the citizen is an adult.
    pub adult: bool,
    /// Whether it attends school.
    pub school: bool,
    /// The health.
    pub health: f32,
    /// The happiness.
    pub happiness: f32,
    /// Whether it is educated.
    pub educated: bool,
    /// Whether it has a coat.
    pub coat: bool,
}

// +0x198; +0x150
/// The Town Info graphs' history (StockManager) and the time to the next sample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statistics {
    /// The history of each graph.
    pub series: Vec<Vec<i32>>,
    /// The time to the next sample.
    pub timer: i64,
}

impl Default for Statistics {
    fn default() -> Self {
        Statistics {
            series: vec![Vec::new(); SERIES],
            timer: 0,
        }
    }
}

/// `cvttss2si`: out of range (and NaN) is `i32::MIN`.
fn truncate(v: f32) -> i32 {
    if v.is_nan() || v >= 2_147_483_648.0 || v < -2_147_483_648.0 {
        i32::MIN
    } else {
        v as i32
    }
}

impl Statistics {
    /// The timer counts down `dt`; below 1 a sample is taken and the next is 75 s off.
    ///
    /// Original: `FUN_14008fc00`, `FUN_140091780` (sampling).
    pub fn update(
        &mut self,
        dt: i64,
        citizens: &[CitizenSample],
        health_max: f32,
        happiness_max: f32,
        stock: &[i32; 21],
        counters: &mut StockCounters,
    ) -> bool {
        self.timer -= dt;
        if 0 < self.timer {
            return false;
        }
        self.sample(citizens, health_max, happiness_max, stock, counters);
        self.timer = crate::scheduler::ticks(SAMPLE_SECONDS);
        true
    }

    /// The population (all, adults, students, children), the average health and happiness and the
    /// share educated (of the adults) and with coats, in percent; each stock slot's count, used and
    /// made; then the used and made counts restart.
    ///
    /// Original: `FUN_14008fc00`.
    pub fn sample(
        &mut self,
        citizens: &[CitizenSample],
        health_max: f32,
        happiness_max: f32,
        stock: &[i32; 21],
        counters: &mut StockCounters,
    ) {
        let (mut adults, mut students, mut children) = (0, 0, 0);
        let (mut health, mut happiness) = (0.0f32, 0.0f32);
        let (mut educated, mut coats) = (0, 0);
        for c in citizens {
            if !c.adult || c.school {
                if c.school {
                    students += 1;
                } else {
                    children += 1;
                }
            } else {
                adults += 1;
            }
            health += c.health;
            happiness += c.happiness;
            if c.educated && c.adult && !c.school {
                educated += 1;
            }
            if c.coat {
                coats += 1;
            }
        }
        let (mut h, mut j, mut e, mut k) = (0, 0, 0, 0);
        if !citizens.is_empty() {
            let n = citizens.len() as f32;
            h = truncate(((health / n) / health_max) * 100.0);
            j = truncate(((happiness / n) / happiness_max) * 100.0);
            e = truncate((educated as f32 / adults as f32) * 100.0);
            k = truncate((coats as f32 / n) * 100.0);
        }
        let people = [
            citizens.len() as i32,
            adults,
            students,
            children,
            h,
            e,
            j,
            k,
        ];
        for (s, v) in self.series.iter_mut().zip(people) {
            s.push(v);
        }
        for slot in 0..21 {
            self.series[8 + 3 * slot].push(stock[slot]);
            self.series[9 + 3 * slot].push(counters.used[slot]);
            self.series[10 + 3 * slot].push(counters.made[slot]);
        }
        *counters = StockCounters::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tracker_keeps_this_year_and_last() {
        let mut t = Tracker::default();
        t.add(0, 3, 5);
        t.add(0, 3, 2);
        t.add(0, 4, 1);
        assert_eq!(t.this_year, vec![(3, 7), (4, 1)]);
        t.add(1, 4, 9);
        assert_eq!(t.last_year, vec![(3, 7), (4, 1)]);
        assert_eq!(t.this_year, vec![(4, 9)]);
    }

    #[test]
    fn a_sample_counts_the_town() {
        let adult = CitizenSample {
            adult: true,
            health: 2.0,
            happiness: 1.0,
            educated: true,
            coat: true,
            ..Default::default()
        };
        let child = CitizenSample {
            health: 2.0,
            happiness: 1.0,
            ..Default::default()
        };
        let student = CitizenSample {
            school: true,
            ..child
        };
        let mut stats = Statistics::default();
        let mut counters = StockCounters::default();
        counters.made[2] = 7;
        counters.used[2] = 3;
        let stock = [1; 21];
        assert!(stats.update(
            1,
            &[adult, child, student, adult],
            4.0,
            2.0,
            &stock,
            &mut counters
        ));
        let last: Vec<i32> = stats.series.iter().map(|s| *s.last().unwrap()).collect();
        assert_eq!(&last[..8], &[4, 2, 1, 1, 50, 100, 50, 50]);
        assert_eq!(&last[8 + 6..8 + 9], &[1, 3, 7]);
        assert_eq!(counters, StockCounters::default());
        // The next sample is 75 s off; with no one, the shares are 0.
        assert!(!stats.update(
            crate::scheduler::ticks(74.0),
            &[],
            4.0,
            2.0,
            &stock,
            &mut counters
        ));
        assert!(stats.update(
            crate::scheduler::ticks(1.0),
            &[],
            4.0,
            2.0,
            &stock,
            &mut counters
        ));
        assert_eq!(stats.series[4], vec![50, 0]);
    }
}
