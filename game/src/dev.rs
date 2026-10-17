//! The shell's own dev menu, not the game's: its key (F2) opens it and pauses, the key again or
//! Escape closes it. It picks the melee skin held in the fists' place, by click or by number
//! key; the game's files are read again for it (`data::reload`), and the viewmodel is drawn
//! again from them.

use bevy::prelude::*;
use lurch::assets::{Heirloom, HEIRLOOMS};

use crate::binds::{Action, Input};
use crate::cmd::{pause_input, CmdBuilder};
use crate::data::{Data, Loading};
use crate::{common, Config, DevMenu, Frame, KeyTrap, MenuOpen, Paused};

const DIGITS: [KeyCode; 9] =
    [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9];

#[derive(Component)]
struct Panel;

/// A row: its number key's index, and the fists (None) or an heirloom.
#[derive(Component)]
struct Choice(usize, Option<&'static Heirloom>);

fn choices() -> impl Iterator<Item = Option<&'static Heirloom>> {
    std::iter::once(None).chain(HEIRLOOMS.iter().map(Some))
}

pub struct DevPlugin;

impl Plugin for DevPlugin {
    fn build(&self, app: &mut App) {
        common(app);
        app.add_systems(Startup, spawn)
            .add_systems(PreUpdate, toggle.in_set(Frame::Pause).before(pause_input))
            .add_systems(Update, (choose, show).chain().in_set(Frame::View));
    }
}

fn spawn(mut commands: Commands) {
    let font = TextFont::from_font_size(16.0);
    let panel = Node { position_type: PositionType::Absolute, top: px(120), left: px(8), flex_direction: FlexDirection::Column, padding: UiRect::all(px(8)), row_gap: px(2), ..default() };
    commands.spawn((Panel, panel, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.75)), Visibility::Hidden)).with_children(|p| {
        p.spawn((Text::new("Dev menu: heirloom"), font.clone()));
        for (n, h) in choices().enumerate() {
            p.spawn((Button, Choice(n, h), Text::default(), font.clone(), Node { padding: UiRect::axes(px(6), px(2)), ..default() }));
        }
    });
}

/// Escape closes the menu before the in-match menu sees it.
#[allow(clippy::too_many_arguments)] // Bevy's system parameters
fn toggle(mut keys: ResMut<ButtonInput<KeyCode>>, cfg: Res<Config>, menu: Res<MenuOpen>, trap: Res<KeyTrap>, mut dev: ResMut<DevMenu>, mut paused: ResMut<Paused>, mut b: ResMut<CmdBuilder>) {
    let key = keys.get_just_pressed().any(|&k| cfg.binds.get(Input::Key(k)) == Some(Action::DevMenu));
    let escape = dev.0 && keys.clear_just_pressed(KeyCode::Escape);
    if trap.0 || menu.0.is_some() || !(key || escape) {
        return;
    }
    dev.0 = !dev.0;
    paused.set_if_neq(Paused(dev.0));
    if dev.0 {
        b.release_all();
    }
}

fn choose(keys: Res<ButtonInput<KeyCode>>, dev: Res<DevMenu>, rows: Query<(&Interaction, &Choice), Changed<Interaction>>, mut cfg: ResMut<Config>) {
    if !dev.0 {
        return;
    }
    let clicked = rows.iter().find(|(i, _)| **i == Interaction::Pressed).map(|(_, c)| c.1);
    let typed = DIGITS.iter().position(|&k| keys.just_pressed(k)).and_then(|n| choices().nth(n));
    if let Some(h) = clicked.or(typed).filter(|&h| h != cfg.heirloom) {
        cfg.heirloom = h;
    }
}

fn show(dev: Res<DevMenu>, cfg: Res<Config>, data: Option<Res<Data>>, loading: Option<Res<Loading>>, mut panel: Query<&mut Visibility, With<Panel>>, mut rows: Query<(&Choice, &Interaction, &mut Text, &mut BackgroundColor)>) {
    for mut v in &mut panel {
        v.set_if_neq(if dev.0 { Visibility::Inherited } else { Visibility::Hidden });
    }
    if !dev.0 {
        return;
    }
    let loaded = data.map(|d| d.0.weapon_class);
    for (Choice(n, h), interaction, mut text, mut bg) in &mut rows {
        let chosen = *h == cfg.heirloom;
        let state = match (chosen, loaded == Some(h.map_or(lurch::assets::FISTS, |h| h.weapon)), loading.is_some()) {
            (false, _, _) | (true, true, _) => "",
            (true, false, true) => "  (loading)",
            (true, false, false) => "  (failed, see the log)",
        };
        let label = format!("{} {}  {}{state}", if chosen { '>' } else { ' ' }, n + 1, h.map_or("fists", |h| h.name));
        if text.0 != label {
            text.0 = label;
        }
        bg.set_if_neq(BackgroundColor(if *interaction == Interaction::None { Color::NONE } else { Color::srgba(1.0, 1.0, 1.0, 0.15) }));
    }
}
