mod signals;

use std::mem;

use belly::prelude::*;
use bevy::prelude::*;
use pecs::prelude::*;
use rand::seq::SliceRandom;
use signals::prelude::*;
const COLORS: &[&'static str] = &[
    // from https://colorswall.com/palette/105557
    // Red     Pink       Purple     Deep Purple
    "#f44336", "#e81e63", "#9c27b0", "#673ab7",
    // Indigo  Blue       Light Blue Cyan
    "#3f51b5", "#2196f3", "#03a9f4", "#00bcd4",
    // Teal    Green      Light      Green Lime
    "#009688", "#4caf50", "#8bc34a", "#cddc39",
    // Yellow  Amber      Orange     Deep Orange
    "#ffeb3b", "#ffc107", "#ff9800", "#ff5722",
];
fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugin(BellyPlugin)
        .add_plugin(PecsPlugin)
        .add_plugin(SignalsPlugin)
        .add_startup_system(setup)
        .add_system(update_cooldown)
        .add_system(update_filed)
        .run();
}

#[derive(Resource)]
pub struct GameState {
    life: usize,
    pts: usize,
    streak: usize,
    button_size: f32,
    timeout: f32,
    max_buttons: usize,
    buttons_on_screen: usize,
}

impl GameState {
    pub fn new() -> Self {
        GameState {
            life: 3,
            timeout: 3.0,
            streak: 0,
            pts: 0,
            button_size: 50.,
            max_buttons: 1,
            buttons_on_screen: 0,
        }
    }
    pub fn drop_life(&mut self) {
        if self.life == 0 {
            return;
        }
        self.life -= 1;
        self.streak = 0;
        self.button_size += 15.;
        self.timeout += 1.0;
        self.max_buttons = if self.max_buttons > 1 {
            self.max_buttons - 2
        } else {
            1
        }
    }
    pub fn add_pts(&mut self) {
        self.streak += 1;
        if self.streak > self.max_buttons * self.max_buttons * 3 {
            self.max_buttons += 1;
        }
        self.pts += self.streak;
        self.button_size = (self.button_size * 0.95).max(6.0);
        self.timeout -= 0.1 * rand::random::<f32>();
        self.timeout = self.timeout.max(1.0);
        // self.button_size = self.button_size.max(6.0);
    }
}

pub fn setup(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
    commands.add(StyleSheet::load("styles.ess"));
    let field = commands.spawn_empty().id();
    commands.insert_resource(GameState::new());

    commands.add(eml! {
        <body>
            <div {field} id="field"/>
            <div id="ui">
                <span c:life>
                    <label c:value bind:value=from!(GameState:life|fmt.p("{p}"))/>
                </span>
                <span c:pts>
                    <label c:value bind:value=from!(GameState:pts|fmt.p("{p}"))/>
                </span>
            </div>
            <div id="popups">
                <span id="start-game" c:hidden c:border c:center>
                    <span c:content c:column>
                        <span>"Hit the buttons as fast as you can!"</span>  
                        <button id="btn-start">"Start!"</button>
                    </span>
                </span>
                <span id="restart-game" c:hidden c:border>
                    <span c:content c:column>
                        <label c:value bind:value=from!(GameState:pts|fmt.p("You loose with {p} pts!"))/>
                        <span c:row>
                            <button id="btn-restart">"Restart"</button>
                            <button id="btn-exit">"Exit"</button>
                        </span>
                    </span>
                </span>
            </div>
        </body>
    });
    commands.add(gameloop())
}

pub fn gameloop() -> Promise<(), ()> {
    Promise::from(())
        .then(asyn!({
            popup_start()
        }))
        .then_repeat(asyn!(mut commands: Commands => {
            commands.insert_resource(GameState::new());
            (0..5)
                .map(|idx| buttonloop(idx))
                .promise()
                .any()
                .then(asyn!({
                    popup_restart()
                }))
                .then(asyn!(_, restart =>{
                    if restart {
                        Promise::resolve(Repeat::Continue)
                    } else {
                        Promise::resolve(Repeat::Break(()))
                    }
                }))

        }))
        .then(asyn!({
            asyn::app::exit()
        }))
}

pub struct ButtonLoop {
    idx: usize,
    btn: Option<Entity>,
}
impl ButtonLoop {
    pub fn new(id: usize) -> Self {
        ButtonLoop { idx: id, btn: None }
    }
    pub fn btn(&mut self) -> Entity {
        mem::take(&mut self.btn).unwrap()
    }
}

pub fn buttonloop(idx: usize) -> Promise<ButtonLoop, ()> {
    Promise::repeat(ButtonLoop::new(idx), asyn!(this, game: Res<GameState> => {
        if this.idx >= game.max_buttons {
            return this.asyn().timeout(1.0).with_result(Repeat::Continue);
        }
        let delay = if game.buttons_on_screen > 0 {
            rand::random::<f32>() * game.timeout
        } else {
            0.
        };
        this
            .asyn().timeout(delay)
            .then(asyn!(this, mut elements: Elements, mut game: ResMut<GameState> => {
                let btn = elements.add_button(game.button_size, game.timeout);
                game.buttons_on_screen += 1;
                let was_life = game.life;
                this.btn = Some(btn);
                this.any((
                    signals::pressed(btn),
                    signals::resource_changed(from!(GameState:life), |life| life == &0),
                    signals::resource_changed(from!(GameState:life), move |life| { *life != was_life }),
                    asyn::timeout(game.timeout),
                ))
            }))
            .then(asyn!(
                this, (hit, gameover, lifelost, _),
                mut commands: Commands,
                mut elements: Elements,
                mut game: ResMut<GameState>
            => {
                commands.entity(this.btn()).despawn_recursive();
                game.buttons_on_screen -= 1;
                if gameover.is_some() {
                    return this.resolve(Repeat::Break(()));
                }
                if lifelost.is_some() {
                    return this.resolve(Repeat::Continue);
                }
                if hit.is_none() {
                    elements.show_failed();
                    game.drop_life();
                } else {
                    game.add_pts();
                }
                if game.life == 0 {
                    this.resolve(Repeat::Break(()))
                } else {
                    this.resolve(Repeat::Continue)
                }
            }))
    }))
}

fn popup_start() -> Promise<(), ()> {
    Promise::from(())
        .then(asyn!(mut elements: Elements => {
            elements.select("#start-game").remove_class("hidden");
            let start_btn = elements.select("#btn-start").entities()[0];
            signals::pressed(start_btn)
        }))
        .then(asyn!(mut elements: Elements => {
            elements.select("#start-game").add_class("hidden");
        }))
}

fn popup_restart() -> Promise<(), bool> {
    Promise::from(())
        .then(asyn!(mut elements: Elements => {
            elements.select("#restart-game").remove_class("hidden");
            let restart_btn = elements.select("#btn-restart").entities()[0];
            let exit_btn = elements.select("#btn-exit").entities()[0];
            Promise::any((
                signals::pressed(restart_btn),
                signals::pressed(exit_btn)
            ))
        }))
        .then(asyn!(_, (restart, _exit), mut elements: Elements => {
            elements.select("#restart-game").add_class("hidden");
            Promise::resolve(restart.is_some())
        }))
}

pub trait GameUi {
    fn add_button(&mut self, size: f32, timeout: f32) -> Entity;
    fn show_failed(&mut self);
}

impl<'w, 's> GameUi for Elements<'w, 's> {
    fn show_failed(&mut self) {
        let field = self.select("#field").entities()[0];
        self.commands().entity(field).insert(Failed::default());
    }
    fn add_button(&mut self, size: f32, timeout: f32) -> Entity {
        let width = size;
        let height = size + 5. + size * rand::random::<f32>();
        let x = Val::Percent((100.0 - width) * rand::random::<f32>());
        let y = Val::Percent((100.0 - height) * rand::random::<f32>());
        let root = self.select("#field").entities()[0];
        let color = COLORS.choose(&mut rand::thread_rng()).unwrap();
        let cooldown = Cooldown::new(timeout);
        self.add_child(
            root,
            eml! {
                <button
                    mode="instant"
                    s:position-type="absolute"
                    s:width=Val::Percent(width)
                    s:height=Val::Percent(height)
                    s:left=x
                    s:top=y
                >
                    <span s:width="100%" s:height="100%" s:background-color=color>
                        <span with=cooldown
                            s:width="100%"
                            s:height=managed()
                            s:background-color="#0000002f"/>
                    </span>
                </button>
            },
        )
    }
}

#[derive(Component)]
pub struct Cooldown {
    duration: f32,
    left: f32,
}
impl Cooldown {
    pub fn new(duration: f32) -> Cooldown {
        Cooldown {
            duration,
            left: duration,
        }
    }
}

fn update_cooldown(time: Res<Time>, mut query: Query<(&mut Cooldown, &mut Style)>) {
    let delta = time.delta_seconds();
    for (mut timeout, mut style) in query.iter_mut() {
        timeout.left -= delta;
        timeout.left = timeout.left.max(0.0);
        let size = 100. * timeout.left / timeout.duration;
        let size = Val::Percent(size);
        style.size.height = size;
    }
}

const FAILED_DURATION: f32 = 0.4;
const HALF_DURATION: f32 = FAILED_DURATION * 0.5;
#[derive(Component, Default)]
pub struct Failed {
    duration: f32,
}
fn update_filed(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut Failed, &mut BackgroundColor)>,
) {
    let delta = time.delta_seconds();
    for (entity, mut failed, mut color) in query.iter_mut() {
        failed.duration += delta;
        if failed.duration > FAILED_DURATION {
            color.0 = Color::NONE;
            commands.entity(entity).remove::<Failed>();
        } else {
            color.0 = Color::RED;
            if failed.duration < HALF_DURATION * 0.5 {
                color.0.set_a(failed.duration / HALF_DURATION);
            } else {
                color
                    .0
                    .set_a(1.0 - (failed.duration - HALF_DURATION) / HALF_DURATION);
            }
        }
    }
}
