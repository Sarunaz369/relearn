use super::{Critic, CriticUpdateRule, HistoryFeatures, RuleOpt, RuleOptConfig, StatsLogger};
use crate::torch::optimizers::{opt_expect_ok_log, AdamConfig, Optimizer};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tch::COptimizer;

/// Critic update rule that performs multiple steps of gradient-based loss minimization.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GradOptRule {
    /// Number of optimizer iterations per update
    pub optimizer_iters: u64,
}

/// A [`LearningCritic`][1] using gradient-based loss minimization ([`GradOptRule`]).
///
/// [1]: super::LearningCritic
pub type GradOpt<C, O = COptimizer> = RuleOpt<C, O, GradOptRule>;
/// Configuration for [`GradOpt`], a gradient-based loss-minimizing [`LearningCritic`][1].
///
/// [1]: super::LearningCritic
pub type GradOptConfig<CB, OB = AdamConfig> = RuleOptConfig<CB, OB, GradOptRule>;

impl Default for GradOptRule {
    #[inline]
    fn default() -> Self {
        Self {
            optimizer_iters: 80,
        }
    }
}

impl<C, O> CriticUpdateRule<C, O> for GradOptRule
where
    C: Critic,
    O: Optimizer,
{
    fn update_external_critic(
        &mut self,
        critic: &C,
        optimizer: &mut O,
        features: &dyn HistoryFeatures,
        logger: &mut dyn StatsLogger,
    ) {
        let mut loss_fn = || {
            critic
                .loss(features)
                .expect("critic has no trainable parameters")
        };

        let mut critic_opt_start = Instant::now();
        for i in 0..self.optimizer_iters {
            let result = optimizer.backward_step(&mut loss_fn, logger);
            let opt_loss = opt_expect_ok_log(result, "policy step error").map(f64::from);
            let critic_opt_end = Instant::now();

            let mut step_logger = logger.with_scope("step").group();
            if let Some(loss) = opt_loss {
                step_logger.log_scalar("loss", loss);
            }
            step_logger.log_counter_increment("count", 1);
            step_logger.log_duration("time", critic_opt_end - critic_opt_start);
            drop(step_logger);

            critic_opt_start = critic_opt_end;

            if let Some(loss) = opt_loss {
                if i == 0 {
                    logger.log_scalar("loss_initial", loss);
                } else if i == self.optimizer_iters - 1 {
                    logger.log_scalar("loss_final", loss);
                }
            }
        }
    }
}
