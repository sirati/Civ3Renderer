//! Reading C3X's districts from the running game, for the world snapshot.
//!
//! Costs nothing without C3X (or a C3X of another release than the one the
//! layouts are for) and next to nothing while its districts are off: the
//! state is looked for once, and then only its switches are read.

use std::cell::OnceCell;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use civ3_sys::build::IMAGE_BASE;
use civ3_sys::c3x::{C3xState, District, DistrictConfigs};
use civ3_sys::{Civ3, MapInfo};

use crate::world::Civ3Improvement;

/// What the world needs of C3X.
#[derive(Clone, Debug, Default)]
pub struct DistrictState {
    pub configs: Arc<DistrictConfigs>,
    pub districts: Vec<District>,
    /// By improvement id.
    pub improvements: Vec<Civ3Improvement>,
    /// By improvement id: a wonder or small wonder.
    pub wonders: Vec<bool>,
    /// By civ, the keys of the buildings wonders give it for free.
    pub free_improvements: Vec<Vec<i32>>,
}

impl DistrictState {
    pub fn is_wonder(&self, improvement: i32) -> bool {
        usize::try_from(improvement).ok().and_then(|i| self.wonders.get(i)).copied().unwrap_or(false)
    }
}

/// C3X's state in this process: found once (`None` inside: there is none).
fn c3x_state() -> Option<C3xState> {
    // (A failed search is tried again; two threads racing find the same.)
    static STATE: OnceLock<Option<C3xState>> = OnceLock::new();
    if let Some(found) = STATE.get() {
        return *found;
    }
    match C3xState::find(IMAGE_BASE) {
        Ok(found) => *STATE.get_or_init(|| found),
        Err(e) => {
            log::debug!("looking for C3X's state: {e}");
            None
        }
    }
}

/// The configs C3X holds, read again only when they change: what they were
/// read from, then the names of the improvements their wonders were matched
/// with.
fn configs(state: &C3xState, names: &[String]) -> Option<Arc<DistrictConfigs>> {
    static CACHE: handoff::Latest<((u64, u64), Arc<DistrictConfigs>)> = handoff::Latest::new();
    let key = (state.fingerprint().ok()?, {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        names.hash(&mut hasher);
        hasher.finish()
    });
    if let Some(cached) = CACHE.load()
        && cached.0 == key
    {
        return Some(cached.1.clone());
    }
    let mut configs = state.configs().inspect_err(|e| log::debug!("reading C3X's district configs: {e}")).ok()?;
    configs.resolve_wonders(names);
    let configs = Arc::new(configs);
    log::info!(
        "C3X districts: {} kinds, {} wonders, {} natural wonders, {:?}",
        configs.districts.len(),
        configs.wonders.len(),
        configs.natural_wonders.len(),
        configs.flags
    );
    CACHE.publish((key, configs.clone()));
    Some(configs)
}

/// C3X's districts now. Empty (and `configs.flags` all off) when C3X has none
/// or does not draw them.
pub fn read(game: &Civ3, map: &MapInfo) -> DistrictState {
    static OFF: OnceLock<Arc<DistrictConfigs>> = OnceLock::new();
    let off = || DistrictState { configs: OFF.get_or_init(Default::default).clone(), ..Default::default() };
    let Some(state) = c3x_state() else { return off() };
    match state.flags() {
        Ok(flags) if flags.any() => {}
        Ok(_) => return off(),
        Err(e) => {
            log::debug!("reading C3X's switches: {e}");
            return off();
        }
    }
    let Ok(improvements) = game.improvements().inspect_err(|e| log::debug!("reading the improvements: {e}")) else { return off() };
    let names: Vec<String> = improvements.iter().map(|i| i.name.clone()).collect();
    let Some(configs) = configs(&state, &names) else { return off() };

    // A district knows its tile's coordinates, and it is the tile's pointer
    // that C3X's table is keyed by: believe the coordinates only if the
    // pointer agrees.
    let pointers: OnceCell<Vec<u32>> = OnceCell::new();
    let by_pointer: OnceCell<HashMap<u32, usize>> = OnceCell::new();
    let locate = |key: u32, x: i32, y: i32| {
        let pointers = pointers.get_or_init(|| game.tile_pointers(map).unwrap_or_default());
        let on_map = (0..map.width).contains(&x) && (0..map.height).contains(&y) && (x + y) % 2 == 0;
        if on_map && pointers.get(map.tile_index(x, y)) == Some(&key) {
            return Some((x, y));
        }
        let index = *by_pointer.get_or_init(|| pointers.iter().enumerate().map(|(i, &p)| (p, i)).collect()).get(&key)? as i32;
        let per_row = (map.width / 2).max(1);
        let y = index / per_row;
        Some(((index % per_row) * 2 + (y & 1), y))
    };
    let districts = state
        .districts(locate, configs.districts.len())
        .inspect_err(|e| log::debug!("reading C3X's districts: {e}"))
        .unwrap_or_default();

    let free_improvements = (0..32).map(|civ| game.free_improvements(civ).unwrap_or_default()).collect();

    DistrictState {
        configs,
        districts,
        free_improvements,
        wonders: improvements.iter().map(|i| i.wonder).collect(),
        improvements: improvements
            .iter()
            .map(|i| Civ3Improvement { government: Some(i.government).filter(|&g| g >= 0), obsolete_for: i.obsolete_for })
            .collect(),
    }
}
