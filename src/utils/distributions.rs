//! Distribution utilities
use rand::distributions::{Bernoulli as BoolBernoulli, Distribution};
use rand::prelude::*;
use std::cmp::PartialOrd;
use std::convert::Infallible;

/// Distributions that can be constructed from a mean.
pub trait FromMean<T>
where
    Self: Sized,
{
    type Error;

    fn from_mean(mean: T) -> Result<Self, Self::Error>;
}

/// Bounds on a scalar value
pub trait Bounded<T: PartialOrd> {
    /// Minimum and maximum values (inclusive).
    ///
    /// Values x must satisfy min <= x && x <= max.
    /// If max < min then the interval is empty.
    fn bounds(&self) -> (T, T);
}

/// A determistic distribution.
///
/// Always produces the same value when sampled.
#[derive(Debug)]
pub struct Deterministic<T>(T);

impl<T> Deterministic<T> {
    pub fn new(value: T) -> Self {
        Self(value)
    }
}

impl<T: Copy> Distribution<T> for Deterministic<T> {
    fn sample<R: Rng + ?Sized>(&self, _rng: &mut R) -> T {
        self.0
    }
}
impl Bounded<f64> for Deterministic<f64> {
    fn bounds(&self) -> (f64, f64) {
        (self.0, self.0)
    }
}
impl<T> FromMean<T> for Deterministic<T> {
    type Error = Infallible;

    fn from_mean(mean: T) -> Result<Self, Self::Error> {
        Ok(Self::new(mean))
    }
}

/// Bernoulli distribution that can sample floats
#[derive(Debug)]
pub struct Bernoulli(BoolBernoulli);
impl Bernoulli {
    pub fn new(mean: f64) -> Result<Bernoulli, rand::distributions::BernoulliError> {
        Ok(Self(BoolBernoulli::new(mean)?))
    }
}
impl Distribution<f64> for Bernoulli {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> f64 {
        self.0.sample(rng) as u8 as f64
    }
}
impl Bounded<f64> for Bernoulli {
    fn bounds(&self) -> (f64, f64) {
        (0.0, 1.0)
    }
}
impl FromMean<f64> for Bernoulli {
    type Error = rand::distributions::BernoulliError;

    fn from_mean(mean: f64) -> Result<Self, Self::Error> {
        Self::new(mean)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asserts that distribution samples do not violate its reported bounds
    fn check_bounded<T: PartialOrd, D: Distribution<T> + Bounded<T>>(
        d: &D,
        num_samples: usize,
        seed: u64,
    ) {
        let (lower_bound, upper_bound) = d.bounds();
        let mut rng = StdRng::seed_from_u64(seed);
        for _ in 0..num_samples {
            let x: T = d.sample(&mut rng);
            assert!(x >= lower_bound);
            assert!(x <= upper_bound);
        }
    }

    /// Asserts that the distribution empirical mean is close to the given expected value.
    ///
    /// # Args
    /// * `d` - The distribution to test. d - E[d] must be sub-gaussian.
    /// * `expected_mean` - The expected value for the mean.
    /// * `num_samples` - Number of samples to generate.
    /// * `stddev_upper_bound` - An upper bound on the standard deviation of a single sample.
    /// * `seed` - Random seed.
    fn check_mean<D: Distribution<f64> + FromMean<f64>>(
        d: &D,
        expected_mean: f64,
        num_samples: usize,
        stddev_upper_bound: f64,
        seed: u64,
    ) {
        let rng = StdRng::seed_from_u64(seed);
        let empirical_mean = Distribution::<f64>::sample_iter(&d, rng)
            .take(num_samples)
            .sum::<f64>()
            / (num_samples as f64);
        // Want to be close enough to the mean to have false positive probability < 1e-5
        let false_positive_prob: f64 = 1e-5;
        let error_bound =
            stddev_upper_bound * (-2.0 / (num_samples as f64) * false_positive_prob.ln()).sqrt();
        // Make sure our error bound isn't huge
        assert!(
            error_bound < 0.1 || error_bound < 0.1 * expected_mean.abs(),
            "Use more samples"
        );
        assert!((empirical_mean - expected_mean) < error_bound);
    }

    #[test]
    fn deterministic_bounded() {
        check_bounded(&Deterministic::new(0.7), 1000, 1);
    }

    #[test]
    fn deterministic_mean() {
        check_mean(&Deterministic::new(0.7), 0.7, 100, 1e-6, 2);
    }

    #[test]
    fn bernoulli_bounded() {
        check_bounded(&Bernoulli::new(0.7).unwrap(), 1000, 1);
    }

    #[test]
    fn bernoulli_mean() {
        let p: f64 = 0.7;
        let stddev = (p * (1.0 - p)).sqrt();
        check_mean(&Bernoulli::new(p).unwrap(), p, 1000, stddev, 2);
    }
}