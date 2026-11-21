use std::collections::VecDeque;
use std::ops::Range;

use rustcraft_authority::{Coordinate, GAME_TICKS_PER_SECOND, PlayerCommand};
use rustcraft_host::TickCommand;
use rustcraft_vulkan::ConsoleKey;

const COMMANDS: &[&str] = &[
    "tp", "teleport", "give", "gamemode", "tick", "freecam", "redstone",
];
const TICK_ACTIONS: &[&str] = &["query", "rate", "freeze", "unfreeze", "step", "sprint"];
const LOCAL_TARGETS: &[&str] = &["@s", "@p", "@a", "@r"];
const DEBUG_ACTIONS: &[&str] = &["on", "off", "step", "previous", "next"];
const GAME_MODES: &[&str] = &["creative", "survival"];
const JAVA_TEST_ITEMS: &[&str] = &[
    "minecraft:bedrock",
    "minecraft:dirt",
    "minecraft:grass_block",
    "minecraft:smooth_stone",
    "minecraft:lever",
    "minecraft:redstone",
    "minecraft:redstone_lamp",
    "minecraft:repeater",
    "minecraft:piston",
];

const LINE_LIMIT: usize = 192;
const HISTORY_LIMIT: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DebugAction {
    On,
    Off,
    Step,
    Previous,
    Next,
}

#[derive(Debug)]
pub(super) enum Command {
    Player(PlayerCommand),
    Redstone(DebugAction),
    FreeCamera(Option<bool>),
    Tick(TickCommand),
}

struct Suggestions {
    span: Range<usize>,
    candidates: Vec<&'static str>,
    selected: Option<usize>,
}

#[derive(Default)]
pub(super) struct Console {
    pub open: bool,
    line: String,
    cursor: usize,
    history: VecDeque<String>,
    browse: Option<usize>,
    draft: String,
    suggestions: Option<Suggestions>,
    pub message: String,
}

impl Console {
    pub fn show(&mut self) {
        self.open = true;
        self.line.clear();
        self.cursor = 0;
        self.message.clear();
        self.browse = None;
        self.suggestions = None;
    }

    pub fn edit(&mut self, key: ConsoleKey) -> Option<Command> {
        match key {
            ConsoleKey::Open => self.show(),
            ConsoleKey::Close if self.suggestions.is_some() => self.suggestions = None,
            ConsoleKey::Close => self.open = false,
            ConsoleKey::Complete | ConsoleKey::CompleteBackward => {
                self.complete(matches!(key, ConsoleKey::CompleteBackward));
            }
            ConsoleKey::Previous | ConsoleKey::Next if self.suggestions.is_some() => {
                self.complete(matches!(key, ConsoleKey::Previous));
            }
            ConsoleKey::Previous => self.browse_history(true),
            ConsoleKey::Next => self.browse_history(false),
            ConsoleKey::Submit if self.suggestions.is_some() => {
                let menu = self.suggestions.take().expect("open completion menu");
                let candidate = menu.candidates[menu.selected.unwrap_or(0)];
                self.replace_token(menu.span, candidate, true);
            }
            ConsoleKey::Submit => match parse(&self.line) {
                Ok(command) => {
                    if self.history.back() != Some(&self.line) {
                        if self.history.len() == HISTORY_LIMIT {
                            self.history.pop_front();
                        }
                        self.history.push_back(self.line.clone());
                    }
                    self.browse = None;
                    return Some(command);
                }
                Err(error) => self.message = error.into(),
            },
            key => self.edit_text(key),
        }
        None
    }

    fn edit_text(&mut self, key: ConsoleKey) {
        self.suggestions = None;
        match key {
            ConsoleKey::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                self.line.remove(self.cursor);
            }
            ConsoleKey::Delete if self.cursor < self.line.len() => {
                self.line.remove(self.cursor);
            }
            ConsoleKey::Left => self.cursor = self.cursor.saturating_sub(1),
            ConsoleKey::Right if self.cursor == self.line.len() => {
                let suffix = self.inline().to_owned();
                self.line.push_str(&suffix);
                self.cursor = self.line.len();
            }
            ConsoleKey::Right => self.cursor += 1,
            ConsoleKey::Home => self.cursor = 0,
            ConsoleKey::End => self.cursor = self.line.len(),
            ConsoleKey::Text(text)
                if self.line.len() + text.len() <= LINE_LIMIT && text.is_ascii() =>
            {
                self.line.insert_str(self.cursor, &text);
                self.cursor += text.len();
            }
            _ => {}
        }
    }

    fn browse_history(&mut self, previous: bool) {
        if previous {
            let Some(last) = self.history.len().checked_sub(1) else {
                return;
            };
            if self.browse.is_none() {
                self.draft.clone_from(&self.line);
            }
            self.browse = Some(self.browse.map_or(last, |index| index.saturating_sub(1)));
        } else {
            let Some(index) = self.browse else {
                return;
            };
            self.browse = (index + 1 < self.history.len()).then_some(index + 1);
        }
        self.line.clone_from(
            self.browse
                .map_or(&self.draft, |index| &self.history[index]),
        );
        self.cursor = self.line.len();
    }

    fn complete(&mut self, backward: bool) {
        if let Some(menu) = &mut self.suggestions {
            let count = menu.candidates.len();
            menu.selected = Some(match (menu.selected, backward) {
                (None, false) => 0,
                (None, true) => count - 1,
                (Some(index), false) => (index + 1) % count,
                (Some(index), true) => (index + count - 1) % count,
            });
            return;
        }
        let (mut span, matches) = self.completion();
        let Some(&first) = matches.first() else {
            return;
        };
        if matches.len() == 1 {
            self.replace_token(span, first, true);
            return;
        }
        let common = matches.iter().skip(1).fold(first, |prefix, candidate| {
            let len = prefix
                .bytes()
                .zip(candidate.bytes())
                .take_while(|(a, b)| a == b)
                .count();
            &prefix[..len]
        });
        let typed = &self.line[span.start..self.cursor];
        if self.cursor == span.end && common.len() > typed.len() && common.starts_with(typed) {
            self.replace_token(span.clone(), common, false);
            span.end = self.cursor;
        }
        let selected = backward.then_some(matches.len() - 1);
        self.suggestions = Some(Suggestions {
            span,
            candidates: matches,
            selected,
        });
    }

    fn replace_token(&mut self, span: Range<usize>, candidate: &str, append_space: bool) {
        let start = span.start;
        let append = append_space && span.end == self.line.len();
        if self.line.len() - span.len() + candidate.len() + usize::from(append) > LINE_LIMIT {
            return;
        }
        self.line.replace_range(span, candidate);
        self.cursor = start + candidate.len();
        if append {
            self.line.push(' ');
            self.cursor += 1;
        }
    }

    pub fn rows(&self) -> Vec<String> {
        let mut rows = vec!["Tab complete   Shift+Tab back   Up/Down history   Esc close".into()];
        rows.push(format!("/{}", self.line));
        if let Some(menu) = &self.suggestions {
            let start = menu.selected.map_or(0, |index| index / 5 * 5);
            rows.extend(menu.candidates.iter().enumerate().skip(start).take(5).map(
                |(index, candidate)| {
                    format!(
                        "{} {candidate}",
                        if menu.selected == Some(index) {
                            ">"
                        } else {
                            " "
                        }
                    )
                },
            ));
        }
        if !self.message.is_empty() {
            rows.push(self.message.clone());
        }
        rows
    }

    pub fn cursor(&self) -> usize {
        self.cursor + 1
    }

    pub fn inline(&self) -> &'static str {
        if self.suggestions.is_some() || self.cursor != self.line.len() {
            return "";
        }
        let (span, matches) = self.completion();
        let typed = &self.line[span];
        if typed.is_empty() {
            return "";
        }
        matches
            .first()
            .and_then(|candidate| candidate.strip_prefix(typed))
            .unwrap_or_default()
    }

    fn completion(&self) -> (Range<usize>, Vec<&'static str>) {
        let start = self.line[..self.cursor]
            .rfind(|c: char| c.is_ascii_whitespace())
            .map_or(0, |index| index + 1);
        let end = self.line[self.cursor..]
            .find(|c: char| c.is_ascii_whitespace())
            .map_or(self.line.len(), |index| self.cursor + index);
        let span = start..end;
        let typed = &self.line[start..self.cursor];
        let mut preceding = self.line[..start].split_ascii_whitespace();
        let choices = match (
            preceding.next().map(command_name),
            preceding.next(),
            preceding.next(),
        ) {
            (None, _, _) => COMMANDS,
            (Some("redstone"), None, _) => DEBUG_ACTIONS,
            (Some("freecam"), None, _) => &["on", "off", "toggle"],
            (Some("gamemode"), None, _) => GAME_MODES,
            (Some("gamemode"), Some(mode), None) if GAME_MODES.contains(&mode) => LOCAL_TARGETS,
            (Some("give"), None, _) => LOCAL_TARGETS,
            (Some("tp"), None, _) => &["@s", "@p", "@a", "@r", "~", "^"],
            (Some("give"), Some(target), None) if LOCAL_TARGETS.contains(&target) => {
                JAVA_TEST_ITEMS
            }
            (Some("tick"), None, _) => TICK_ACTIONS,
            (Some("tick"), Some("step" | "sprint"), None) => &["stop"],
            _ => &[],
        };
        let mut ranked: Vec<_> = choices
            .iter()
            .copied()
            .filter_map(|name| match_score(typed, name).map(|score| (score, name)))
            .collect();
        ranked.sort_unstable();
        (span, ranked.into_iter().map(|(_, name)| name).collect())
    }
}

fn match_score(typed: &str, name: &str) -> Option<(u8, usize)> {
    if name.starts_with(typed) {
        return Some((0, name.len()));
    }
    let local = name.split_once(':').map_or(name, |(_, path)| path);
    if local.starts_with(typed) {
        return Some((1, local.len()));
    }
    if let Some(index) = local.find(typed) {
        return Some((2, index));
    }
    let mut cursor = 0;
    for byte in typed.bytes() {
        cursor += local[cursor..]
            .bytes()
            .position(|candidate| candidate == byte)?
            + 1;
    }
    Some((3, cursor))
}

fn parse(line: &str) -> Result<Command, &'static str> {
    let mut fields: Vec<_> = line
        .trim_start_matches('/')
        .split_ascii_whitespace()
        .collect();
    if let Some(name) = fields.first_mut() {
        *name = command_name(name);
    }
    match fields.as_slice() {
        ["tp", args @ ..] => teleport(args).map(Command::Player),
        ["give", target, name] if LOCAL_TARGETS.contains(target) => {
            Ok(Command::Player(PlayerCommand::Give {
                item: (*name).into(),
                count: 1,
            }))
        }
        ["give", target, name, count] if LOCAL_TARGETS.contains(target) => {
            Ok(Command::Player(PlayerCommand::Give {
                item: (*name).into(),
                count: count
                    .parse::<u16>()
                    .ok()
                    .filter(|&amount| amount > 0)
                    .ok_or("COUNT MUST BE POSITIVE")?,
            }))
        }
        ["freecam"] | ["freecam", "toggle"] => Ok(Command::FreeCamera(None)),
        ["freecam", "on"] => Ok(Command::FreeCamera(Some(true))),
        ["freecam", "off"] => Ok(Command::FreeCamera(Some(false))),
        ["gamemode", mode] => Ok(Command::Player(PlayerCommand::GameMode(mode.parse()?))),
        ["gamemode", mode, target] if LOCAL_TARGETS.contains(target) => {
            Ok(Command::Player(PlayerCommand::GameMode(mode.parse()?)))
        }
        ["tick", args @ ..] => tick_command(args).map(Command::Tick),
        ["redstone", action] => Ok(Command::Redstone(match *action {
            "on" => DebugAction::On,
            "off" => DebugAction::Off,
            "step" => DebugAction::Step,
            "previous" => DebugAction::Previous,
            "next" => DebugAction::Next,
            _ => return Err("REDSTONE ON/OFF/STEP/PREVIOUS/NEXT"),
        })),
        ["give", ..] => Err("GIVE <TARGET> <ITEM> [COUNT]"),
        ["gamemode", ..] => Err("GAMEMODE <CREATIVE|SURVIVAL> [TARGET]"),
        _ => Err("AVAILABLE: TP  GIVE  GAMEMODE  TICK  FREECAM  REDSTONE"),
    }
}

fn command_name(name: &str) -> &str {
    match name {
        "teleport" | "minecraft:teleport" | "minecraft:tp" => "tp",
        "minecraft:give" => "give",
        "minecraft:gamemode" => "gamemode",
        "minecraft:tick" => "tick",
        "rustcraft:freecam" => "freecam",
        "rustcraft:redstone" => "redstone",
        _ => name,
    }
}

fn tick_command(args: &[&str]) -> Result<TickCommand, &'static str> {
    Ok(match args {
        ["query"] => TickCommand::Query,
        ["rate", rate] => TickCommand::Rate(rate.parse().map_err(|_| "INVALID TPS")?),
        ["freeze"] => TickCommand::Freeze,
        ["unfreeze"] => TickCommand::Unfreeze,
        ["step"] => TickCommand::Step(1),
        ["step", "stop"] => TickCommand::StopStep,
        ["step", time] => TickCommand::Step(tick_time(time)?),
        ["sprint", "stop"] => TickCommand::StopSprint,
        ["sprint", time] => TickCommand::Sprint(tick_time(time)?),
        _ => return Err("TICK QUERY/RATE/FREEZE/UNFREEZE/STEP/SPRINT"),
    })
}

fn tick_time(input: &str) -> Result<u32, &'static str> {
    let (number, scale) = match input.as_bytes().last() {
        Some(b't') => (&input[..input.len() - 1], 1.0),
        Some(b's') => (&input[..input.len() - 1], f64::from(GAME_TICKS_PER_SECOND)),
        Some(b'd') => (&input[..input.len() - 1], 24_000.0),
        _ => (input, 1.0),
    };
    let ticks = (number.parse::<f64>().map_err(|_| "INVALID TICK TIME")? * scale).round();
    if !(1.0..=f64::from(i32::MAX)).contains(&ticks) {
        return Err("TICK TIME MUST BE POSITIVE");
    }
    Ok(ticks as u32)
}

fn teleport(args: &[&str]) -> Result<PlayerCommand, &'static str> {
    let args = match args {
        [target, rest @ ..] if LOCAL_TARGETS.contains(target) => rest,
        _ => args,
    };
    let (x, y, z, rotation) = match args {
        [x, y, z] => (*x, *y, *z, None),
        [x, y, z, yaw, pitch] => (
            *x,
            *y,
            *z,
            Some([coordinate(yaw, false)?, coordinate(pitch, false)?]),
        ),
        _ => return Err("TP [TARGET] X Y Z [YAW PITCH]"),
    };
    let position = [
        coordinate(x, true)?,
        coordinate(y, false)?,
        coordinate(z, true)?,
    ];
    let local = position
        .iter()
        .filter(|coordinate| matches!(coordinate, Coordinate::Local(_)))
        .count();
    if local != 0 && local != 3 {
        return Err("USE THREE LOCAL COORDINATES TOGETHER");
    }
    if let Some(rotation) = rotation {
        if rotation
            .iter()
            .any(|coordinate| matches!(coordinate, Coordinate::Local(_)))
        {
            return Err("ROTATION ALLOWS ABSOLUTE OR RELATIVE ANGLES");
        }
        Ok(PlayerCommand::TeleportFacing { position, rotation })
    } else {
        Ok(PlayerCommand::Teleport(position))
    }
}

fn coordinate(text: &str, center: bool) -> Result<Coordinate, &'static str> {
    let relative = text.strip_prefix('~');
    let local = text.strip_prefix('^');
    let input = relative.or(local).unwrap_or(text);
    let value = if input.is_empty() && (relative.is_some() || local.is_some()) {
        0.0
    } else {
        input
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .ok_or("INVALID COORDINATE")?
    };
    Ok(if local.is_some() {
        Coordinate::Local(value)
    } else if relative.is_some() {
        Coordinate::Relative(value)
    } else {
        Coordinate::Absolute(if center && !input.contains('.') {
            value + 0.5
        } else {
            value
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completes_commands_and_accepts_relative_self_teleport() {
        let mut console = Console::default();
        console.edit(ConsoleKey::Open);
        console.edit(ConsoleKey::Text("gv".into()));
        assert_eq!(console.completion(), (0..2, vec!["give"]));
        console.edit(ConsoleKey::Complete);
        assert_eq!(console.line, "give ");
        let teleport = parse("tp @s ~16 -60 ~").unwrap();
        assert!(matches!(
            teleport,
            Command::Player(PlayerCommand::Teleport([
                Coordinate::Relative(16.0),
                Coordinate::Absolute(-60.0),
                Coordinate::Relative(0.0),
            ]))
        ));
        assert!(parse("tp @s NaN -60 0").is_err());
        assert!(parse("give @a minecraft:dirt 1").is_ok());
        assert!(parse("give @e minecraft:dirt 1").is_err());
        assert!(matches!(
            parse("redstone step").unwrap(),
            Command::Redstone(DebugAction::Step)
        ));
    }

    #[test]
    fn recognizes_vanilla_tick_units_and_teleport_aliases() {
        assert!(matches!(
            parse("tick step"),
            Ok(Command::Tick(TickCommand::Step(1)))
        ));
        assert!(matches!(
            parse("/minecraft:tick step 0.5s"),
            Ok(Command::Tick(TickCommand::Step(10)))
        ));
        assert!(matches!(
            parse("/tick sprint 1d"),
            Ok(Command::Tick(TickCommand::Sprint(24_000)))
        ));
        assert!(matches!(
            parse("/tick step stop"),
            Ok(Command::Tick(TickCommand::StopStep))
        ));
        assert!(parse("/tick step 0t").is_err());
        assert!(parse("/tick step 1min").is_err());
        assert!(matches!(
            parse("/minecraft:teleport @p 0 -60 10.0 90 ~"),
            Ok(Command::Player(PlayerCommand::TeleportFacing {
                position: [
                    Coordinate::Absolute(0.5),
                    Coordinate::Absolute(-60.0),
                    Coordinate::Absolute(10.0)
                ],
                rotation: [Coordinate::Absolute(90.0), Coordinate::Relative(0.0)],
            }))
        ));
        assert!(parse("/teleport @s ^ ^ ^3").is_ok());
        assert!(parse("/teleport @s ^ ~ ^3").is_err());
    }

    #[test]
    fn completes_after_repeated_whitespace_and_edits_only_the_current_word() {
        let mut console = Console::default();
        console.edit(ConsoleKey::Text("give  @s \t minecraft:di 5".into()));
        for _ in 0..2 {
            console.edit(ConsoleKey::Left);
        }
        console.edit(ConsoleKey::Complete);
        assert_eq!(console.line, "give  @s \t minecraft:dirt 5");
        assert!(matches!(
            console.edit(ConsoleKey::Submit),
            Some(Command::Player(PlayerCommand::Give { count: 5, .. }))
        ));
    }

    #[test]
    fn tab_cycles_ambiguous_candidates_and_enter_accepts_before_running() {
        let mut console = Console::default();
        console.edit(ConsoleKey::Text("give  @s minecraft:red".into()));
        console.edit(ConsoleKey::Complete);
        assert_eq!(console.line, "give  @s minecraft:redstone");
        console.edit(ConsoleKey::Complete);
        console.edit(ConsoleKey::CompleteBackward);
        assert!(console.edit(ConsoleKey::Submit).is_none());
        assert_eq!(console.line, "give  @s minecraft:redstone_lamp ");
        assert!(matches!(
            console.edit(ConsoleKey::Submit),
            Some(Command::Player(PlayerCommand::Give { count: 1, .. }))
        ));
    }
}
