use crate::sstable_manager::{Level, SSTableManager};


#[derive(Debug, Clone)]
pub struct CompactionCandidate {
    pub input_level: Level,
    pub output_level: Level,

    pub input_tables: Vec<usize>,
    pub output_tables: Vec<usize>,
}

pub struct CompactionPicker;

impl CompactionPicker {
    pub fn pick_l0(
        manager: &SSTableManager,
    ) -> Option<CompactionCandidate> {

        let inputs = manager.find_size_tiered_candidates();

        if inputs.is_empty() {
            return None;
        }

        Some(CompactionCandidate {
            input_level: Level::L0,
            output_level: Level::L1,
            input_tables: inputs,
            output_tables: Vec::new(),
        })
    }

    pub fn pick_l1(
        manager: &SSTableManager,
    ) -> Option<CompactionCandidate> {

        if manager.l1.len() < 4 {
            return None;
        }

        let table = &manager.l1[0];

        let overlaps = manager
            .l2
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                t.overlaps(&table.min_key, &table.max_key)
            })
            .map(|(i, _)| i)
            .collect();

        Some(CompactionCandidate {
            input_level: Level::L1,
            output_level: Level::L2,
            input_tables: vec![0],
            output_tables: overlaps,
        })
    }
}   