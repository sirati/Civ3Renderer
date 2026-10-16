//! What the map label of a city shows beyond the city's own record (docs
//! §10.8): the turns until it grows and until it completes what it builds, and
//! the icons of its HUD.
//!
//! These are the game's own rules, ported from its code (addresses are PCGames
//! v1.22), so that labels can be drawn for cities the game does not draw, and
//! over any view of the world. The rules are plain data and functions
//! ([`Rules`], [`CityFacts`]); reading them from the game is
//! [`crate::Civ3::cities`].

use std::collections::HashSet;

use crate::layout as L;

/// The number the game shows for "never" (`0x668F70`).
pub const NEVER: i32 = 9999;

/// The bits of [`Rules::veteran_icons`]: which of the icons beside a city the
/// game shows, by the `ITF_Veteran_*` building it has.
pub const VETERAN_GROUND: u8 = 1;
pub const VETERAN_SEA: u8 = 2;
pub const VETERAN_AIR: u8 = 4;

/// An improvement as the label rules see it (a `BIC.Improvements` record).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImprovementRules {
    pub name: String,
    pub cost: i32,
    /// `ImprovementFlags`, `Characteristics`, `SmallWonderFlags`.
    pub flags: u32,
    pub characteristics: u32,
    pub small_wonder_flags: u32,
    /// The government it needs (-1: any).
    pub government: i32,
    /// The tech that makes it obsolete (-1: none).
    pub obsolete: i32,
}

/// A player as the label rules see it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LeaderRules {
    pub government: i32,
    pub capital: i32,
    pub cities: i32,
    /// The `Race.Bonuses` of the player's race (bit per `RB_*`).
    pub race_bonuses: u32,
    /// Per improvement, the city holding that small wonder.
    pub small_wonders: Vec<i32>,
}

/// What the label rules read from the game once, for all cities.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rules {
    /// The civ whose view is drawn.
    pub viewer: i32,
    /// `p_human_player_bits`.
    pub human_players: u32,
    /// `BIC.DifficultyLevels[p_game_difficulty].Cost_Factor`.
    pub difficulty_cost_factor: i32,
    /// `TR_ACCELERATED_PRODUCTION`.
    pub accelerated_production: bool,
    /// `General.MaximumSize_Town` / `MaximumSize_City`.
    pub max_town: i32,
    pub max_city: i32,
    pub improvements: Vec<ImprovementRules>,
    /// Per unit type: `Name` and `Cost`.
    pub unit_names: Vec<String>,
    pub unit_costs: Vec<i32>,
    /// Per citizen type: `Construction` (shields a specialist adds).
    pub citizen_construction: Vec<i32>,
    /// Per tech: `Advance.Flags`, and the players knowing it (bit per civ).
    pub tech_flags: Vec<u32>,
    pub tech_known_by: Vec<u32>,
    /// `WorldSizes[world size].OptimalCityCount`.
    pub optimal_cities: i32,
    /// `BIC.transition_government` (anarchy).
    pub anarchy: i32,
    /// Per improvement, the city holding that wonder (`Game_get_wonder_city_id`).
    pub wonder_cities: Vec<i32>,
    pub leaders: Vec<LeaderRules>,
    /// The viewer's `Leader.At_War`, `field_D50`, `field_D70` per civ (the
    /// foreign capitals' icons).
    pub viewer_war: [bool; 32],
    pub viewer_d50: [bool; 32],
    pub viewer_d70: [bool; 32],
}

/// What the rules need to know of a city.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CityFacts {
    pub id: i32,
    pub civ: i32,
    pub size: i32,
    pub improvements: [u8; 32],
    /// `Order_Type` (`COT_*`) and `Order_ID`.
    pub order: (i32, i32),
    pub stored_food: i32,
    pub food_income: i32,
    pub stored_production: i32,
    pub production_income: i32,
    pub specialist_stock: i32,
    /// Shields the city's specialists add (`Citizen_Type.Construction` of each
    /// citizen with status 0).
    pub specialist_shields: i32,
    /// Whether the city's tile has a river.
    pub on_river: bool,
}

/// `a / b` rounded up (`b > 0`), as the game computes turns.
fn ceil_div(a: i32, b: i32) -> i32 {
    let q = a / b;
    q + (q * b < a) as i32
}

impl Rules {
    fn leader(&self, civ: i32) -> Option<&LeaderRules> {
        usize::try_from(civ).ok().and_then(|c| self.leaders.get(c))
    }

    /// The name of what a city builds (its `Order_Type`, `Order_ID`): an
    /// improvement or a unit type.
    pub fn order_name(&self, (kind, id): (i32, i32)) -> Option<String> {
        let id = usize::try_from(id).ok()?;
        match kind as i64 {
            x if x == L::COT_IMPROVEMENT => self.improvements.get(id).map(|i| i.name.clone()),
            x if x == L::COT_UNIT => self.unit_names.get(id).cloned(),
            _ => None,
        }
    }

    /// `Leader_get_food_cost_factor` (0x566090, `ignore_difficulty` false):
    /// the cost factor of production and growth. 10 for a human player.
    pub fn cost_factor(&self, civ: i32) -> i32 {
        let human = self.human_players & (1u32 << (civ & 31)) != 0;
        let f = if human { 10 } else { self.difficulty_cost_factor };
        let f = if self.accelerated_production { f / 2 } else { f };
        f.max(1)
    }

    /// `City_get_size_class` (0x4275C0).
    pub fn size_class(&self, size: i32) -> i32 {
        if size > self.max_city {
            2
        } else {
            (size > self.max_town) as i32
        }
    }

    /// Whether the player knows `tech` (`Leader_has_tech`, 0x5613F0).
    pub fn knows(&self, civ: i32, tech: i32) -> bool {
        if tech < 0 {
            return true;
        }
        usize::try_from(tech).ok().and_then(|t| self.tech_known_by.get(t)).is_some_and(|k| k & (1u32 << (civ & 31)) != 0)
    }

    /// `Leader_get_improvement_cost` (0x569F90): what the player pays for
    /// improvement `id` (shields).
    pub fn improvement_cost(&self, civ: i32, id: i32) -> i32 {
        let Some(imp) = usize::try_from(id).ok().and_then(|i| self.improvements.get(i)) else { return 1 };
        let mut cost = self.cost_factor(civ) * imp.cost;
        let wonder = imp.characteristics & (L::ITC_WONDER | L::ITC_SMALL_WONDER) as u32 != 0;
        let bonuses = self.leader(civ).map_or(0, |l| l.race_bonuses);
        let has = |bonus: i64| bonuses & (1u32 << bonus) != 0;
        let trait_discount = [
            (L::RB_MILITARISTIC, L::ITC_MILITARISTIC),
            (L::RB_RELIGIOUS, L::ITC_RELIGIOUS),
            (L::RB_AGRICULTURAL, L::ITC_AGRICULTURAL),
            (L::RB_SEAFARING, L::ITC_SEAFARING),
            (L::RB_SCIENTIFIC, L::ITC_SCIENTIFIC),
        ]
        .iter()
        .any(|&(bonus, flag)| has(bonus) && imp.characteristics & flag as u32 != 0);
        // A civ's trait halves the improvements of its kind.
        if !wonder && trait_discount {
            cost /= 2;
        }
        // The palace costs more with the number of cities.
        if imp.flags & L::ITF_CENTER_OF_EMPIRE as u32 != 0 {
            let cities = self.leader(civ).map_or(0, |l| l.cities);
            let k = (cities * 6) / self.optimal_cities.max(1);
            cost *= k.clamp(3, 10);
        }
        cost.max(1)
    }

    /// `Leader_get_unit_cost` (0x56A1C0).
    pub fn unit_cost(&self, civ: i32, id: i32) -> i32 {
        let base = usize::try_from(id).ok().and_then(|i| self.unit_costs.get(i)).copied().unwrap_or(0);
        (self.cost_factor(civ) * base / 10).max(1)
    }

    /// `City_get_turns_to_build` (0x4BFDD0) for the city's own order, as the
    /// label asks (`subtract the stored production`), with C3X's guard against
    /// no production.
    pub fn turns_to_build(&self, city: &CityFacts) -> i32 {
        let (kind, id) = city.order;
        let mut income = city.production_income;
        if income < 1 {
            return NEVER;
        }
        let mut cost = match kind as i64 {
            x if x == L::COT_IMPROVEMENT => self.improvement_cost(city.civ, id),
            x if x == L::COT_UNIT => self.unit_cost(city.civ, id),
            _ => 0,
        };
        let improvement = kind as i64 == L::COT_IMPROVEMENT;
        if improvement {
            // Specialists add shields to improvements.
            income = (income + city.specialist_shields).max(0);
            if income == 0 {
                return NEVER;
            }
        }
        let extra = if improvement { city.specialist_stock } else { 0 };
        cost -= (city.stored_production + extra).max(0);
        ceil_div(cost, income).max(1)
    }

    /// The improvement (id) whose lack stops the city from growing, if any
    /// (`City_requires_improvement_to_grow`, 0x4B1E50): the first with
    /// `ITF_Allows_City_Level_2` for a town at its limit that has none and no
    /// river, the first with `ITF_Allows_City_Level_3` for a city at its limit.
    /// `has` tells whether the city has an improvement active
    /// (`City_has_improvement`, with free ones); lakes next to the city are not
    /// looked for.
    pub fn requires_improvement_to_grow(&self, city: &CityFacts, has: &dyn Fn(i32) -> bool) -> Option<i32> {
        let allowing = |flag: i64| {
            let own = (0..self.improvements.len() as i32).any(|i| {
                let imp = &self.improvements[i as usize];
                has(i) && !(imp.obsolete >= 0 && self.knows(city.civ, imp.obsolete)) && imp.flags & flag as u32 != 0
            });
            let first = self.improvements.iter().position(|imp| imp.flags & flag as u32 != 0).map(|i| i as i32);
            (own, first)
        };
        if self.max_town <= city.size {
            let (has_level_2, first) = allowing(L::ITF_ALLOWS_CITY_LEVEL_2);
            if !has_level_2 && !city.on_river && first.is_some() {
                return first;
            }
        }
        if self.max_city <= city.size {
            let (has_level_3, first) = allowing(L::ITF_ALLOWS_CITY_LEVEL_3);
            if has_level_3 {
                return None;
            }
            return first;
        }
        None
    }

    /// The turns until the city grows, as its label shows them
    /// (`Main_Screen_Form::draw_city_hud`, 0x4E5640): [`NEVER`] without food
    /// surplus or while an improvement is needed.
    pub fn turns_to_grow(&self, city: &CityFacts, has: &dyn Fn(i32) -> bool) -> i32 {
        if city.food_income < 1 {
            return NEVER;
        }
        let factor = self.cost_factor(city.civ);
        let class = self.size_class(city.size);
        let full = (class * 2 + 2) * factor;
        let mut turns = (full - city.stored_food) / city.food_income;
        if city.food_income * turns < full - city.stored_food {
            turns += 1;
        }
        match turns {
            0 => match self.requires_improvement_to_grow(city, has) {
                None => 1,
                Some(_) => NEVER,
            },
            t if t < 1 => 1,
            t => t,
        }
    }

    /// Per improvement, whether `civ` has one in the city as the game counts
    /// it (`City_has_improvement` with free buildings, 0x4ACBE0): `free` are
    /// the keys of the player's free buildings and `continent` the id of the
    /// city's continent.
    pub fn has_improvement(&self, city: &CityFacts, free: &HashSet<i32>, continent: i32, id: i32) -> bool {
        if id < 0 {
            return true;
        }
        let Some(imp) = self.improvements.get(id as usize) else { return false };
        let government = self.leader(city.civ).map_or(-1, |l| l.government);
        if imp.government != -1 && imp.government != government {
            return false;
        }
        if free.contains(&id) || free.contains(&((continent + 1) * self.improvements.len() as i32 + id)) {
            return true;
        }
        (0..256).contains(&id) && city.improvements[id as usize >> 3] & (1 << (id & 7)) != 0
    }

    /// Whether the city has a working, not obsolete improvement with `ImprovementFlags`
    /// `flag` (`City_count_improvements_with_flag`, 0x4B2020, more than 0).
    pub fn has_improvement_with_flag(&self, city: &CityFacts, has: &dyn Fn(i32) -> bool, flag: u32) -> bool {
        self.improvements.iter().enumerate().any(|(i, imp)| {
            imp.flags & flag != 0 && has(i as i32) && !(imp.obsolete >= 0 && self.knows(city.civ, imp.obsolete))
        })
    }

    /// The icons the label shows for buildings that train veterans (§10.8), as a
    /// bit set: [`VETERAN_GROUND`] (barracks, `0x4AE160`), [`VETERAN_SEA`]
    /// (harbour) and [`VETERAN_AIR`] (airport; C3X's `city_icons_show_unit_effects_not_trade`).
    pub fn veteran_icons(&self, city: &CityFacts, has: &dyn Fn(i32) -> bool) -> u8 {
        [(VETERAN_GROUND, 0x2), (VETERAN_SEA, 0x2_0000), (VETERAN_AIR, 0x4_0000)]
            .iter()
            .filter(|&&(_, flag)| self.has_improvement_with_flag(city, has, flag))
            .fold(0, |bits, &(bit, _)| bits | bit)
    }

    /// How many small wonders and wonders with `small_wonder_flags` the player
    /// `civ` has (`Leader_count_wonders_with_small_flag`, 0x55A9C0), in city
    /// `only` if given. `owner` gives the civ of a city.
    fn count_spy_wonders(&self, civ: i32, only: Option<i32>, owner: &dyn Fn(i32) -> Option<i32>) -> usize {
        let Some(leader) = self.leader(civ) else { return 0 };
        self.improvements
            .iter()
            .enumerate()
            .filter(|(_, imp)| (imp.government == leader.government || imp.government == -1) && imp.small_wonder_flags & L::ITSW_ALLOWS_SPY_MISSIONS as u32 != 0)
            .filter_map(|(i, imp)| {
                if imp.characteristics & L::ITC_SMALL_WONDER as u32 != 0 {
                    leader.small_wonders.get(i).copied()
                } else if imp.characteristics & L::ITC_WONDER as u32 != 0 {
                    self.wonder_cities.get(i).copied()
                } else {
                    None
                }
            })
            .filter(|&city| only.is_none_or(|o| o == city) && owner(city) == Some(civ))
            .count()
    }

    /// The icons of the label's HUD box (`City_draw_hud_icon`, 0x4B9CF0), in the
    /// order they are drawn, all at one place: 0 the spies' wonder (a small
    /// wonder allowing spy missions), 1 the capital's star, 2 the diplomats'
    /// coin (a capital of a player that knows diplomats). Foreign capitals
    /// show them by what the viewer knows of the player. `owner` gives the civ
    /// of a city.
    pub fn hud_icons(&self, city: &CityFacts, owner: &dyn Fn(i32) -> Option<i32>) -> Vec<u8> {
        let Some(leader) = self.leader(city.civ) else { return Vec::new() };
        let capital = city.id == leader.capital;
        let mut icons = Vec::new();
        if city.civ == self.viewer {
            // `Leader.Espionage_2`: wonders allowing spy missions, not in anarchy.
            let spies_active = |only| self.count_spy_wonders(city.civ, only, owner) > 0 && leader.government != self.anarchy;
            if self.count_spy_wonders(city.civ, Some(city.id), owner) > 0 && spies_active(None) {
                icons.push(0);
            }
            if capital {
                icons.push(1);
                // `Leader.Espionage_1`: a capital, a tech enabling diplomats, not in anarchy.
                let diplomats = self
                    .tech_flags
                    .iter()
                    .zip(&self.tech_known_by)
                    .any(|(&flags, &known)| flags & L::ATF_ENABLES_DIPLOMATS as u32 != 0 && known & (1u32 << (city.civ & 31)) != 0);
                if leader.capital != -1 && diplomats && leader.government != self.anarchy {
                    icons.push(2);
                }
            }
        } else if capital {
            let civ = (city.civ & 31) as usize;
            if self.viewer_d70[civ] {
                icons.push(0);
            }
            icons.push(1);
            if self.viewer_d50[civ] && !self.viewer_war[civ] {
                icons.push(2);
            }
        }
        icons
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> Rules {
        Rules {
            viewer: 1,
            human_players: 1 << 1,
            difficulty_cost_factor: 12,
            max_town: 6,
            max_city: 12,
            improvements: vec![
                ImprovementRules { name: "Palace".into(), cost: 10, flags: L::ITF_CENTER_OF_EMPIRE as u32, characteristics: L::ITC_SMALL_WONDER as u32, government: -1, obsolete: -1, ..Default::default() },
                ImprovementRules { name: "Temple".into(), cost: 3, characteristics: L::ITC_RELIGIOUS as u32, government: -1, obsolete: -1, ..Default::default() },
                ImprovementRules { name: "Aqueduct".into(), cost: 6, flags: L::ITF_ALLOWS_CITY_LEVEL_2 as u32, government: -1, obsolete: -1, ..Default::default() },
                ImprovementRules { name: "Sewer".into(), cost: 8, flags: L::ITF_ALLOWS_CITY_LEVEL_3 as u32, government: -1, obsolete: -1, ..Default::default() },
            ],
            unit_costs: vec![10, 30],
            optimal_cities: 24,
            anarchy: 0,
            leaders: vec![LeaderRules::default(), LeaderRules { race_bonuses: 1 << L::RB_RELIGIOUS, cities: 6, capital: 0, government: 1, ..Default::default() }],
            ..Default::default()
        }
    }

    fn city() -> CityFacts {
        CityFacts { id: 0, civ: 1, size: 3, production_income: 3, food_income: 2, stored_food: 6, stored_production: 4, order: (1, 1), ..Default::default() }
    }

    #[test]
    fn production_turns_are_the_ceiling_of_what_is_left() {
        let r = rules();
        let mut c = city();
        // A temple: 10 · 3, halved for a religious civ: 15; 4 stored, 3 a turn.
        assert_eq!(r.improvement_cost(1, 1), 15);
        assert_eq!(r.turns_to_build(&c), 4);
        // The palace: 10 · 10 shields, ×3 for few cities.
        assert_eq!(r.improvement_cost(1, 0), 300);
        // A unit: 30 shields (factor 10), none stored.
        c.order = (2, 1);
        c.stored_production = 0;
        assert_eq!(r.turns_to_build(&c), 10);
        // Stored production beyond the cost: one turn.
        c.stored_production = 99;
        assert_eq!(r.turns_to_build(&c), 1);
        c.production_income = 0;
        assert_eq!(r.turns_to_build(&c), NEVER);
    }

    #[test]
    fn growth_turns_follow_the_food_box() {
        let r = rules();
        let mut c = city();
        // A town: box 2 · 1 · 10 = 20, 6 stored, 2 a turn: 7 turns.
        assert_eq!(r.turns_to_grow(&c, &|_| false), 7);
        c.food_income = 0;
        assert_eq!(r.turns_to_grow(&c, &|_| false), NEVER);
        // A full box that may grow: next turn; at the limit without an aqueduct: never.
        c.food_income = 2;
        c.stored_food = 20;
        assert_eq!(r.turns_to_grow(&c, &|_| false), 1);
        c.size = 6;
        assert_eq!(r.turns_to_grow(&c, &|_| false), NEVER);
        // More food than the box holds does not matter: it is full.
        c.stored_food = 40;
        assert_eq!(r.turns_to_grow(&c, &|_| false), 1);
        c.stored_food = 20;
        assert_eq!(r.turns_to_grow(&c, &|i| i == 2), 1);
        c.on_river = true;
        assert_eq!(r.turns_to_grow(&c, &|_| false), 1);
    }

    #[test]
    fn hud_icons_by_capital_and_viewer() {
        let mut r = rules();
        r.viewer_d50[2] = true;
        r.leaders.push(LeaderRules { capital: 7, ..Default::default() });
        let own = |_| Some(1);
        // The viewer's capital: the star; the coin once a tech enables diplomats.
        let c = city();
        assert_eq!(r.hud_icons(&c, &own), vec![1]);
        r.tech_flags = vec![L::ATF_ENABLES_DIPLOMATS as u32];
        r.tech_known_by = vec![1 << 1];
        assert_eq!(r.hud_icons(&c, &own), vec![1, 2]);
        // A foreign capital shows what the viewer knows of its owner; other cities nothing.
        let foreign = CityFacts { id: 7, civ: 2, ..Default::default() };
        assert_eq!(r.hud_icons(&foreign, &own), vec![1, 2]);
        r.viewer_war[2] = true;
        assert_eq!(r.hud_icons(&foreign, &own), vec![1]);
        assert!(r.hud_icons(&CityFacts { id: 8, civ: 2, ..Default::default() }, &own).is_empty());
    }
}
