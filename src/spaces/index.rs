use super::{BaseSpace, FiniteSpace, Space};
use crate::logging::Loggable;
use rand::distributions::Distribution;
use rand::Rng;
use std::fmt;

/// An index space; integers 0 .. size-1
#[derive(Debug, Clone)]
pub struct IndexSpace {
    pub size: usize,
}

impl IndexSpace {
    pub fn new(size: usize) -> Self {
        Self { size }
    }
}

impl fmt::Display for IndexSpace {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "IndexSpace({})", self.size)
    }
}

impl BaseSpace for IndexSpace {}

impl Space for IndexSpace {
    type Element = usize;

    fn contains(&self, value: &Self::Element) -> bool {
        value < &self.size
    }

    fn as_loggable(&self, value: &Self::Element) -> Loggable {
        Loggable::IndexSample {
            value: *value,
            size: self.size,
        }
    }
}

impl FiniteSpace for IndexSpace {
    fn size(&self) -> usize {
        self.size
    }

    fn to_index(&self, element: &Self::Element) -> usize {
        *element
    }

    fn from_index(&self, index: usize) -> Option<Self::Element> {
        Some(index)
    }
}

impl Distribution<<Self as Space>::Element> for IndexSpace {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> <Self as Space>::Element {
        rng.gen_range(0..self.size)
    }
}
