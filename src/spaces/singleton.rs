//! Singleton space definition.
use super::{ElementRefInto, FiniteSpace, SampleSpace, Space};
use crate::logging::Loggable;
use rand::distributions::Distribution;
use rand::Rng;
use std::fmt;

/// A space containing a single element.
#[derive(Debug, Clone)]
pub struct SingletonSpace;

impl SingletonSpace {
    pub const fn new() -> Self {
        SingletonSpace
    }
}

impl Default for SingletonSpace {
    fn default() -> Self {
        SingletonSpace
    }
}

impl fmt::Display for SingletonSpace {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "SingletonSpace")
    }
}

impl Space for SingletonSpace {
    type Element = ();

    fn contains(&self, _value: &Self::Element) -> bool {
        true
    }
}

impl FiniteSpace for SingletonSpace {
    fn size(&self) -> usize {
        1
    }

    fn to_index(&self, _element: &Self::Element) -> usize {
        0
    }

    fn from_index(&self, index: usize) -> Option<Self::Element> {
        if index == 0 {
            Some(())
        } else {
            None
        }
    }

    fn from_index_unchecked(&self, _index: usize) -> Option<Self::Element> {
        Some(())
    }
}

impl Distribution<<Self as Space>::Element> for SingletonSpace {
    fn sample<R: Rng + ?Sized>(&self, _rng: &mut R) -> <Self as Space>::Element {}
}

impl SampleSpace for SingletonSpace {}

impl ElementRefInto<Loggable> for SingletonSpace {
    fn elem_ref_into(&self, _element: &Self::Element) -> Loggable {
        Loggable::Nothing
    }
}

#[cfg(test)]
mod singleton_space {
    use super::super::testing;
    use super::*;

    #[test]
    fn contains_unit() {
        let space = SingletonSpace::new();
        assert!(space.contains(&()));
    }

    #[test]
    fn contains_samples() {
        let space = SingletonSpace::new();
        testing::check_contains_samples(&space, 10);
    }

    #[test]
    fn from_to_index_iter_size() {
        let space = SingletonSpace::new();
        testing::check_from_to_index_iter_size(&space);
    }

    #[test]
    fn from_index_sampled() {
        let space = SingletonSpace::new();
        testing::check_from_index_sampled(&space, 10);
    }

    #[test]
    fn from_index_invalid() {
        let space = SingletonSpace::new();
        testing::check_from_index_invalid(&space);
    }
}
