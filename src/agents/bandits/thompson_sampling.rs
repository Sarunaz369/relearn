//! Thompson sampling bandit agent
use super::super::{
    ActorMode, BuildAgentError, BuildIndexAgent, FiniteSpaceAgent, PureActor, SetActorMode,
    SynchronousUpdate,
};
use crate::logging::TimeSeriesLogger;
use crate::simulation::TransientStep;
use crate::utils::iter::ArgMaxBy;
use ndarray::{Array, Array2, Axis};
use rand::distributions::Distribution;
use rand::prelude::*;
use rand_distr::Beta;
use std::fmt;

/// Configuration for [`BetaThompsonSamplingAgent`]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BetaThompsonSamplingAgentConfig {
    /// Number of posterior samples to draw.
    /// Takes the action with the highest mean sampled value.
    pub num_samples: usize,
}

impl BetaThompsonSamplingAgentConfig {
    pub const fn new(num_samples: usize) -> Self {
        Self { num_samples }
    }
}

impl Default for BetaThompsonSamplingAgentConfig {
    fn default() -> Self {
        Self::new(1)
    }
}

impl BuildIndexAgent for BetaThompsonSamplingAgentConfig {
    type Agent = BaseBetaThompsonSamplingAgent;

    fn build_index_agent(
        &self,
        num_observations: usize,
        num_actions: usize,
        reward_range: (f64, f64),
        _discount_factor: f64,
        _seed: u64,
    ) -> Result<Self::Agent, BuildAgentError> {
        Ok(BaseBetaThompsonSamplingAgent::new(
            num_observations,
            num_actions,
            reward_range,
            self.num_samples,
        ))
    }
}

/// A Thompson sampling agent for Bernoulli rewards with a Beta prior.
pub type BetaThompsonSamplingAgent<OS, AS> =
    FiniteSpaceAgent<BaseBetaThompsonSamplingAgent, OS, AS>;

/// Base Thompson sampling agent for Bernoulli rewards with a Beta prior.
///
/// Implemented only for index action and observation spaces.
#[derive(Debug, Clone, PartialEq)]
pub struct BaseBetaThompsonSamplingAgent {
    /// Reward is partitioned into high/low separated by this threshold.
    pub reward_threshold: f64,
    /// Number of posterior samples to draw.
    /// Takes the action with the highest mean sampled value.
    pub num_samples: usize,
    /// Mode of actor behaviour
    pub mode: ActorMode,

    /// Count of low and high rewards for each observation-action pair.
    low_high_reward_counts: Array2<(u64, u64)>,
}

impl BaseBetaThompsonSamplingAgent {
    pub fn new(
        num_observations: usize,
        num_actions: usize,
        reward_range: (f64, f64),
        num_samples: usize,
    ) -> Self {
        let (reward_min, reward_max) = reward_range;
        let reward_threshold = (reward_min + reward_max) / 2.0;
        let low_high_reward_counts = Array::from_elem((num_observations, num_actions), (1, 1));
        Self {
            reward_threshold,
            num_samples,
            mode: ActorMode::Training,
            low_high_reward_counts,
        }
    }
}

impl fmt::Display for BaseBetaThompsonSamplingAgent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "BaseBetaThompsonSamplingAgent({})",
            self.reward_threshold
        )
    }
}

impl BaseBetaThompsonSamplingAgent {
    /// Take a training-mode action.
    fn act_training(&self, obs_idx: usize, rng: &mut StdRng) -> usize {
        let num_samples = self.num_samples;
        self.low_high_reward_counts
            .index_axis(Axis(0), obs_idx)
            .mapv(|(beta, alpha)| -> f64 {
                // Explanation for the rng reference:
                // sample_iter takes its argument by value rather than by reference.
                // We cannot pass the rng into sample_iter because it needs to stay with self.
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
                Beta::new(alpha as f64, beta as f64)
                    .unwrap()
                    .sample_iter(&mut *rng)
                    .take(num_samples)
                    .sum()
            })
            .into_iter()
            .argmax_by(|a, b| a.partial_cmp(b).unwrap())
            .expect("Empty action space")
    }

    /// Take a release-mode (greedy) action.
    fn act_release(&self, obs_idx: usize) -> usize {
        // Take the action with highest posterior mean
        // Counts are initalized to 1 so there is no risk of 0/0
        self.low_high_reward_counts
            .index_axis(Axis(0), obs_idx)
            .mapv(|(beta, alpha)| alpha as f64 / (alpha + beta) as f64)
            .into_iter()
            .argmax_by(|a, b| a.partial_cmp(b).unwrap())
            .expect("Empty action space")
    }
}

impl PureActor<usize, usize> for BaseBetaThompsonSamplingAgent {
    type State = StdRng;

    #[inline]
    fn initial_state(&self, seed: u64) -> Self::State {
        StdRng::seed_from_u64(seed)
    }

    #[inline]
    fn act(&self, rng: &mut Self::State, observation: &usize) -> usize {
        match self.mode {
            ActorMode::Training => self.act_training(*observation, rng),
            ActorMode::Release => self.act_release(*observation),
        }
    }
}

impl SynchronousUpdate<usize, usize> for BaseBetaThompsonSamplingAgent {
    fn update(&mut self, step: TransientStep<usize, usize>, _logger: &mut dyn TimeSeriesLogger) {
        let reward_count = self
            .low_high_reward_counts
            .get_mut((step.observation, step.action))
            .unwrap();
        if step.reward > self.reward_threshold {
            reward_count.1 += 1;
        } else {
            reward_count.0 += 1;
        }
    }
}

impl SetActorMode for BaseBetaThompsonSamplingAgent {
    fn set_actor_mode(&mut self, mode: ActorMode) {
        self.mode = mode
    }
}

#[cfg(test)]
mod beta_thompson_sampling {
    use super::super::super::{testing, BuildAgent};
    use super::*;

    #[test]
    fn learns_determinstic_bandit() {
        let config = BetaThompsonSamplingAgentConfig::default();
        testing::pure_train_deterministic_bandit(
            |env| config.build_agent(env, 0).unwrap(),
            1000,
            0.9,
        );
    }
}
