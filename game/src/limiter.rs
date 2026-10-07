use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::render::{Render, RenderApp, RenderSystems};

use crate::{common, config};

/// Sleeping wakes late by up to this much, so the last stretch is spun.
const SPIN: Duration = Duration::from_micros(500);

/// Holds frames to a period: each `wait` returns no earlier than one period after the last.
#[derive(Clone, Debug)]
pub struct Limiter {
    pub period: Duration,
    next: Instant,
}

impl Limiter {
    pub fn new(fps: u32) -> Self {
        Limiter { period: Duration::from_secs_f64(1.0 / fps as f64), next: Instant::now() }
    }

    pub fn wait(&mut self) {
        let now = Instant::now();
        if self.next > now + SPIN {
            std::thread::sleep(self.next - now - SPIN);
        }
        while Instant::now() < self.next {
            std::hint::spin_loop();
        }
        self.next = (self.next + self.period).max(Instant::now());
    }
}

#[derive(Resource)]
struct FrameLimiter(Limiter);

fn limit(mut l: ResMut<FrameLimiter>) {
    l.0.wait();
}

/// The fps cap. Windowed without pipelining it waits in the render sub-app after present,
/// so the sleep counts in the next frame's delta and none sits between input and present.
/// Off in script mode.
pub struct LimiterPlugin;

impl Plugin for LimiterPlugin {
    fn build(&self, app: &mut App) {
        common(app);
        let cfg = config(app);
        if cfg.scripted || cfg.fps_max == 0 {
            return;
        }
        let limiter = FrameLimiter(Limiter::new(cfg.fps_max));
        match app.get_sub_app_mut(RenderApp).filter(|_| !cfg.pipelined) {
            Some(render) => {
                render.insert_resource(limiter).add_systems(Render, limit.in_set(RenderSystems::Cleanup));
            }
            None => {
                app.insert_resource(limiter).add_systems(Last, limit);
            }
        }
    }
}
