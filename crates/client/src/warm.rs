//! Warm: everything this client draws has been drawn once and every
//! pipeline built, so a match starts without a stall. In a browser the
//! page waits for this before it seats us (`play`), drawing a level behind
//! its menu meanwhile (`link::Pending`); a pipeline is the same for every
//! map, so one level warms them all.

use bevy::prelude::*;

use crate::look::prewarm::{WarmModels, Warming};
use crate::pipelines::PipelineBacklog;

/// Frames everything must stay clear before it counts as warm: the
/// render world's count arrives a frame late (Gates' `DONE_FRAMES`).
const CLEAR_FRAMES: u32 = 3;
/// Frames the pipeline count may stay up, unchanging, once the rest is
/// in, before we go anyway (Gates' `PIPELINE_STALL_FRAMES`).
const STALL_FRAMES: u32 = 300;

#[derive(Resource, Default)]
pub struct Warm {
    pub done: bool,
    clear: u32,
    stuck: u32,
    last: u32,
}

pub fn check(
    mut warm: ResMut<Warm>,
    models: Res<WarmModels>,
    textures: Res<crate::look::textures::Textures>,
    warming: Query<(), With<Warming>>,
    backlog: Res<PipelineBacklog>,
) {
    if warm.done {
        return;
    }
    let loaded = models.0 && textures.ready() && warming.is_empty();
    let n = backlog.pending();
    if loaded && n > 0 {
        warm.stuck = if n == warm.last { warm.stuck + 1 } else { 0 };
    }
    warm.last = n;
    if loaded && (n == 0 || warm.stuck >= STALL_FRAMES) {
        warm.clear += 1;
    } else {
        warm.clear = 0;
    }
    if warm.clear >= CLEAR_FRAMES {
        if n > 0 {
            warn!("warm: {n} pipeline(s) still building after {STALL_FRAMES} frames; going anyway");
        }
        warm.done = true;
    }
}
