//! Critic (baseline) modules
mod gae;
mod r#return;

pub use gae::{Gae, GaeConfig};
pub use r#return::Return;

use super::features::HistoryFeatures;
use crate::torch::modules::Module;
use crate::torch::packed::PackedTensor;
use tch::{Device, Tensor};

/// Critic for a reinforcement learning environment.
///
/// Assigns a value to each state-action pair in a trajectory.
/// The value may depend on past states and actions in the trajectory and on future rewards.
/// Higher values represent better outcomes than lower values.
/// These are *not* necessarily Q values.
///
/// # Terminology
/// ## Critic
/// This use of "Critic" is possibly more expansive than the typical use:
/// it does not just refer to a runtime evaluator of expected future reward given the observed
/// trajectory so far, but also includes a retrospective evaluation of states and actions given
/// the empirical future trajectory.
///
/// ## Value Function
/// Some critics use value functions to improve their value estimates.
/// A value function is a function approximator that maps a sequence of episode history features to
/// estimates of the expected future return of each observation or observation-action pair.
/// May only use the past history within an episode, not from the future or across episodes.
pub trait Critic: Module {
    /// Provide values for a packed sequence of steps.
    ///
    /// # Args
    /// * `features` - A view of the packed step history features.
    ///
    /// # Returns
    /// Packed step values. A 1D f32 tensor with the same shape as `rewards` and `returns`.
    /// There is no meaning to the returned values apart from higher values representing better
    /// outcomes (in estimate).
    fn step_values(&self, features: &dyn HistoryFeatures) -> PackedTensor;

    /// The loss of any trainable internal variables given the observed history features.
    ///
    /// Returns `None` if this critic has no trainable variables.
    fn loss(&self, features: &dyn HistoryFeatures) -> Option<Tensor>;
}

/// Build a [`Critic`] instance.
pub trait BuildCritic {
    type Critic: Critic;

    /// Build a new [`Critic`] instance.
    ///
    /// # Args
    /// * `in_dim` - Number of input feature dimensions.
    /// * `device`  - Device on which to store the critic variables.
    fn build_critic(&self, in_dim: usize, device: Device) -> Self::Critic;
}
