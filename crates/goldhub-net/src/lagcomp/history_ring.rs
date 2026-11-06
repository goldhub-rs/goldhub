//! Circular entity state history buffer for sub-tick lag compensation.

pub const MAX_LAGCOMP_ENTITIES: usize = 32;
pub const HISTORY_SLOTS_PER_ENTITY: usize = 128;
const HISTORY_MASK: usize = HISTORY_SLOTS_PER_ENTITY - 1;

/// Bounded entity bounding box and spatial transform snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EntitySnapshot {
    pub entity_id: u32,
    pub timestamp_micros: u64,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
}

impl EntitySnapshot {
    /// Returns the world-space bounding box minimum corner.
    #[inline(always)]
    pub fn world_mins(&self) -> [f32; 3] {
        [
            self.origin[0] + self.mins[0],
            self.origin[1] + self.mins[1],
            self.origin[2] + self.mins[2],
        ]
    }

    /// Returns the world-space bounding box maximum corner.
    #[inline(always)]
    pub fn world_maxs(&self) -> [f32; 3] {
        [
            self.origin[0] + self.maxs[0],
            self.origin[1] + self.maxs[1],
            self.origin[2] + self.maxs[2],
        ]
    }
}

/// Circular history storage for server-side / client-side lag compensation.
pub struct LagCompHistoryRing {
    snapshots: Box<[[EntitySnapshot; HISTORY_SLOTS_PER_ENTITY]]>,
    head_indices: [usize; MAX_LAGCOMP_ENTITIES],
}

impl Default for LagCompHistoryRing {
    fn default() -> Self {
        Self::new()
    }
}

impl LagCompHistoryRing {
    /// Creates a new empty lag compensation history buffer pre-allocated on the heap.
    pub fn new() -> Self {
        const DEFAULT_SNAP: EntitySnapshot = EntitySnapshot {
            entity_id: 0,
            timestamp_micros: 0,
            origin: [0.0; 3],
            angles: [0.0; 3],
            mins: [0.0; 3],
            maxs: [0.0; 3],
        };
        let boxed =
            vec![[DEFAULT_SNAP; HISTORY_SLOTS_PER_ENTITY]; MAX_LAGCOMP_ENTITIES].into_boxed_slice();
        Self {
            snapshots: boxed,
            head_indices: [0; MAX_LAGCOMP_ENTITIES],
        }
    }

    /// Records an authoritative entity spatial state at the given wall-clock timestamp.
    pub fn record_snapshot(&mut self, snap: EntitySnapshot) {
        let ent_idx = (snap.entity_id as usize) % MAX_LAGCOMP_ENTITIES;
        let head = self.head_indices[ent_idx];
        self.snapshots[ent_idx][head] = snap;
        self.head_indices[ent_idx] = (head + 1) & HISTORY_MASK;
    }

    /// Finds the two adjacent historical snapshots enclosing `target_timestamp_micros` for sub-tick interpolation.
    pub fn find_enclosing_snapshots(
        &self,
        entity_id: u32,
        target_timestamp_micros: u64,
    ) -> Option<(EntitySnapshot, EntitySnapshot)> {
        let ent_idx = (entity_id as usize) % MAX_LAGCOMP_ENTITIES;
        let buffer = &self.snapshots[ent_idx];

        // Search backward from head
        let mut prev_snap: Option<EntitySnapshot> = None;

        for offset in 0..HISTORY_SLOTS_PER_ENTITY {
            let idx = (self.head_indices[ent_idx].wrapping_sub(1 + offset)) & HISTORY_MASK;
            let snap = buffer[idx];

            if snap.timestamp_micros == 0 {
                break;
            }

            if snap.timestamp_micros <= target_timestamp_micros {
                if let Some(next) = prev_snap {
                    return Some((snap, next));
                }
                // Exactly matches or is newest
                return Some((snap, snap));
            }

            prev_snap = Some(snap);
        }

        prev_snap.map(|s| (s, s))
    }
}
