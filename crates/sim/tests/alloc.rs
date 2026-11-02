//! No allocation in the tick after setup (docs/DESIGN.md §17): a counting
//! allocator around a bot match.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use sim::bot::Bot;
use sim::consts::MAX_FIGHTERS;
use sim::input::Input;
use sim::world::{Rules, World};
use sim::Map;

struct Counting;

static ALLOCS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}

#[global_allocator]
static A: Counting = Counting;

#[test]
fn the_tick_allocates_nothing() {
    let rules = Rules {
        score_limit: 1000,
        time_limit_ms: 0,
        ..Rules::doubles()
    };
    let mut w = World::new(Map::proving_grounds(), rules, 5, (1, 2));
    let mut bots = [Bot::new(5, 0, 0.5); MAX_FIGHTERS];
    for (s, b) in bots.iter_mut().enumerate().take(4) {
        w.seat(s, (s & 1) as u8);
        *b = Bot::new(5, s, 0.6);
    }
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    let lag = [3 << 8; MAX_FIGHTERS];
    let mut run = |w: &mut World, n: u32| {
        for _ in 0..n {
            for s in 0..4 {
                inputs[s] = bots[s].think(w, s);
            }
            w.step(&inputs, &lag);
        }
    };
    run(&mut w, 500);
    let before = ALLOCS.load(Ordering::Relaxed);
    run(&mut w, 10_000);
    assert_eq!(ALLOCS.load(Ordering::Relaxed) - before, 0);
}
