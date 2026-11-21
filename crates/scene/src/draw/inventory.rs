use super::text::label;
use super::{ItemIcon, Panel, Vertex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryHit {
    Player(usize),
    Catalog(usize),
    Delete,
}

pub struct InventoryLayout {
    size: [f32; 2],
    edge: f32,
    left: f32,
    top: f32,
    slot_count: usize,
    catalog_count: Option<usize>,
}

impl InventoryLayout {
    pub fn new(size: [u32; 2], slot_count: usize, catalog_count: Option<usize>) -> Self {
        let [width, height] = size.map(|value| value as f32);
        let main_rows = slot_count.saturating_sub(9).div_ceil(9);
        let catalog_rows = catalog_count.map_or(0, |count| count.div_ceil(9) + 1);
        let rows = main_rows + catalog_rows + 1;
        let edge = (width / 11.5)
            .min(height / (rows as f32 + 2.3))
            .clamp(12.0, 52.0);
        let left = (width - 9.0 * edge) / 2.0;
        let top = (height - rows as f32 * edge) / 2.0;
        Self {
            size: [width, height],
            edge,
            left,
            top,
            slot_count,
            catalog_count,
        }
    }

    fn catalog_rows(&self) -> usize {
        self.catalog_count.map_or(0, |count| count.div_ceil(9))
    }

    fn slot(&self, hit: InventoryHit) -> [f32; 4] {
        let (column, row, spacing) = match hit {
            InventoryHit::Catalog(index) => (index % 9, index / 9, 0.0),
            InventoryHit::Player(index) if index < 9 => (
                index,
                self.catalog_rows() + self.slot_count.saturating_sub(9).div_ceil(9),
                if self.catalog_count.is_some() {
                    0.45
                } else {
                    0.2
                },
            ),
            InventoryHit::Player(index) => (
                index % 9,
                self.catalog_rows() + (index - 9) / 9,
                if self.catalog_count.is_some() {
                    0.30
                } else {
                    0.0
                },
            ),
            InventoryHit::Delete => (
                8,
                self.catalog_rows() + self.slot_count.saturating_sub(9).div_ceil(9) + 1,
                0.45,
            ),
        };
        let x = self.left + column as f32 * self.edge;
        let y = self.top + (row as f32 + spacing) * self.edge;
        [x, y, self.edge * 0.9, self.edge * 0.9]
    }

    pub fn hit(&self, cursor: [f64; 2]) -> Option<InventoryHit> {
        let [cx, cy] = cursor.map(|value| value as f32);
        (0..self.slot_count)
            .map(InventoryHit::Player)
            .chain((0..self.catalog_count.unwrap_or_default()).map(InventoryHit::Catalog))
            .chain(self.catalog_count.map(|_| InventoryHit::Delete))
            .find(|&hit| {
                let [x, y, w, h] = self.slot(hit);
                (x..x + w).contains(&cx) && (y..y + h).contains(&cy)
            })
    }

    pub fn draw(
        &self,
        slots: &[Option<(ItemIcon, u16)>],
        selected: usize,
        cursor: Option<[f64; 2]>,
        carried: Option<(ItemIcon, u16)>,
        catalog: &[(&str, ItemIcon)],
    ) -> Vec<Vertex> {
        let mut panel = Panel {
            vertices: Vec::new(),
            size: self.size,
        };
        let creative = self.catalog_count.is_some();
        let bottom = self.slot(if creative {
            InventoryHit::Delete
        } else {
            InventoryHit::Player(0)
        })[1]
            + self.edge;
        panel.rect(
            [
                self.left - 12.0,
                self.top - 28.0,
                9.0 * self.edge + 18.0,
                bottom - self.top + 42.0,
            ],
            [0.07, 0.10, 0.14],
        );
        let hovered = cursor.and_then(|position| self.hit(position));
        let title = match hovered {
            Some(InventoryHit::Catalog(index)) => catalog[index].0,
            Some(InventoryHit::Delete) => "Delete carried stack",
            _ if creative => "Creative items   L: 1   M: stack",
            _ => "Backpack   E/Esc close",
        };
        label(
            &mut panel,
            title,
            self.left,
            self.top - 19.0,
            1.35,
            [0.23, 0.95, 0.88],
        );
        for (index, item) in slots.iter().enumerate() {
            let hit = InventoryHit::Player(index);
            let rect = self.slot(hit);
            draw_slot(&mut panel, rect, hovered == Some(hit), index == selected);
            if let Some((color, amount)) = item {
                draw_item(&mut panel, inset(rect, 3.0), *color, Some(*amount));
            }
        }
        for (index, &(_, color)) in catalog.iter().enumerate() {
            let hit = InventoryHit::Catalog(index);
            let rect = self.slot(hit);
            draw_slot(&mut panel, rect, hovered == Some(hit), false);
            draw_item(&mut panel, inset(rect, 3.0), color, None);
        }
        if creative {
            let hit = InventoryHit::Delete;
            let [x, y, w, h] = self.slot(hit);
            draw_slot(&mut panel, [x, y, w, h], hovered == Some(hit), false);
            panel.rect(inset([x, y, w, h], 4.0), [0.63, 0.16, 0.20]);
            label(
                &mut panel,
                "X",
                x + w / 2.0 - 5.0,
                y + h / 2.0 - 6.0,
                1.5,
                [1.0; 3],
            );
        }
        if let (Some([x, y]), Some((color, amount))) = (cursor, carried) {
            let edge = self.edge * 0.54;
            draw_item(
                &mut panel,
                [x as f32 + 5.0, y as f32 + 5.0, edge, edge],
                color,
                Some(amount),
            );
        }
        panel.vertices
    }
}

fn inset([x, y, w, h]: [f32; 4], margin: f32) -> [f32; 4] {
    [x + margin, y + margin, w - 2.0 * margin, h - 2.0 * margin]
}

fn draw_slot(panel: &mut Panel, rect: [f32; 4], hovered: bool, selected: bool) {
    let border = if hovered {
        [0.48, 0.85, 0.91]
    } else if selected {
        [0.92, 0.72, 0.34]
    } else {
        [0.24, 0.30, 0.38]
    };
    panel.rect(rect, border);
    panel.rect(inset(rect, 1.0), [0.12, 0.16, 0.21]);
}

fn draw_item(panel: &mut Panel, [x, y, w, h]: [f32; 4], icon: ItemIcon, count: Option<u16>) {
    panel.item([x, y, w, h], icon);
    if let Some(count) = count {
        label(
            panel,
            &count.to_string(),
            x + 1.0,
            y + (h - 8.0).max(0.0),
            1.0,
            [1.0; 3],
        );
    }
}
