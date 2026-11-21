use super::text::label;
use super::{Panel, Vertex};

const ROWS: [&str; 6] = [
    "RESUME",
    "SENSITIVITY",
    "INVERT Y",
    "MASTER VOLUME",
    "MUTE",
    "VIEW DISTANCE",
];
pub const PAUSE_MENU_ROWS: usize = ROWS.len();

pub struct PauseMenu {
    pub selected: usize,
    pub sensitivity_percent: u16,
    pub invert_y: bool,
    pub volume_percent: u8,
    pub muted: bool,
    pub paused: bool,
    pub view_distance: u8,
}

pub fn pause_menu([width, height]: [u32; 2], menu: PauseMenu) -> Vec<Vertex> {
    let size = [width as f32, height as f32];
    let mut panel = Panel {
        vertices: Vec::new(),
        size,
    };
    let w = (size[0] - 16.0).clamp(32.0, 356.0);
    let h = 194.0;
    let x = (size[0] - w) / 2.0;
    let y = (size[1] - h) / 2.0;
    let ink = [0.94, 0.96, 0.98];
    let accent = [1.0, 0.87, 0.46];
    panel.rect([x - 2.0, y - 2.0, w + 4.0, h + 4.0], [0.31, 0.37, 0.45]);
    panel.rect([x, y, w, h], [0.075, 0.095, 0.13]);
    label(
        &mut panel,
        if menu.paused { "PAUSED" } else { "PAUSING" },
        x + 14.0,
        y + 12.0,
        3.0,
        ink,
    );
    let sensitivity = format!("{}%", menu.sensitivity_percent);
    let volume = format!("{}%", menu.volume_percent);
    let distance = menu.view_distance.to_string();
    let values = [
        "",
        &sensitivity,
        if menu.invert_y { "ON" } else { "OFF" },
        &volume,
        if menu.muted { "ON" } else { "OFF" },
        &distance,
    ];
    for (index, (title, value)) in ROWS.into_iter().zip(values).enumerate() {
        let row = y + 40.0 + index as f32 * 22.0;
        let selected = index == menu.selected;
        let background = if selected {
            [0.26, 0.32, 0.40]
        } else {
            [0.13, 0.17, 0.22]
        };
        panel.rect([x + 12.0, row, w - 24.0, 20.0], background);
        let color = if selected { accent } else { ink };
        label(&mut panel, title, x + 20.0, row + 2.0, 2.0, color);
        label(&mut panel, value, x + w - 70.0, row + 2.0, 2.0, color);
    }
    label(
        &mut panel,
        "ARROWS  ENTER  ESC",
        x + 14.0,
        y + 179.0,
        1.5,
        [0.74, 0.80, 0.86],
    );
    panel.vertices
}
