/// Description of an environment step
#[derive(Debug)]
pub struct Step<'a, O, A> {
    /// The initial observation.
    pub observation: O,
    /// The action taken from the initial state given the initial observation.
    pub action: A,
    /// The resulting reward.
    pub reward: f64,
    /// The resulting successor state; is None if the successor state is terminal.
    /// All trajectories from a terminal state have 0 reward on each step.
    pub next_observation: Option<&'a O>,
    /// Whether this step ends the episode.
    /// An episode is always done if it reaches a terminal state.
    /// An episode may be done for other reasons, like a step limit.
    pub episode_done: bool,
}

/// An actor that produces actions in response to a sequence of observations.
///
/// The action selection process may depend on the history of observations and actions within an
/// episode.
pub trait Actor<O, A> {
    /// Choose an action in the environment.
    ///
    /// This must be called sequentially within an episode.
    ///
    /// # Args
    /// * `observation`: The current observation of the environment state.
    /// * `new_episode`: Whether this observation is the start of a new episode.
    fn act(&mut self, observation: &O, new_episode: bool) -> A;
}

/// A learning agent.
///
/// Can interact with an environment and learns from the interaction.
pub trait Agent<O, A>: Actor<O, A> {
    /// Update the agent based on the most recent action.
    ///
    /// # Args
    /// * `step`: The environment step resulting from the  most recent call to [Actor::act].
    fn update(&mut self, _step: Step<O, A>) {} // Default implementation does nothing
}
