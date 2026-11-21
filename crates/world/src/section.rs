use crate::BlockStateId;

pub const SECTION_EDGE: usize = 16;
pub const SECTION_VOLUME: usize = SECTION_EDGE * SECTION_EDGE * SECTION_EDGE;

const VOLUME: usize = SECTION_VOLUME;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Section {
    Uniform(BlockStateId),
    Paletted(Palette),
}

impl Section {
    pub fn uniform(state: BlockStateId) -> Self {
        Self::Uniform(state)
    }

    pub fn get(&self, index: usize) -> BlockStateId {
        assert!(index < VOLUME);
        match self {
            Self::Uniform(state) => *state,
            Self::Paletted(palette) => palette.states[palette.read(index)],
        }
    }

    pub fn set(&mut self, index: usize, state: BlockStateId) {
        if self.get(index) == state {
            return;
        }
        if let Self::Uniform(previous) = *self {
            *self = Self::Paletted(Palette {
                states: vec![previous],
                bits: 1,
                words: vec![0; VOLUME / 64],
            });
        }
        if let Self::Paletted(palette) = self {
            palette.set(index, state);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    states: Vec<BlockStateId>,
    bits: usize,
    words: Vec<u64>,
}

impl Palette {
    fn read(&self, index: usize) -> usize {
        let bit = index * self.bits;
        let word = bit / 64;
        let shift = bit % 64;
        let mut value = self.words[word] >> shift;
        if shift + self.bits > 64 {
            value |= self.words[word + 1] << (64 - shift);
        }
        (value & ((1 << self.bits) - 1)) as usize
    }

    fn write(&mut self, index: usize, value: usize) {
        let bit = index * self.bits;
        let word = bit / 64;
        let shift = bit % 64;
        let mask = (1_u64 << self.bits) - 1;
        self.words[word] = (self.words[word] & !(mask << shift)) | ((value as u64) << shift);
        if shift + self.bits > 64 {
            let remaining = 64 - shift;
            self.words[word + 1] &= !(mask >> remaining);
            self.words[word + 1] |= (value as u64) >> remaining;
        }
    }

    fn set(&mut self, index: usize, state: BlockStateId) {
        let entry = match self.states.iter().position(|existing| *existing == state) {
            Some(entry) => entry,
            None => {
                let entry = self.states.len();
                self.states.push(state);
                if entry == 1 << self.bits {
                    let previous = self.clone();
                    self.bits += 1;
                    self.words = vec![0; (VOLUME * self.bits).div_ceil(64)];
                    for slot in 0..VOLUME {
                        self.write(slot, previous.read(slot));
                    }
                }
                entry
            }
        };
        self.write(index, entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widens_palette_and_preserves_cross_word_entries() {
        let empty = BlockStateId::from_registry_index(0);
        let mut section = Section::uniform(empty);
        for index in 0..VOLUME {
            section.set(
                index,
                BlockStateId::from_registry_index((index % 37) as u16),
            );
        }
        section.set(21, empty);
        for index in 0..VOLUME {
            let expected = if index == 21 { 0 } else { (index % 37) as u16 };
            assert_eq!(
                section.get(index),
                BlockStateId::from_registry_index(expected)
            );
        }
    }
}
