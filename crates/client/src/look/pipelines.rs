//! How many render pipelines are still compiling, read from the render
//! world: after Gates' `client/src/render/pipelines.rs` (MIT).
//!
//! On the desktop a draw whose pipeline isn't ready is skipped, so a screen
//! that ends the moment its level is spawned shows a flat clear colour, or
//! a level with pieces missing, for the seconds the compiles take. The
//! title's curtain and a match's loading overlay (`loading.rs`) wait for
//! this to reach zero as well. In a browser (no threads) a pipeline is
//! built inside the frame that first draws with it, so the count there is
//! mostly the ones waiting on a shader asset.
//!
//! `PipelineCache::waiting_pipelines` holds both the pipelines being
//! created and those waiting on a shader that hasn't loaded (re-queued every
//! frame). One that failed for good isn't in it, so a broken shader can't
//! hold the count up; `loading.rs` keeps a stall guard anyway.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use bevy::prelude::*;
use bevy::render::render_resource::PipelineCache;
use bevy::render::{Render, RenderApp, RenderSystems};

/// Pipelines still compiling as of the render world's last frame, shared
/// between the two worlds; 0 with no render world.
#[derive(Resource, Clone, Default)]
pub struct PipelineBacklog(Arc<AtomicU32>);

impl PipelineBacklog {
    pub fn pending(&self) -> u32 {
        self.0.load(Ordering::Relaxed)
    }
}

pub fn plugin(app: &mut App) {
    let backlog = PipelineBacklog::default();
    app.insert_resource(backlog.clone());
    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app
            .insert_resource(backlog)
            .add_systems(Render, count.in_set(RenderSystems::Cleanup));
    }
}

/// After `RenderSystems::Render`, where the cache works its queue, so this
/// frame's new pipelines are already counted.
fn count(cache: Res<PipelineCache>, backlog: Res<PipelineBacklog>) {
    let n = cache.waiting_pipelines().count() as u32;
    backlog.0.store(n, Ordering::Relaxed);
}
