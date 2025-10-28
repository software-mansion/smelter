use std::collections::{HashMap, HashSet};

use crate::scene::{
    ComponentId,
    types::interpolation::{ContinuousValue, InterpolationState},
};

use super::tiles::Tile;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(super) enum TileId {
    ComponentId(ComponentId),
    Index(usize),
}

impl ContinuousValue for Vec<Option<Tile>> {
    fn interpolate(start: &Self, end: &Self, state: InterpolationState) -> Self {
        let start_id_map: HashMap<&TileId, usize> = start
            .iter()
            .enumerate()
            .filter_map(|(index, tile)| tile.as_ref().map(|tile| (&tile.id, index)))
            .collect();
        let end_id_set: HashSet<&TileId> = end
            .iter()
            .flat_map(|tile| tile.as_ref().map(|tile| &tile.id))
            .collect();

        if state.0 >= 1.0 {
            return end.clone();
        };

        end.iter()
            .map(|tile| {
                let tile = tile.as_ref()?;
                start_id_map
                    .get(&tile.id)
                    .and_then(|index| start.get(*index))
                    .and_then(|old_tile| {
                        old_tile
                            .as_ref()
                            .map(|old_tile| ContinuousValue::interpolate(old_tile, tile, state))
                    })
                    .or_else(|| {
                        for start_tile in start.iter().flatten() {
                            if are_positions_equal(tile, start_tile) {
                                if !end_id_set.contains(&start_tile.id) {
                                    return Some(ContinuousValue::interpolate(tile, tile, state));
                                } else {
                                    return None;
                                }
                            }
                        }
                        None
                    })
            })
            .collect()
    }
}

impl ContinuousValue for Tile {
    fn interpolate(start: &Self, end: &Self, state: InterpolationState) -> Self {
        Self {
            id: end.id.clone(),
            top: ContinuousValue::interpolate(&start.top, &end.top, state),
            left: ContinuousValue::interpolate(&start.left, &end.left, state),
            width: ContinuousValue::interpolate(&start.width, &end.width, state),
            height: ContinuousValue::interpolate(&start.height, &end.height, state),
        }
    }
}

fn are_positions_equal(lhs: &Tile, rhs: &Tile) -> bool {
    const TOLERANCE: f32 = 0.001;

    let top_eq = f32::abs(lhs.top - rhs.top) <= TOLERANCE;
    let left_eq = f32::abs(lhs.left - rhs.left) <= TOLERANCE;
    let width_eq = f32::abs(lhs.width - rhs.width) <= TOLERANCE;
    let height_eq = f32::abs(lhs.height - rhs.height) <= TOLERANCE;

    top_eq && left_eq && width_eq && height_eq
}
