use std::{
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
    time::Duration,
};

use crate::Timestamp;

/// LT1: how far back delivery delays are remembered.
pub(super) const HISTORY: Duration = Duration::from_secs(120);

/// LT1: the history keeps the smallest and the largest delay per slot of this length.
const SLOT: Duration = Duration::from_millis(250);

const HISTORY_SLOTS: i64 = (HISTORY.as_millis() / SLOT.as_millis()) as i64;

/// LT3: two occasions further apart do not make a level regular (`repeat_max`).
const REPEAT_MAX_SLOTS: i64 = HISTORY_SLOTS / 2;

/// LT4: a level stays regular at least this long after its latest occasion.
const KEEP_MIN: Duration = Duration::from_secs(10);
const KEEP_MIN_SLOTS: i64 = (KEEP_MIN.as_millis() / SLOT.as_millis()) as i64;

/// LT5: an arrival gap at least this long makes the next chunk a first sight.
const FIRST_SIGHT_GAP: Duration = Duration::from_millis(500);

/// Smallest and largest delivery delay among the chunks that arrived in one slot.
#[derive(Clone, Copy)]
struct Slot {
    max: Timestamp,
    min: Timestamp,
}

/// LT2: one occasion on which a level was reached.
struct Occasion {
    /// Newest slot of the occasion.
    end: i64,
    /// Largest delay during the occasion.
    peak: Timestamp,
}

/// What was last read from the history, and for which slot and edge.
#[derive(Clone, Copy)]
struct Cached {
    slot: i64,
    edge_key: i64,
    level: Timestamp,
    lasts: Duration,
}

/// Lateness that repeats: a level of delivery delay that was reached twice is regular, and it
/// stays regular for twice the longest time between two occasions.
pub(super) struct Lateness {
    tolerance: Duration,
    segmented: bool,
    slots: BTreeMap<i64, Slot>,
    /// LT5: `(arrival, delay)` of the first chunk after the latest arrival gap.
    first_sight: Option<(Timestamp, Timestamp)>,
    cache: Option<Cached>,
}

impl Lateness {
    pub fn new(tolerance: Duration, segmented: bool) -> Self {
        Self {
            tolerance,
            segmented,
            slots: BTreeMap::new(),
            first_sight: None,
            cache: None,
        }
    }

    /// `gap`: time since the previous chunk of the track.
    pub fn observe(&mut self, arrival: Timestamp, delay: Timestamp, gap: Duration) {
        match self.slots.entry(slot_index(arrival)) {
            Entry::Vacant(entry) => {
                entry.insert(Slot {
                    max: delay,
                    min: delay,
                });
                self.cache = None;
            }
            Entry::Occupied(mut entry) => {
                let slot = entry.get_mut();
                if delay > slot.max || delay < slot.min {
                    slot.max = Timestamp::max(slot.max, delay);
                    slot.min = Timestamp::min(slot.min, delay);
                    self.cache = None;
                }
            }
        }
        if self.segmented && gap >= FIRST_SIGHT_GAP {
            self.first_sight = Some((arrival, delay));
        }
    }

    /// LT7: the edge rose by `delta` (negative: a rise is taken back), and what was measured
    /// before `before` moves with it.
    pub fn shift(&mut self, delta: Timestamp, before: Timestamp) {
        for (_, slot) in self.slots.range_mut(..slot_index(before)) {
            slot.max += delta;
            slot.min += delta;
        }
        if let Some((arrival, delay)) = &mut self.first_sight
            && *arrival < before
        {
            *delay += delta;
        }
        self.cache = None;
    }

    /// `(L, A)`: how far above `edge` the delivery delay gets regularly, and how long delivery
    /// regularly stays away from the edge.
    pub fn regular(&mut self, now: Timestamp, edge: Timestamp) -> (Timestamp, Duration) {
        let slot = slot_index(now);
        // reading the history is expensive, and a change of the edge this small changes nothing
        let edge_key = edge.as_micros().div_euclid(100);
        let cached = match self.cache {
            Some(cached) if cached.slot == slot && cached.edge_key == edge_key => cached,
            _ => {
                let (level, lasts) = self.read_history(slot, edge);
                let cached = Cached {
                    slot,
                    edge_key,
                    level,
                    lasts,
                };
                self.cache = Some(cached);
                cached
            }
        };
        let (mut level, lasts) = (cached.level, cached.lasts);

        // LT5
        if let Some((arrival, delay)) = self.first_sight {
            let keep = Timestamp::max(KEEP_MIN.into(), (delay - edge) * 2);
            if arrival + keep >= now {
                level = Timestamp::max(level, delay);
            }
        }
        (level - edge, lasts)
    }

    /// The highest regular level of delay, and the longest regular time away from the edge.
    fn read_history(&mut self, now_slot: i64, edge: Timestamp) -> (Timestamp, Duration) {
        self.slots = self.slots.split_off(&(now_slot - HISTORY_SLOTS));
        let slots: Vec<(i64, Slot)> = self.slots.iter().map(|(i, slot)| (*i, *slot)).collect();

        // LT6: the highest level that is regular
        let mut regular_level = edge;
        let levels: BTreeSet<Timestamp> = slots
            .iter()
            .map(|(_, slot)| slot.max)
            .filter(|max| *max > edge)
            .collect();
        for level in levels.into_iter().rev() {
            let margin = Timestamp::max(Timestamp::from_millis(10), (level - edge) / 10i64);
            let occasions = occasions(&slots, level, margin);
            let ends: Vec<i64> = occasions.iter().map(|occasion| occasion.end).collect();
            if let Some(last) = ends.last()
                && is_kept(&ends, *last, now_slot)
            {
                // what two of them reached: the second highest
                let mut peaks: Vec<Timestamp> =
                    occasions.iter().map(|occasion| occasion.peak).collect();
                peaks.sort();
                regular_level = Timestamp::min(level, peaks[peaks.len() - 2]);
                break;
            }
        }

        // LT9, how long delivery stays away from the edge: the same rule on durations.
        // A stretch is `(slot with a chunk at the edge before it, the one after it)`.
        let mut stretches: Vec<(i64, i64)> = Vec::new();
        let mut last_at_edge: Option<usize> = None;
        for (k, (index, slot)) in slots.iter().enumerate() {
            if slot.min <= edge + self.tolerance {
                if let Some(last) = last_at_edge
                    && k - last > 1
                {
                    stretches.push((slots[last].0, *index));
                }
                last_at_edge = Some(k);
            }
        }
        // the stretch in progress counts
        if let Some(last) = last_at_edge
            && last + 1 < slots.len()
        {
            stretches.push((slots[last].0, now_slot));
        }

        let mut regular_length = 0;
        let lengths: BTreeSet<i64> = stretches.iter().map(|(start, end)| end - start).collect();
        for length in lengths.into_iter().rev() {
            let margin = i64::max(1, length / 10);
            let reached: Vec<(i64, i64)> = stretches
                .iter()
                .copied()
                .filter(|(start, end)| end - start >= length - margin)
                .collect();
            let starts: Vec<i64> = reached.iter().map(|(start, _)| *start).collect();
            if let Some((_, last_end)) = reached.last()
                && is_kept(&starts, *last_end, now_slot)
            {
                let mut lengths: Vec<i64> =
                    reached.iter().map(|(start, end)| end - start).collect();
                lengths.sort();
                regular_length = i64::min(length, lengths[lengths.len() - 2]);
                break;
            }
        }

        (regular_level, SLOT * regular_length as u32)
    }
}

fn slot_index(time: Timestamp) -> i64 {
    time.as_nanos().div_euclid(SLOT.as_nanos() as i64)
}

/// LT2: every separate occasion on which `level` was reached (within `margin`), oldest first.
fn occasions(mut slots: &[(i64, Slot)], level: Timestamp, margin: Timestamp) -> Vec<Occasion> {
    let mut found = Vec::new();
    loop {
        // the newest slot left that reached the level
        while let Some(((_, slot), older)) = slots.split_last()
            && slot.max < level - margin
        {
            slots = older;
        }
        let Some(((end, slot), older)) = slots.split_last() else {
            break;
        };
        let mut occasion = Occasion {
            end: *end,
            peak: slot.max,
        };
        slots = older;

        // It reaches back to the first slot in which delivery was clearly faster, that slot
        // included; before it is an older occasion.
        while let Some(((_, slot), older)) = slots.split_last() {
            occasion.peak = Timestamp::max(occasion.peak, slot.max);
            slots = older;
            if slot.min < level - margin * 2 {
                break;
            }
        }
        found.push(occasion);
    }
    found.reverse();
    found
}

/// LT3, LT4: occasions at the slots `at`: whether the latest two are close enough, and `last`
/// is not longer ago than twice the longest time between two of them.
fn is_kept(at: &[i64], last: i64, now_slot: i64) -> bool {
    let [.., before_latest, latest] = at else {
        return false;
    };
    if latest - before_latest > REPEAT_MAX_SLOTS {
        return false;
    }
    let longest = at
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .filter(|apart| *apart <= REPEAT_MAX_SLOTS)
        .max()
        .unwrap_or(0);
    now_slot - last <= i64::max(KEEP_MIN_SLOTS, 2 * longest)
}
