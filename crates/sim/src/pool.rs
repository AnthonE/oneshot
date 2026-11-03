//! A fixed number of slots for things that come and go (projectiles, later
//! vehicles): no allocation after setup, and the slot a new thing takes is
//! always the first free one, so the same inputs fill the same slots native
//! and wasm.

use crate::thing::Thing;
use core::ops::{Deref, DerefMut};

#[derive(Clone, Debug, PartialEq)]
pub struct Pool<T, const N: usize> {
    items: [T; N],
}

impl<T: Thing, const N: usize> Default for Pool<T, N> {
    fn default() -> Self {
        Pool {
            items: [T::default(); N],
        }
    }
}

impl<T: Thing, const N: usize> Pool<T, N> {
    /// Puts `t` in the first free slot and returns it; `None` when all are
    /// in play (the thing is never made, as when a gun is fired dry).
    pub fn alloc(&mut self, t: T) -> Option<usize> {
        let i = self.items.iter().position(|x| !x.live())?;
        self.items[i] = t;
        Some(i)
    }

    /// The slots in play, in slot order.
    pub fn live(&self) -> impl Iterator<Item = (usize, &T)> {
        self.items.iter().enumerate().filter(|(_, t)| t.live())
    }
}

impl<T, const N: usize> Deref for Pool<T, N> {
    type Target = [T; N];
    fn deref(&self) -> &[T; N] {
        &self.items
    }
}

impl<T, const N: usize> DerefMut for Pool<T, N> {
    fn deref_mut(&mut self) -> &mut [T; N] {
        &mut self.items
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::Fnv;

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct Shot(u8);

    impl Thing for Shot {
        fn live(&self) -> bool {
            self.0 != 0
        }
        fn hash(&self, h: &mut Fnv) {
            h.u8(self.0);
        }
    }

    #[test]
    fn the_first_free_slot_is_taken_and_a_full_pool_refuses() {
        let mut p: Pool<Shot, 3> = Pool::default();
        assert_eq!(p.alloc(Shot(1)), Some(0));
        assert_eq!(p.alloc(Shot(2)), Some(1));
        p[0] = Shot(0);
        assert_eq!(p.alloc(Shot(3)), Some(0), "a freed slot is reused first");
        assert_eq!(p.alloc(Shot(4)), Some(2));
        assert_eq!(p.alloc(Shot(5)), None);
        let live: Vec<_> = p.live().map(|(i, s)| (i, s.0)).collect();
        assert_eq!(live, [(0, 3), (1, 2), (2, 4)]);
    }
}
