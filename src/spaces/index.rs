//! `IndexSpace` definition
use super::{
    FeatureSpace, FiniteSpace, LogElementSpace, NonEmptySpace, ParameterizedDistributionSpace,
    ReprSpace, Space, SubsetOrd,
};
use crate::logging::{LogError, LogValue, StatsLogger};
use crate::torch::distributions::Categorical;
use crate::utils::distributions::ArrayDistribution;
use ndarray::{s, ArrayBase, DataMut, Ix2};
use num_traits::{Float, One, Zero};
use rand::distributions::Distribution;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;
use tch::{Device, Kind, Tensor};

/// An index space; consists of the integers `0` to `size - 1`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IndexSpace {
    pub size: usize,
}

impl IndexSpace {
    #[must_use]
    #[inline]
    pub const fn new(size: usize) -> Self {
        Self { size }
    }
}

impl fmt::Display for IndexSpace {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "IndexSpace({})", self.size)
    }
}

impl Space for IndexSpace {
    type Element = usize;

    #[inline]
    fn contains(&self, value: &Self::Element) -> bool {
        value < &self.size
    }
}

impl SubsetOrd for IndexSpace {
    #[inline]
    fn subset_cmp(&self, other: &Self) -> Option<Ordering> {
        self.size.partial_cmp(&other.size)
    }
}

impl NonEmptySpace for IndexSpace {
    #[inline]
    fn some_element(&self) -> <Self as Space>::Element {
        assert_ne!(self.size, 0, "space is empty");
        0
    }
}

impl Distribution<<Self as Space>::Element> for IndexSpace {
    #[inline]
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> <Self as Space>::Element {
        rng.gen_range(0..self.size)
    }
}

impl FiniteSpace for IndexSpace {
    #[inline]
    fn size(&self) -> usize {
        self.size
    }

    #[inline]
    fn to_index(&self, element: &Self::Element) -> usize {
        *element
    }

    #[inline]
    fn from_index(&self, index: usize) -> Option<Self::Element> {
        if index >= self.size {
            None
        } else {
            Some(index)
        }
    }

    #[inline]
    fn from_index_unchecked(&self, index: usize) -> Option<Self::Element> {
        Some(index)
    }
}

/// Features are one-hot vectors
impl FeatureSpace for IndexSpace {
    #[inline]
    fn num_features(&self) -> usize {
        self.size
    }

    #[inline]
    fn features_out<'a, F: Float>(
        &self,
        element: &Self::Element,
        out: &'a mut [F],
        zeroed: bool,
    ) -> &'a mut [F] {
        let (out, rest) = out.split_at_mut(self.size);
        if !zeroed {
            out.fill(F::zero());
        }
        out[self.to_index(element)] = F::one();
        rest
    }

    #[inline]
    fn batch_features_out<'a, I, A>(&self, elements: I, out: &mut ArrayBase<A, Ix2>, zeroed: bool)
    where
        I: IntoIterator<Item = &'a Self::Element>,
        Self::Element: 'a,
        A: DataMut,
        A::Elem: Float,
    {
        if !zeroed {
            out.slice_mut(s![.., 0..self.num_features()])
                .fill(Zero::zero());
        }

        // Don't zip rows so that we can check whether there are too few rows.
        let mut rows = out.rows_mut().into_iter();
        for element in elements {
            let mut row = rows.next().expect("fewer rows than elements");
            row[self.to_index(element)] = One::one();
        }
    }
}

/// Represents elements as integer tensors.
impl ReprSpace<Tensor> for IndexSpace {
    #[inline]
    fn repr(&self, element: &Self::Element) -> Tensor {
        Tensor::scalar_tensor(self.to_index(element) as i64, (Kind::Int64, Device::Cpu))
    }

    #[inline]
    fn batch_repr<'a, I>(&self, elements: I) -> Tensor
    where
        I: IntoIterator<Item = &'a Self::Element>,
        Self::Element: 'a,
    {
        let indices: Vec<_> = elements
            .into_iter()
            .map(|elem| self.to_index(elem) as i64)
            .collect();
        Tensor::of_slice(&indices)
    }
}

impl ParameterizedDistributionSpace<Tensor> for IndexSpace {
    type Distribution = Categorical;

    #[inline]
    fn num_distribution_params(&self) -> usize {
        self.size
    }

    #[inline]
    fn sample_element(&self, params: &Tensor) -> Self::Element {
        self.from_index(
            self.distribution(params)
                .sample()
                .int64_value(&[])
                .try_into()
                .unwrap(),
        )
        .unwrap()
    }

    #[inline]
    fn distribution(&self, params: &Tensor) -> Self::Distribution {
        Self::Distribution::new(params)
    }
}

/// Log the index as a sample from `0..N`
impl LogElementSpace for IndexSpace {
    #[inline]
    fn log_element<L: StatsLogger + ?Sized>(
        &self,
        name: &'static str,
        element: &Self::Element,
        logger: &mut L,
    ) -> Result<(), LogError> {
        let log_value = LogValue::Index {
            value: self.to_index(element),
            size: self.size,
        };
        logger.log(name.into(), log_value)
    }
}

impl<T: FiniteSpace + ?Sized> From<&T> for IndexSpace {
    #[inline]
    fn from(space: &T) -> Self {
        Self { size: space.size() }
    }
}

#[cfg(test)]
mod space {
    use super::super::testing;
    use super::*;
    use rstest::rstest;

    #[rstest]
    fn contains_zero(#[values(1, 5)] size: usize) {
        let space = IndexSpace::new(size);
        assert!(space.contains(&0));
    }

    #[rstest]
    fn not_contains_too_large(#[values(1, 5)] size: usize) {
        let space = IndexSpace::new(size);
        assert!(!space.contains(&100));
    }

    #[rstest]
    fn contains_samples(#[values(1, 5)] size: usize) {
        let space = IndexSpace::new(size);
        testing::check_contains_samples(&space, 100);
    }
}

#[cfg(test)]
mod subset_ord {
    use super::super::SubsetOrd;
    use super::*;
    use std::cmp::Ordering;

    #[test]
    fn same_eq() {
        assert_eq!(IndexSpace::new(2), IndexSpace::new(2));
        assert_eq!(
            IndexSpace::new(2).subset_cmp(&IndexSpace::new(2)),
            Some(Ordering::Equal)
        );
    }

    #[test]
    fn different_not_eq() {
        assert!(IndexSpace::new(2) != IndexSpace::new(1));
        assert_ne!(
            IndexSpace::new(2).subset_cmp(&IndexSpace::new(1)),
            Some(Ordering::Equal)
        );
    }

    #[test]
    fn same_subset_of() {
        assert!(IndexSpace::new(2).subset_of(&IndexSpace::new(2)));
    }

    #[test]
    fn smaller_strict_subset_of() {
        assert!(IndexSpace::new(1).strict_subset_of(&IndexSpace::new(2)));
    }

    #[test]
    fn larger_not_subset_of() {
        assert!(!IndexSpace::new(3).subset_of(&IndexSpace::new(1)));
    }
}

#[cfg(test)]
mod finite_space {
    use super::super::testing;
    use super::*;
    use rstest::rstest;

    #[rstest]
    fn from_to_index_iter_size(#[values(1, 5)] size: usize) {
        let space = IndexSpace::new(size);
        testing::check_from_to_index_iter_size(&space);
    }

    #[rstest]
    fn from_index_sampled(#[values(1, 5)] size: usize) {
        let space = IndexSpace::new(size);
        testing::check_from_index_sampled(&space, 100);
    }

    #[rstest]
    fn from_index_invalid(#[values(1, 5)] size: usize) {
        let space = IndexSpace::new(size);
        testing::check_from_index_invalid(&space);
    }
}

#[cfg(test)]
mod feature_space {
    use super::*;

    #[test]
    fn num_features() {
        assert_eq!(IndexSpace::new(3).num_features(), 3);
    }

    features_tests!(f, IndexSpace::new(3), 1, [0.0, 1.0, 0.0]);
    batch_features_tests!(
        b,
        IndexSpace::new(3),
        [2, 0, 1],
        [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
    );
}

#[cfg(test)]
mod repr_space_tensor {
    use super::*;

    #[test]
    fn repr() {
        let space = IndexSpace::new(3);
        assert_eq!(
            space.repr(&0),
            Tensor::scalar_tensor(0, (Kind::Int64, Device::Cpu))
        );
        assert_eq!(
            space.repr(&1),
            Tensor::scalar_tensor(1, (Kind::Int64, Device::Cpu))
        );
        assert_eq!(
            space.repr(&2),
            Tensor::scalar_tensor(2, (Kind::Int64, Device::Cpu))
        );
    }

    #[test]
    fn batch_repr() {
        let space = IndexSpace::new(3);
        let elements = [0, 1, 2, 1];
        let actual = space.batch_repr(&elements);
        let expected = Tensor::of_slice(&[0_i64, 1, 2, 1]);
        assert_eq!(actual, expected);
    }
}

#[cfg(test)]
mod parameterized_sample_space_tensor {
    use super::*;
    use std::ops::RangeInclusive;

    #[test]
    fn num_sample_params() {
        let space = IndexSpace::new(3);
        assert_eq!(3, space.num_distribution_params());
    }

    #[test]
    fn sample_element_deterministic() {
        let space = IndexSpace::new(3);
        let params = Tensor::of_slice(&[f32::NEG_INFINITY, 0.0, f32::NEG_INFINITY]);
        for _ in 0..10 {
            assert_eq!(1, space.sample_element(&params));
        }
    }

    #[test]
    fn sample_element_two_of_three() {
        let space = IndexSpace::new(3);
        let params = Tensor::of_slice(&[f32::NEG_INFINITY, 0.0, 0.0]);
        for _ in 0..10 {
            assert!(0 != space.sample_element(&params));
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)] // negative f64 casts to 0.0 as desired
    fn bernoulli_confidence_interval(p: f64, n: u64) -> RangeInclusive<u64> {
        // Using Wald method <https://en.wikipedia.org/wiki/Binomial_distribution#Wald_method>
        // Quantile for error rate of 1e-5
        let z = 4.4;
        let nf = n as f64;
        let stddev = (p * (1.0 - p) * nf).sqrt();
        let lower_bound = nf * p - z * stddev;
        let upper_bound = nf * p + z * stddev;
        (lower_bound.round() as u64)..=(upper_bound.round() as u64)
    }

    #[test]
    fn sample_element_check_distribution() {
        let space = IndexSpace::new(3);
        let params = Tensor::of_slice(&[-1.0, 0.0, 1.0]);
        // Corresponding approximate probabilities
        let probs = [0.090, 0.245, 0.665];
        let n = 5000;

        let mut one_count = 0;
        let mut two_count = 0;
        let mut three_count = 0;
        for _ in 0..n {
            match space.sample_element(&params) {
                0 => one_count += 1,
                1 => two_count += 1,
                2 => three_count += 1,
                _ => panic!(),
            }
        }
        // Check that the counts are within their expected intervals
        let one_interval = bernoulli_confidence_interval(probs[0], n);
        let two_interval = bernoulli_confidence_interval(probs[1], n);
        let three_interval = bernoulli_confidence_interval(probs[2], n);
        assert!(one_interval.contains(&one_count));
        assert!(two_interval.contains(&two_count));
        assert!(three_interval.contains(&three_count));
    }
}
