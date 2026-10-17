//! The set of data centers a player may buy from, derived from settings.

use crate::error::{AppError, AppResult};
use crate::settings::Settings;
use crate::worlds::World;

#[derive(Debug, Clone)]
pub struct ScopeDc {
    pub name: String,
    /// World used for `/aggregated/{world}` queries: the home world for the
    /// home DC (so the same call yields the home anchor), else the lowest id.
    pub query_world: World,
    pub is_home_dc: bool,
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub home: World,
    pub dcs: Vec<ScopeDc>,
    /// Every world of every DC in scope.
    pub worlds: Vec<World>,
}

impl Scope {
    pub fn world(&self, world_id: i64) -> Option<&World> {
        self.worlds.iter().find(|w| w.id == world_id)
    }
}

pub fn build(settings: &Settings, worlds: &[World]) -> AppResult<Scope> {
    let home_id = settings
        .home_world_id
        .ok_or_else(|| AppError::Invalid("no home world selected".into()))?;
    let home = worlds
        .iter()
        .find(|w| w.id == home_id)
        .cloned()
        .ok_or_else(|| AppError::Invalid(format!("home world {home_id} not found")))?;

    let in_scope =
        |w: &World| w.region == home.region || settings.extra_regions.contains(&w.region);
    let scope_worlds: Vec<World> = worlds.iter().filter(|w| in_scope(w)).cloned().collect();

    let mut dc_names: Vec<&str> = scope_worlds.iter().map(|w| w.dc.as_str()).collect();
    dc_names.sort_unstable();
    dc_names.dedup();

    let dcs = dc_names
        .into_iter()
        .filter_map(|dc| {
            let members: Vec<&World> = scope_worlds.iter().filter(|w| w.dc == dc).collect();
            let is_home_dc = dc == home.dc;
            let query_world = if is_home_dc {
                home.clone()
            } else {
                (*members.iter().min_by_key(|w| w.id)?).clone()
            };
            Some(ScopeDc {
                name: dc.to_string(),
                query_world,
                is_home_dc,
            })
        })
        .collect();

    Ok(Scope {
        home,
        dcs,
        worlds: scope_worlds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worlds;

    #[test]
    fn chaos_home_with_oceania() {
        let s = Settings {
            home_world_id: Some(83),
            extra_regions: vec!["Oceania".into()],
            ..Settings::default()
        };
        let scope = build(&s, &worlds::bundled()).unwrap();
        let names: Vec<(&str, bool)> = scope
            .dcs
            .iter()
            .map(|d| (d.name.as_str(), d.is_home_dc))
            .collect();
        assert_eq!(
            names,
            vec![("Chaos", true), ("Light", false), ("Materia", false)]
        );
        assert_eq!(
            scope.dcs[0].query_world.id, 83,
            "home DC is queried through the home world"
        );
        assert_eq!(scope.world(33).unwrap().dc, "Light");
    }
}
