use std::marker::PhantomData;

use belly::{prelude::*, widgets::input::button::{BtnEvent, ButtonWidget}, core::{relations::bind::{FromResource, BindableSource, BindableTarget}, impl_properties, Widgets}, build::GetProperties};
use bevy::{
    prelude::*,
    utils::{HashMap, HashSet},
};
use pecs::{prelude::*, core::AsynOps};

pub mod prelude {
    pub use super::SignalsPlugin;
    pub use super::SignalOpsExt;
    pub mod signals {
        pub use super::super::pressed;
        pub use super::super::resource_changed;
    }
}

pub struct SignalsPlugin;
impl Plugin for SignalsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SignalsRegistry::<BtnEvent>::new());
        app.add_system(resolve_signals::<BtnEvent>);
        app.add_system(resolve_value_changes);
    }
}

pub fn pressed(entity: Entity) -> Promise<(), ()> {
    // let x = <ButtonWidget as Widget>::Signals::instance();
    // Widgets::button().on().press().connect(world, entity, func)
    // let sig = ButtonWidget::instance().on().press();
    // sig.connect(world, source, |ctx| {});
    // sig.promise(source);

    // let x = connect!(|ctx| { });
    // let promise = asyn::signal(connect![button:press]).element("#my-button")
    // let promise = asyn::emitted(signal![button press]).on_entity(id)
    // let promise = asyn::emitted(signal![button press]).on("#my-button")
    // let promise = elements.select("#my-button").emitted(signal![button press]);
    // let promise = asyn::resource_changed(from!)
    Promise::register(
        move |world, id| {
            world
                .resource_mut::<SignalsRegistry<BtnEvent>>()
                .register_promise(id, entity, |e| e.pressed())
        },
        |world, id| {
            world
                .resource_mut::<SignalsRegistry<BtnEvent>>()
                .discard_promise(id)
        },
    )
}

#[derive(Component)]
pub struct ValueWatcher<T> {
    value: Option<T>,
    filter: Box<dyn Fn(&T) -> bool>,
}
unsafe impl <T> Send for ValueWatcher<T> { }
unsafe impl <T> Sync for ValueWatcher<T> { }
impl<T> ValueWatcher<T> {
    pub fn resolved(&self) -> bool {
        if let Some(value) = &self.value {
            (self.filter)(value)
        } else {
            false
        }
    }
}
#[derive(Component)]
pub struct ValueReporter {
    promise: PromiseId,
    resolved: bool
}


pub fn resource_changed<R: Resource, V: BindableSource + BindableTarget, F: 'static + Fn(&V) -> bool>(value: FromResource<R, V>, filter: F) -> Promise<(), ()> {
    Promise::register(
        move |world, id| {
            let entity = world.spawn((
                ValueWatcher { 
                    filter: Box::new(filter),
                    value: None,
                },
                ValueReporter {
                    promise: id,
                    resolved: false
                }
            )).id();
            let watch = value >> to!(entity, ValueWatcher::<V>:value|some);
            let report = from!(entity, ValueWatcher::<V>:resolved()) >> to!(entity, ValueReporter:resolved);
            watch.write(world);
            report.write(world);

        },
        move |world, id| {
            if let Some(entity) = world.query::<(Entity, &ValueReporter)>()
                .iter(world)
                .filter(|(_, r)| r.promise == id)
                .map(|(e, _)| e)
                .next()
            {
                // info!("despawning from ")
                world.entity_mut(entity).despawn();        
            }
        }
    )
}

fn resolve_value_changes(
    mut commands: Commands,
    changed: Query<(Entity, &ValueReporter), Changed<ValueReporter>>,
) {
    for (entity, reporter) in changed.iter() {
        if reporter.resolved {
            commands.entity(entity).despawn();
            commands.promise(reporter.promise).resolve(())
        }
    }
}

pub struct SignalsOps<S>(S);
impl<S: 'static> SignalsOps<S> {
    pub fn pressed(self, entity: Entity) -> Promise<S, ()> {
        pressed(entity).with(self.0)
    }

    pub fn resource_changed<R: Resource, V: BindableSource + BindableTarget>(
        self, value: FromResource<R, V>, filter: fn(&V) -> bool
    ) -> Promise<S, ()> {
        resource_changed(value, filter).with(self.0)
    }
}

pub trait SignalOpsExt<S: 'static> {
    fn signals(self) -> SignalsOps<S>;
}
impl<S: 'static> SignalOpsExt<S> for AsynOps<S> {
    fn signals(self) -> SignalsOps<S> {
        SignalsOps(self.0)
    }
}

#[derive(Resource)]
pub struct SignalsRegistry<S: Signal> {
    marker: PhantomData<S>,
    entities: HashMap<PromiseId, (Entity, fn(&S) -> bool)>,
    promises: HashMap<Entity, HashSet<PromiseId>>,
}

impl<S: Signal> SignalsRegistry<S> {
    pub fn new() -> Self {
        SignalsRegistry {
            marker: PhantomData,
            entities: HashMap::new(),
            promises: HashMap::new(),
        }
    }
    pub fn register_promise(&mut self, promise: PromiseId, entity: Entity, filter: fn(&S) -> bool) {
        self.entities.insert(promise, (entity, filter));
        self.promises
            .entry(entity)
            .or_insert_with(HashSet::new)
            .insert(promise);
    }
    pub fn discard_promise(&mut self, promise: PromiseId) {
        if let Some((entity, _)) = self.entities.remove(&promise) {
            self.promises
                .entry(entity)
                .or_insert_with(HashSet::new)
                .remove(&promise);
        }
    }
    pub fn drain_promises_for(&mut self, signal: &S) -> HashSet<PromiseId> {
        let mut result = HashSet::new();
        let mut drop_promises = vec![];
        let mut drop_entities = vec![];
        for entity in signal.sources() {
            if let Some(promises) = self.promises.get_mut(entity) {
                for promise in promises.drain_filter(|p| {
                    if let Some((_, filter)) = self.entities.get(p) {
                        filter(signal) 
                    } else {
                        false
                    }
                }) {
                    result.insert(promise);
                    drop_promises.push(promise);
                }
                if promises.is_empty() {
                    drop_entities.push(*entity);
                }
            }
        }
        for entity in drop_entities {
            self.promises.remove(&entity);
        }
        for promise in drop_promises {
            self.entities.remove(&promise);
        }
        result
    }
}

fn resolve_signals<S: Signal>(
    mut commands: Commands,
    mut registry: ResMut<SignalsRegistry<S>>,
    mut events: EventReader<S>,
) {
    for event in events.iter() {
        for promise in registry.drain_promises_for(event) {
            commands.promise(promise).resolve(())
        }
    }
}
