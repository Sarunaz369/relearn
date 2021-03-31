//! Thompson sampling bandit agent
use super::super::{Actor, Agent, Step};
use crate::spaces::FiniteSpace;
use crate::utils::iter::ArgMaxBy;
use ndarray::{Array, Array2, Axis};
use rand::distributions::Distribution;
use rand::prelude::*;
use statrs::distribution::Beta;
use std::fmt;

/// A Thompson sampling agent for Bernoulli rewards with a Beta prior.
#[derive(Debug)]
pub struct BetaThompsonSamplingAgent<OS, AS>
where
    OS: FiniteSpace,
    AS: FiniteSpace,
{
    /// Environment observation space
    pub observation_space: OS,
    /// Environment action space
    pub action_space: AS,
    /// Reward is partitioned into high/low separated by this threshold.
    pub reward_threshold: f64,
    /// Number of posterior samples to draw.
    /// Takes the action with the highest mean sampled value.
    pub num_samples: usize,

    /// Count of low and high rewards for each observation-action pair.
    low_high_reward_counts: Array2<(u64, u64)>,

    rng: StdRng,
}

impl<OS, AS> BetaThompsonSamplingAgent<OS, AS>
where
    OS: FiniteSpace,
    AS: FiniteSpace,
{
    pub fn new(
        observation_space: OS,
        action_space: AS,
        reward_range: (f64, f64),
        num_samples: usize,
        seed: u64,
    ) -> Self {
        let (reward_min, reward_max) = reward_range;
        let reward_threshold = (reward_min + reward_max) / 2.0;
        let low_high_reward_counts =
            Array::from_elem((observation_space.size(), action_space.size()), (1, 1));
        Self {
            observation_space,
            action_space,
            reward_threshold,
            num_samples,
            low_high_reward_counts,
            rng: StdRng::seed_from_u64(seed),
        }
    }
}

impl<OS, AS> fmt::Display for BetaThompsonSamplingAgent<OS, AS>
where
    OS: FiniteSpace + fmt::Display,
    AS: FiniteSpace + fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "BetaThompsonSamplingAgent({}, {}, {})",
            self.observation_space, self.action_space, self.reward_threshold
        )
    }
}

impl<OS, AS> Actor<OS::Element, AS::Element> for BetaThompsonSamplingAgent<OS, AS>
where
    OS: FiniteSpace,
    AS: FiniteSpace,
{
    fn act(&mut self, observation: &OS::Element, _new_episode: bool) -> AS::Element {
        let obs_idx = self.observation_space.to_index(observation);
        let num_samples = self.num_samples;
        let rng = &mut self.rng;
        let act_idx = self
            .low_high_reward_counts
            .index_axis(Axis(0), obs_idx)
            .map(|(beta, alpha)| -> f64 {
                // Explanation for the rng reference:
                // sample_iter takes its argument by value rather than by reference.
                // We cannot the rng into sample_iter because it needs to stay with self.
                // However, (&mut rng) also implements Rng.
                // We cannot directly use &mut self.rng in the closure because that would borrow
                // self. Nor can we create a copy like we do for num_samples.
                // So we create `rng` as a reference.
                // We cannot directly pass in `rng` because that would move it out of `rng` and
                // it needs to be available for multiple function calls.
                //
                // Therefore, the solution is to dereference rng first so that we have back the
                // original rng (without naming `self` within the closure) and then reference it
                // again in the closure so that the reference is created local to the closure and
                // can be safely moved.
                Beta::new(*alpha as f64, *beta as f64)
                    .unwrap()
                    .sample_iter(&mut *rng)
                    .take(num_samples)
                    .sum()
            })
            .into_iter()
            .argmax_by(|a, b| a.partial_cmp(b).unwrap())
            .expect("Empty action space");
        self.action_space.from_index(act_idx).unwrap()
    }
}

impl<OS, AS> Agent<OS::Element, AS::Element> for BetaThompsonSamplingAgent<OS, AS>
where
    OS: FiniteSpace,
    AS: FiniteSpace,
{
    fn update(&mut self, step: Step<OS::Element, AS::Element>) {
        let obs_idx = self.observation_space.to_index(&step.observation);
        let act_idx = self.action_space.to_index(&step.action);

        let reward_count = self
            .low_high_reward_counts
            .get_mut((obs_idx, act_idx))
            .unwrap();
        if step.reward > self.reward_threshold {
            reward_count.1 += 1;
        } else {
            reward_count.0 += 1;
        }
    }
}

#[cfg(test)]
mod beta_thompson_sampling {
    use super::super::super::testing;
    use super::*;

    #[test]
    fn train() {
        testing::train_deterministic_bandit(
            |env_structure| {
                BetaThompsonSamplingAgent::new(
                    env_structure.observation_space,
                    env_structure.action_space,
                    env_structure.reward_range,
                    1,
                    0,
                )
            },
            1000,
            0.9,
        );
    }
}