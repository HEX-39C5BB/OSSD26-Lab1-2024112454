use super::text::label;
use super::{Panel, Vertex};

pub const HUD_ACCENT: [f32; 3] = [0.23, 0.95, 0.88];
pub const HUD_TEXT: [f32; 3] = [0.94, 0.96, 0.98];

pub type HudLine<'a> = Vec<(&'a str, [f32; 3])>;

#[derive(Clone, Copy)]
pub enum HudCorner {
    TopLeft,
    TopRight,
    BottomLeft,
}

pub fn colored_hud(
    size: [u32; 2],
    rows: &[HudLine<'_>],
    corner: HudCorner,
    columns: bool,
) -> Vec<Vertex> {
    hud(size, rows, corner, columns, None)
}

pub fn command_hud(size: [u32; 2], rows: &[String], inline: &str, cursor: usize) -> Vec<Vertex> {
    let [help, input, rest @ ..] = rows else {
        return Vec::new();
    };
    let mut lines: Vec<HudLine<'_>> = vec![vec![(help.as_str(), HUD_ACCENT)]];
    lines.extend(rest.iter().map(|row| {
        vec![(
            row.as_str(),
            if row.starts_with('>') {
                HUD_ACCENT
            } else {
                HUD_TEXT
            },
        )]
    }));
    let prompt_row = lines.len();
    let (command, arguments) = input.split_at(input.find(' ').unwrap_or(input.len()));
    lines.push(vec![
        (command, HUD_ACCENT),
        (arguments, [1.0, 0.83, 0.53]),
        (inline, [0.51, 0.60, 0.67]),
    ]);
    hud(
        size,
        &lines,
        HudCorner::BottomLeft,
        false,
        Some((prompt_row, cursor)),
    )
}

fn hud(
    size: [u32; 2],
    rows: &[HudLine<'_>],
    corner: HudCorner,
    columns: bool,
    caret: Option<(usize, usize)>,
) -> Vec<Vertex> {
    const MARGIN: f32 = 10.0;
    const PAD: f32 = 8.0;
    let size = size.map(|px| px as f32);
    let longest = rows
        .iter()
        .map(|row| row.iter().map(|(text, _)| text.len()).sum::<usize>())
        .max()
        .unwrap_or_default();
    let available_width = if columns { size[0] / 2.0 } else { size[0] };
    if longest == 0 || available_width <= 2.0 * (MARGIN + PAD) || size[1] <= 2.0 * (MARGIN + PAD) {
        return Vec::new();
    }
    let text_width = 6.0 * longest as f32;
    let text_height = 10.0 * rows.len() as f32;
    let scale = 2.0_f32
        .min((available_width - 2.0 * (MARGIN + PAD)) / text_width)
        .min((size[1] - 2.0 * (MARGIN + PAD)) / text_height);
    let width = text_width * scale + 2.0 * PAD;
    let height = text_height * scale + 2.0 * PAD;
    let (x, y) = match corner {
        HudCorner::TopLeft => (MARGIN, MARGIN),
        HudCorner::TopRight => (size[0] - width - MARGIN, MARGIN),
        HudCorner::BottomLeft => (MARGIN, size[1] - height - MARGIN),
    };
    let mut panel = Panel {
        vertices: Vec::new(),
        size,
    };
    panel.rect([x, y, width, height], [0.075, 0.095, 0.13]);
    panel.rect([x, y, 2.0, height], HUD_ACCENT);
    for (index, row) in rows.iter().enumerate() {
        let line_y = y + PAD + 10.0 * index as f32 * scale;
        let mut line_x = x + PAD;
        for &(text, color) in row {
            label(&mut panel, text, line_x, line_y, scale, color);
            line_x += 6.0 * text.len() as f32 * scale;
        }
        if let Some((row, column)) = caret
            && row == index
        {
            panel.rect(
                [
                    x + PAD + 6.0 * column as f32 * scale,
                    line_y,
                    1.0,
                    8.0 * scale,
                ],
                HUD_TEXT,
            );
        }
    }
    panel.vertices
}
