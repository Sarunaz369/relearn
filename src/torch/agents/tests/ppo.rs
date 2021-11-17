//! PPO actor-critic tests
use crate::agents::{buffers::SerialBufferConfig, testing, BatchUpdateAgentConfig, BuildAgent};
use crate::torch::agents::ActorCriticConfig;
use crate::torch::critic::{BuildCritic, GaeConfig, Return};
use crate::torch::modules::{BuildModule, MlpConfig};
use crate::torch::optimizers::AdamConfig;
use crate::torch::policy::Policy;
use crate::torch::seq_modules::{GruMlpConfig, WithStateConfig};
use crate::torch::updaters::{CriticLossUpdateRule, PpoPolicyUpdateRule, WithOptimizer};
use tch::Device;

fn test_train_policy_gradient<PB, CB>(
    mut actor_config: ActorCriticConfig<
        PB,
        WithOptimizer<PpoPolicyUpdateRule, AdamConfig>,
        CB,
        WithOptimizer<CriticLossUpdateRule, AdamConfig>,
    >,
) where
    PB: BuildModule,
    PB::Module: Policy,
    CB: BuildCritic,
{
    // Speed up learning for this simple environment
    actor_config.policy_updater_config.optimizer.learning_rate = 0.1;
    actor_config.critic_updater_config.optimizer.learning_rate = 0.1;
    let config = BatchUpdateAgentConfig {
        actor_config,
        history_buffer_config: SerialBufferConfig {
            soft_threshold: 25,
            hard_threshold: 30,
        },
    };
    testing::train_deterministic_bandit(|env| config.build_agent(env, 0).unwrap(), 1_000, 0.9);
}

#[test]
fn default_mlp_return_learns_derministic_bandit() {
    test_train_policy_gradient::<MlpConfig, Return>(Default::default())
}

#[test]
fn default_mlp_return_learns_derministic_bandit_cuda_if_available() {
    let config = ActorCriticConfig {
        device: Device::cuda_if_available(),
        ..ActorCriticConfig::default()
    };
    test_train_policy_gradient::<MlpConfig, Return>(config)
}

#[test]
fn default_mlp_gae_mlp_learns_derministic_bandit() {
    test_train_policy_gradient::<MlpConfig, GaeConfig<MlpConfig>>(Default::default())
}

#[test]
fn default_gru_mlp_return_learns_derministic_bandit() {
    test_train_policy_gradient::<WithStateConfig<GruMlpConfig>, Return>(Default::default())
}

#[test]
fn default_gru_mlp_gae_mlp_derministic_bandit() {
    test_train_policy_gradient::<WithStateConfig<GruMlpConfig>, GaeConfig<MlpConfig>>(
        Default::default(),
    )
}

#[test]
fn default_gru_mlp_gae_gru_mlp_derministic_bandit() {
    test_train_policy_gradient::<WithStateConfig<GruMlpConfig>, GaeConfig<GruMlpConfig>>(
        Default::default(),
    )
}

#[test]
fn default_gru_mlp_gae_gru_mlp_derministic_bandit_cuda_if_available() {
    let config = ActorCriticConfig {
        device: Device::cuda_if_available(),
        ..ActorCriticConfig::default()
    };
    test_train_policy_gradient::<WithStateConfig<GruMlpConfig>, GaeConfig<GruMlpConfig>>(config)
}
