use bevy_ecs::prelude::Component;
use rustcraft_world::BlockStateId;

use crate::Rejection;

pub const SLOT_COUNT: usize = 8;
pub const JAVA_HOTBAR_COUNT: usize = 9;
pub const JAVA_SLOT_COUNT: usize = 36;
pub const STACK_LIMIT: u16 = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemStack {
    pub block: BlockStateId,
    pub count: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryAction {
    Select {
        slot: usize,
    },
    Transfer {
        from: usize,
        to: usize,
        count: u16,
    },
    Click {
        slot: usize,
        button: InventoryButton,
    },
    ReturnCarried,
    Catalog {
        block: BlockStateId,
        stack: bool,
    },
    DiscardCarried,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryButton {
    Left,
    Right,
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Inventory {
    pub(crate) slots: Vec<Option<ItemStack>>,
    pub(crate) selected: usize,
    pub(crate) revision: u64,
    pub(crate) carried: Option<ItemStack>,
}

impl Inventory {
    pub(crate) fn empty(count: usize) -> Self {
        Self {
            slots: vec![None; count],
            selected: 0,
            revision: 1,
            carried: None,
        }
    }

    pub(crate) fn new(starter: [BlockStateId; 3]) -> Self {
        let mut inventory = Self::empty(SLOT_COUNT);
        for (index, (block, count)) in starter.into_iter().zip([16, 8, 8]).enumerate() {
            inventory.slots[index] = Some(ItemStack { block, count });
        }
        inventory
    }

    pub fn slots(&self) -> &[Option<ItemStack>] {
        &self.slots
    }

    pub fn carried(&self) -> Option<ItemStack> {
        self.carried
    }

    pub fn hotbar(&self) -> &[Option<ItemStack>] {
        &self.slots[..self.hotbar_count()]
    }

    fn hotbar_count(&self) -> usize {
        if self.slots.len() == JAVA_SLOT_COUNT {
            JAVA_HOTBAR_COUNT
        } else {
            SLOT_COUNT
        }
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub(crate) fn selected_item(&self) -> Option<ItemStack> {
        self.slots[self.selected]
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn apply_v1(
        &mut self,
        action: InventoryAction,
        revision: u64,
    ) -> Result<bool, Rejection> {
        if self.revision != revision {
            return Err(Rejection::StaleInventory {
                actual: self.revision,
            });
        }
        match action {
            InventoryAction::Select { slot } if slot < self.hotbar_count() => {
                if slot == self.selected {
                    return Ok(false);
                }
                self.selected = slot;
            }
            InventoryAction::Transfer { from, to, count }
                if from < self.slots.len() && to < self.slots.len() && from != to && count > 0 =>
            {
                let source = self.slots[from]
                    .filter(|source| source.count >= count)
                    .ok_or(Rejection::InvalidInventoryAction)?;
                let existing = self.slots[to];
                let quantity = existing.map_or(count, |dest| dest.count + count);
                if existing.is_some_and(|dest| dest.block != source.block) || quantity > STACK_LIMIT
                {
                    return Err(Rejection::InvalidInventoryAction);
                }
                self.slots[to] = Some(ItemStack {
                    block: source.block,
                    count: quantity,
                });
                let remaining = source.count - count;
                self.slots[from] = (remaining > 0).then_some(ItemStack {
                    count: remaining,
                    ..source
                });
            }
            _ => return Err(Rejection::InvalidInventoryAction),
        }
        self.advance();
        Ok(true)
    }

    pub(crate) fn apply_java(
        &mut self,
        action: InventoryAction,
        revision: u64,
    ) -> Result<bool, Rejection> {
        if self.revision != revision {
            return Err(Rejection::StaleInventory {
                actual: self.revision,
            });
        }
        match action {
            InventoryAction::Click { slot, button } => {
                let target = self
                    .slots
                    .get_mut(slot)
                    .ok_or(Rejection::InvalidInventoryAction)?;
                let before = (*target, self.carried);
                match (target.as_mut(), self.carried.as_mut(), button) {
                    (Some(target), None, InventoryButton::Right) => {
                        let count = target.count.div_ceil(2);
                        self.carried = Some(ItemStack { count, ..*target });
                        target.count -= count;
                    }
                    (None, Some(carried), InventoryButton::Right) => {
                        self.slots[slot] = Some(ItemStack {
                            count: 1,
                            ..*carried
                        });
                        carried.count -= 1;
                    }
                    (Some(target), Some(carried), _) if target.block == carried.block => {
                        let amount = if button == InventoryButton::Right {
                            1
                        } else {
                            carried.count
                        };
                        let count = amount.min(STACK_LIMIT - target.count);
                        target.count += count;
                        carried.count -= count;
                    }
                    _ => std::mem::swap(&mut self.slots[slot], &mut self.carried),
                }
                self.slots[slot] = self.slots[slot].filter(|item| item.count > 0);
                self.carried = self.carried.filter(|item| item.count > 0);
                if before == (self.slots[slot], self.carried) {
                    return Ok(false);
                }
                self.advance();
                Ok(true)
            }
            InventoryAction::ReturnCarried => {
                let Some(carried) = self.carried else {
                    return Ok(false);
                };
                self.give(carried.block, carried.count)?;
                self.carried = None;
                Ok(true)
            }
            _ => self.apply_v1(action, revision),
        }
    }

    pub(crate) fn pick(&mut self, block: BlockStateId) -> Result<bool, Rejection> {
        let slot = self
            .slots
            .iter()
            .position(|entry| entry.is_some_and(|item| item.block == block))
            .ok_or(Rejection::InvalidInventoryAction)?;
        if slot == self.selected {
            return Ok(false);
        }
        if slot < self.hotbar_count() {
            self.selected = slot;
        } else {
            self.selected = self
                .hotbar()
                .iter()
                .position(Option::is_none)
                .unwrap_or(self.selected);
            self.slots.swap(slot, self.selected);
        }
        self.advance();
        Ok(true)
    }

    pub(crate) fn creative_pick(&mut self, block: BlockStateId) -> bool {
        if self.selected_item().is_some_and(|item| item.block == block) {
            return false;
        }
        match self.pick(block) {
            Ok(changed) => changed,
            Err(_) => {
                self.selected = self
                    .hotbar()
                    .iter()
                    .position(Option::is_none)
                    .unwrap_or(self.selected);
                self.slots[self.selected] = Some(ItemStack { block, count: 1 });
                self.advance();
                true
            }
        }
    }

    pub(crate) fn set_carried(
        &mut self,
        stack: Option<ItemStack>,
        revision: u64,
    ) -> Result<bool, Rejection> {
        if revision != self.revision {
            return Err(Rejection::StaleInventory {
                actual: self.revision,
            });
        }
        if self.carried == stack {
            return Ok(false);
        }
        self.carried = stack;
        self.advance();
        Ok(true)
    }

    pub(crate) fn give(&mut self, block: BlockStateId, mut quantity: u16) -> Result<(), Rejection> {
        let room: u16 = self
            .slots
            .iter()
            .map(|slot| match slot {
                None => STACK_LIMIT,
                Some(item) if item.block == block => STACK_LIMIT - item.count,
                _ => 0,
            })
            .sum();
        if quantity == 0 || quantity > room {
            return Err(Rejection::InventoryFull);
        }
        for slot in self
            .slots
            .iter_mut()
            .filter_map(Option::as_mut)
            .filter(|item| item.block == block)
        {
            let count = quantity.min(STACK_LIMIT - slot.count);
            slot.count += count;
            quantity -= count;
        }
        for slot in self.slots.iter_mut().filter(|slot| slot.is_none()) {
            if quantity == 0 {
                break;
            }
            let count = quantity.min(STACK_LIMIT);
            *slot = Some(ItemStack { block, count });
            quantity -= count;
        }
        self.advance();
        Ok(())
    }

    pub(crate) fn take_selected_v1(&mut self) -> Result<BlockStateId, Rejection> {
        let mut stack = self.slots[self.selected].ok_or(Rejection::EmptySelectedSlot)?;
        stack.count -= 1;
        self.slots[self.selected] = (stack.count > 0).then_some(stack);
        self.advance();
        Ok(stack.block)
    }

    pub(crate) fn insert_v1(&mut self, block: BlockStateId) -> Result<(), Rejection> {
        let slot = self
            .slots
            .iter()
            .position(|slot| {
                slot.is_some_and(|stack| stack.block == block && stack.count < STACK_LIMIT)
            })
            .or_else(|| self.slots.iter().position(Option::is_none))
            .ok_or(Rejection::InventoryFull)?;
        let count = self.slots[slot].map_or(1, |stack| stack.count + 1);
        self.slots[slot] = Some(ItemStack { block, count });
        self.advance();
        Ok(())
    }

    fn advance(&mut self) {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("inventory revision exhausted");
    }
}
