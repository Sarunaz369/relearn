use super::PartialStep;
use crate::utils::stats::OnlineMeanVariance;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::iter::FromIterator;

/// Summary statistics of simulation steps.
#[derive(Debug, Default, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepsSummary {
    pub step_reward: OnlineMeanVariance<f64>,
    pub episode_reward: OnlineMeanVariance<f64>,
    pub episode_length: OnlineMeanVariance<f64>,
}

impl From<OnlineStepsSummary> for StepsSummary {
    fn from(online_summary: OnlineStepsSummary) -> Self {
        online_summary.completed
    }
}

impl fmt::Display for StepsSummary {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "step_reward: ")?;
        fmt::Display::fmt(&self.step_reward, f)?;
        write!(f, "\nepisode_reward: ")?;
        fmt::Display::fmt(&self.episode_reward, f)?;
        write!(f, "\nepisode_length: ")?;
        fmt::Display::fmt(&self.episode_length, f)?;
        Ok(())
    }
}

impl StepsSummary {
    #[inline]
    pub fn num_steps(&self) -> u64 {
        self.step_reward.count()
    }

    #[inline]
    pub fn num_episodes(&self) -> u64 {
        self.episode_reward.count()
    }
}

impl<O, A> FromIterator<PartialStep<O, A>> for StepsSummary {
    fn from_iter<I>(steps: I) -> Self
    where
        I: IntoIterator<Item = PartialStep<O, A>>,
    {
        OnlineStepsSummary::from_iter(steps).into()
    }
}

/// Online calculation of simulation step statistics.
#[derive(Debug, Default, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub struct OnlineStepsSummary {
    completed: StepsSummary,
    current_episode_reward: f64,
    current_episode_length: u64,
}

impl OnlineStepsSummary {
    pub fn push<O, A>(&mut self, step: &PartialStep<O, A>) {
        self.completed.step_reward.push(step.reward);
        self.current_episode_reward += step.reward;
        self.current_episode_length += 1;
        if step.next.episode_done() {
            self.completed
                .episode_reward
                .push(self.current_episode_reward);
            self.current_episode_reward = 0.0;
            self.completed
                .episode_length
                .push(self.current_episode_length as f64);
            self.current_episode_length = 0;
        }
    }

    #[inline]
    pub fn num_steps(&self) -> u64 {
        self.completed.num_steps()
    }

    #[inline]
    pub fn num_episodes(&self) -> u64 {
        self.completed.num_episodes()
    }
}

impl<O, A> FromIterator<PartialStep<O, A>> for OnlineStepsSummary {
    fn from_iter<I>(steps: I) -> Self
    where
        I: IntoIterator<Item = PartialStep<O, A>>,
    {
        steps.into_iter().fold(Self::default(), |mut s, step| {
            s.push(&step);
            s
        })
    }
}
