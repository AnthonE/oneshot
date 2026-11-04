//! How many render pipelines are still being built, read from the render
//! world (Gates' `client/src/render/pipelines.rs`, MIT). The page waits for
//! none before seating us (`warm.rs`): a pipeline built mid-fight is a
//! frame-long stall in a browser.
//!
//! `PipelineCache::waiting_pipelines` counts the pipelines being created
//! and those waiting on a shader asset; one that failed for good isn't in
//! it, so a broken shader can't hold the count up (`warm.rs` keeps a stall
//! guard anyway).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use bevy::prelude::*;
use bevy::render::render_resource::PipelineCache;
use bevy::render::{Render, RenderApp, RenderSystems};

/// Pipelines still building as of the render world's last frame; 0 with
/// no render world.
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
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render
            .insert_resource(backlog)
            .add_systems(Render, count.in_set(RenderSystems::Cleanup));
    }
}

fn count(cache: Res<PipelineCache>, backlog: Res<PipelineBacklog>) {
    backlog
        .0
        .store(cache.waiting_pipelines().count() as u32, Ordering::Relaxed);
}
