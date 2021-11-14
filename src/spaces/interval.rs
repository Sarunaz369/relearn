//! `IntervalSpace` definition
use super::{ElementRefInto, EncoderFeatureSpace, NumFeatures, ReprSpace, Space};
use crate::logging::Loggable;
use num_traits::{Float, ToPrimitive};
use rand::distributions::Distribution;
use rand::Rng;
use rand_distr::{Gamma, StandardNormal};
use std::cmp::Ordering;
use std::{fmt, slice};
use tch::Tensor;

/// A closed interval of floating-point numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IntervalSpace<T = f64> {
    pub low: T,
    pub high: T,
}

impl<T: PartialOrd> IntervalSpace<T> {
    pub fn new(low: T, high: T) -> Self {
        assert!(low <= high, "require low <= high");
        Self { low, high }
    }
}

/// The default interval is the full real number line.
impl<T: Float> Default for IntervalSpace<T> {
    fn default() -> Self {
        Self {
            low: T::neg_infinity(),
            high: T::infinity(),
        }
    }
}

impl<T: fmt::Display> fmt::Display for IntervalSpace<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "IntervalSpace({}, {})", self.low, self.high)
    }
}

impl<T: Float> Space for IntervalSpace<T> {
    type Element = T;

    fn contains(&self, value: &Self::Element) -> bool {
        &self.low <= value && value <= &self.high && value.is_finite()
    }
}

impl<T: PartialOrd> PartialOrd for IntervalSpace<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.low == other.low && self.high == other.high {
            Some(Ordering::Equal)
        } else if self.low >= other.low && self.high <= other.high {
            Some(Ordering::Less)
        } else if self.low <= other.low && self.high >= other.high {
            Some(Ordering::Greater)
        } else {
            None
        }
    }
}

/// Represent elements as the same type in a tensor.
impl<T> ReprSpace<Tensor> for IntervalSpace<T>
where
    // NOTE: Remove copy if ever changing batch_repr to take a slice
    T: Copy + Float + tch::kind::Element,
{
    fn repr(&self, element: &Self::Element) -> Tensor {
        Tensor::of_slice(slice::from_ref(element)).squeeze_dim_(0)
    }

    fn batch_repr<'a, I>(&self, elements: I) -> Tensor
    where
        I: IntoIterator<Item = &'a Self::Element>,
        I::IntoIter: ExactSizeIterator + Clone,
        Self::Element: 'a,
    {
        let elements: Vec<_> = elements.into_iter().cloned().collect();
        Tensor::of_slice(&elements)
    }
}

impl<T: Float> NumFeatures for IntervalSpace<T> {
    fn num_features(&self) -> usize {
        1
    }
}

impl<T: Float + ToPrimitive> EncoderFeatureSpace for IntervalSpace<T> {
    type Encoder = ();
    fn encoder(&self) -> Self::Encoder {}
    fn encoder_features_out<F: Float>(
        &self,
        element: &Self::Element,
        out: &mut [F],
        _zeroed: bool,
        _encoder: &Self::Encoder,
    ) {
        out[0] = F::from(*element).expect("could not convert element to float")
    }
}

impl Distribution<f32> for IntervalSpace<f32> {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> <Self as Space>::Element {
        match (self.low.is_finite(), self.high.is_finite()) {
            (true, true) => rng.gen_range(self.low..=self.high),
            (true, false) => self.low + Gamma::new(1.0, 1.0).unwrap().sample(rng),
            (false, true) => self.high - Gamma::new(1.0, 1.0).unwrap().sample(rng),
            (false, false) => StandardNormal.sample(rng),
        }
    }
}

impl Distribution<f64> for IntervalSpace<f64> {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> <Self as Space>::Element {
        match (self.low.is_finite(), self.high.is_finite()) {
            (true, true) => rng.gen_range(self.low..=self.high),
            (true, false) => self.low + Gamma::new(1.0, 1.0).unwrap().sample(rng),
            (false, true) => self.high - Gamma::new(1.0, 1.0).unwrap().sample(rng),
            (false, false) => StandardNormal.sample(rng),
        }
    }
}

impl<T: Copy + PartialOrd + Float + Into<f64>> ElementRefInto<Loggable> for IntervalSpace<T> {
    fn elem_ref_into(&self, element: &Self::Element) -> Loggable {
        Loggable::Scalar((*element).into())
    }
}

#[cfg(test)]
mod space {
    use super::super::testing;
    use super::*;

    #[test]
    fn unit_contains_0() {
        let space = IntervalSpace::new(0.0, 1.0);
        assert!(space.contains(&0.0));
    }

    #[test]
    fn unit_contains_half() {
        let space = IntervalSpace::new(0.0, 1.0);
        assert!(space.contains(&0.5));
    }

    #[test]
    fn unit_contains_1() {
        let space = IntervalSpace::new(0.0, 1.0);
        assert!(space.contains(&1.0));
    }

    #[test]
    fn unit_not_contains_2() {
        let space = IntervalSpace::new(0.0, 1.0);
        assert!(!space.contains(&2.0));
    }

    #[test]
    fn unit_not_contains_neg_1() {
        let space = IntervalSpace::new(0.0, 1.0);
        assert!(!space.contains(&-1.0));
    }

    #[test]
    fn unit_contains_samples() {
        let space = IntervalSpace::new(0.0, 1.0);
        testing::check_contains_samples(&space, 20);
    }

    #[test]
    fn unbounded_contains_0() {
        let space = IntervalSpace::default();
        assert!(space.contains(&0.0));
    }

    #[test]
    fn unbounded_contains_100() {
        let space = IntervalSpace::default();
        assert!(space.contains(&100.0));
    }

    #[test]
    fn unbounded_contains_neg_1() {
        let space = IntervalSpace::default();
        assert!(space.contains(&-1.0));
    }

    #[test]
    fn unbounded_not_contains_inf() {
        let space = IntervalSpace::default();
        assert!(!space.contains(&f64::infinity()));
    }

    #[test]
    fn unbounded_not_contains_nan() {
        let space = IntervalSpace::default();
        assert!(!space.contains(&f64::nan()));
    }

    #[test]
    fn unbounded_contains_samples() {
        let space = IntervalSpace::<f64>::default();
        testing::check_contains_samples(&space, 20);
    }

    #[test]
    fn unbounded_eq_default() {
        let default = IntervalSpace::default();
        let unbounded = IntervalSpace::new(f64::NEG_INFINITY, f64::INFINITY);
        assert_eq!(default, unbounded);
    }

    #[test]
    fn half_contains_lower_bound() {
        let space = IntervalSpace::new(2.0, f64::infinity());
        assert!(space.contains(&2.0));
    }

    #[test]
    fn half_contains_samples() {
        let space = IntervalSpace::new(2.0, f64::infinity());
        testing::check_contains_samples(&space, 20);
    }

    #[test]
    fn point_contains_point() {
        let space = IntervalSpace::new(2.0, 2.0);
        assert!(space.contains(&2.0));
    }

    #[test]
    fn point_not_contains_outside() {
        let space = IntervalSpace::new(2.0, 2.0);
        assert!(!space.contains(&2.1));
    }

    #[test]
    fn point_contains_samples() {
        let space = IntervalSpace::new(2.0, 2.0);
        testing::check_contains_samples(&space, 5);
    }

    #[test]
    #[should_panic]
    fn empty_interval_panics() {
        let _ = IntervalSpace::new(1.0, 0.0);
    }
}

#[cfg(test)]
mod partial_ord {
    use super::*;

    #[test]
    fn same_eq() {
        assert_eq!(IntervalSpace::new(0.0, 1.0), IntervalSpace::new(0.0, 1.0));
    }

    #[test]
    fn same_cmp_equal() {
        assert_eq!(
            IntervalSpace::new(0.0, 1.0).partial_cmp(&IntervalSpace::new(0.0, 1.0)),
            Some(Ordering::Equal)
        );
    }

    #[test]
    fn different_ne() {
        assert!(IntervalSpace::new(0.0, 1.0) != IntervalSpace::new(0.5, 1.0));
    }

    #[test]
    fn subset_lt() {
        assert!(IntervalSpace::new(0.0, 1.0) < IntervalSpace::new(-1.0, 1.0));
    }

    #[test]
    fn same_le() {
        assert!(IntervalSpace::new(0.0, 1.0) <= IntervalSpace::new(0.0, 1.0));
    }

    #[test]
    fn superset_lt() {
        assert!(IntervalSpace::new(0.0, 1.0) > IntervalSpace::new(0.2, 0.8));
    }

    #[test]
    fn disjoint_incomparable() {
        assert_eq!(
            IntervalSpace::new(0.0, 1.0).partial_cmp(&IntervalSpace::new(2.0, 3.0)),
            None
        );
    }

    #[test]
    fn intersecting_incomparable() {
        assert_eq!(
            IntervalSpace::new(0.0, 2.0).partial_cmp(&IntervalSpace::new(1.0, 3.0)),
            None
        );
    }
}

#[cfg(test)]
mod feature_space {
    use super::super::FeatureSpace;
    use super::*;
    use crate::utils::tensor::UniqueTensor;

    fn tensor_from_arrays<T: tch::kind::Element, const N: usize, const M: usize>(
        data: [[T; M]; N],
    ) -> Tensor {
        let flat_data: Vec<T> = data
            .into_iter()
            .map(IntoIterator::into_iter)
            .flatten()
            .collect();
        Tensor::of_slice(&flat_data).reshape(&[N as i64, M as i64])
    }

    macro_rules! features_tests {
        ($label:ident, $space:expr, $elem:expr, $expected:expr) => {
            mod $label {
                use super::*;

                #[test]
                fn tensor_features() {
                    let space = $space;
                    let actual: Tensor = space.features(&$elem);
                    let expected_vec: &[f32] = &$expected;
                    assert_eq!(actual, Tensor::of_slice(expected_vec));
                }

                #[test]
                fn tensor_features_out() {
                    let space = $space;
                    let expected_vec: &[f32] = &$expected;
                    let expected = Tensor::of_slice(&expected_vec);
                    let mut out = UniqueTensor::<f32, _>::zeros(expected_vec.len());
                    space.features_out(&$elem, out.as_slice_mut(), true);
                    assert_eq!(out.into_tensor(), expected);
                }
            }
        };
    }

    macro_rules! batch_features_tests {
        ($label:ident, $space:expr, $elems:expr, $expected:expr) => {
            mod $label {
                use super::*;

                #[test]
                fn tensor_batch_features() {
                    let space = $space;
                    let actual: Tensor = space.batch_features(&$elems);
                    assert_eq!(actual, tensor_from_arrays($expected));
                }

                #[test]
                fn tensor_batch_features_out() {
                    let space = $space;
                    let expected = tensor_from_arrays($expected);
                    let (a, b) = expected.size2().unwrap();
                    let mut out = UniqueTensor::<f32, _>::zeros((a as _, b as _));
                    space.batch_features_out(&$elems, &mut out.array_view_mut(), false);
                    assert_eq!(out.into_tensor(), expected);
                }
            }
        };
    }

    mod unit {
        use super::*;

        #[test]
        fn num_features() {
            let space = IntervalSpace::new(0.0, 1.0);
            assert_eq!(space.num_features(), 1);
        }

        features_tests!(zero, IntervalSpace::new(0.0, 1.0), 0.0, [0.0]);
        features_tests!(one, IntervalSpace::new(0.0, 1.0), 1.0, [1.0]);
        features_tests!(half, IntervalSpace::new(0.0, 1.0), 0.5, [0.5]);
        batch_features_tests!(
            zero_one_half,
            IntervalSpace::new(0.0, 1.0),
            [0.0, 1.0, 0.5],
            [[0.0], [1.0], [0.5]]
        );
    }

    mod neg_pos_one {
        use super::*;

        #[test]
        fn num_features() {
            let space = IntervalSpace::new(-1.0, 1.0);
            assert_eq!(space.num_features(), 1);
        }

        features_tests!(zero, IntervalSpace::new(-1.0, 1.0), 0.0, [0.0]);
        features_tests!(neg_one, IntervalSpace::new(-1.0, 1.0), -1.0, [-1.0]);
        batch_features_tests!(
            zero_neg_one,
            IntervalSpace::new(-1.0, 1.0),
            [0.0, -1.0],
            [[0.0], [-1.0]]
        );
    }

    mod unbounded {
        use super::*;

        #[test]
        fn num_features() {
            let space = IntervalSpace::<f64>::default();
            assert_eq!(space.num_features(), 1);
        }

        features_tests!(zero, IntervalSpace::default(), 0.0, [0.0]);
        features_tests!(ten, IntervalSpace::default(), 10.0, [10.0]);
        batch_features_tests!(
            zero_ten,
            IntervalSpace::default(),
            [0.0, 10.0],
            [[0.0], [10.0]]
        );
    }
}
