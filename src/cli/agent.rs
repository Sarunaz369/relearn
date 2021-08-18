use super::{Options, Update, WithUpdate};
use crate::agents::{
    BetaThompsonSamplingAgentConfig, TabularQLearningAgentConfig, UCB1AgentConfig,
};
use crate::defs::{AgentDef, CriticDef, CriticUpdaterDef, PolicyUpdaterDef, SeqModDef};
use crate::torch::agents::ActorCriticConfig;
use clap::ArgEnum;
use std::fmt;
use std::str::FromStr;

/// Concrete agent type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ArgEnum)]
pub enum ConcreteAgentType {
    Random,
    TabularQLearning,
    BetaThompsonSampling,
    UCB1,
    PolicyGradient,
    Trpo,
    Ppo,
}

impl fmt::Display for ConcreteAgentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", Self::VARIANTS[*self as usize])
    }
}

impl ConcreteAgentType {
    pub fn agent_def(&self, opts: &Options) -> AgentDef {
        use ConcreteAgentType::*;
        match self {
            Random => AgentDef::Random,
            TabularQLearning => AgentDef::TabularQLearning(opts.into()),
            BetaThompsonSampling => AgentDef::BetaThompsonSampling(opts.into()),
            UCB1 => AgentDef::UCB1(From::from(opts)),
            PolicyGradient => {
                let config = ActorCriticConfig {
                    policy_updater_config: PolicyUpdaterDef::default_policy_gradient(),
                    ..ActorCriticConfig::default()
                }
                .with_update(opts);
                AgentDef::ActorCritic(Box::new(config))
            }
            Trpo => {
                let config = ActorCriticConfig {
                    policy_updater_config: PolicyUpdaterDef::default_trpo(),
                    ..ActorCriticConfig::default()
                }
                .with_update(opts);
                AgentDef::ActorCritic(Box::new(config))
            }
            Ppo => {
                let config = ActorCriticConfig {
                    policy_updater_config: PolicyUpdaterDef::default_ppo(),
                    ..ActorCriticConfig::default()
                }
                .with_update(opts);
                AgentDef::ActorCritic(Box::new(config))
            }
        }
    }
}

/// Wrapper agent type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ArgEnum)]
pub enum AgentWrapperType {
    ResettingMeta,
}

impl fmt::Display for AgentWrapperType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", Self::VARIANTS[*self as usize])
    }
}

impl AgentWrapperType {
    pub fn agent_def(&self, inner: AgentDef, _opts: &Options) -> AgentDef {
        use AgentWrapperType::*;
        match self {
            ResettingMeta => AgentDef::ResettingMeta(Box::new(inner)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentType {
    /// Base concrete agent
    pub base: ConcreteAgentType,
    /// Agent wrappers; applied right to left
    pub wrappers: Vec<AgentWrapperType>,
}

impl fmt::Display for AgentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for wrapper in &self.wrappers {
            write!(f, "{}:", wrapper)?;
        }
        write!(f, "{}", self.base)
    }
}

impl FromStr for AgentType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let case_insensitive = true;
        if let Some((wrapper_str, base_str)) = s.rsplit_once(':') {
            let base = ConcreteAgentType::from_str(base_str, case_insensitive)?;
            let wrappers = wrapper_str
                .split(':')
                .map(|ws| AgentWrapperType::from_str(ws, case_insensitive))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Self { base, wrappers })
        } else {
            Ok(Self {
                base: ConcreteAgentType::from_str(s, case_insensitive)?,
                wrappers: Vec::new(),
            })
        }
    }
}

impl AgentType {
    pub fn agent_def(&self, opts: &Options) -> AgentDef {
        let mut agent_def = self.base.agent_def(opts);
        for wrapper in self.wrappers.iter().rev() {
            agent_def = wrapper.agent_def(agent_def, opts);
        }
        agent_def
    }
}

impl From<&Options> for AgentDef {
    fn from(opts: &Options) -> Self {
        opts.agent.agent_def(opts)
    }
}

impl From<&Options> for TabularQLearningAgentConfig {
    fn from(opts: &Options) -> Self {
        Self::default().with_update(opts)
    }
}

impl Update<&Options> for TabularQLearningAgentConfig {
    fn update(&mut self, opts: &Options) {
        if let Some(exploration_rate) = opts.exploration_rate {
            self.exploration_rate = exploration_rate;
        }
    }
}

impl From<&Options> for BetaThompsonSamplingAgentConfig {
    fn from(opts: &Options) -> Self {
        Self::default().with_update(opts)
    }
}

impl Update<&Options> for BetaThompsonSamplingAgentConfig {
    fn update(&mut self, opts: &Options) {
        if let Some(num_samples) = opts.num_samples {
            self.num_samples = num_samples;
        }
    }
}

impl From<&Options> for UCB1AgentConfig {
    fn from(opts: &Options) -> Self {
        Self::default().with_update(opts)
    }
}

impl Update<&Options> for UCB1AgentConfig {
    fn update(&mut self, opts: &Options) {
        if let Some(exploration_rate) = opts.exploration_rate {
            self.exploration_rate = exploration_rate;
        }
    }
}

impl Update<&Options>
    for ActorCriticConfig<SeqModDef, PolicyUpdaterDef, CriticDef, CriticUpdaterDef>
{
    fn update(&mut self, opts: &Options) {
        self.policy_config.update(opts);
        self.policy_updater_config.update(opts);
        self.critic_config.update(opts);
        self.critic_updater_config.update(opts);
        if let Some(steps_per_epoch) = opts.steps_per_epoch {
            self.steps_per_epoch = steps_per_epoch;
        }
        if let Some(device) = opts.device {
            self.device = device.into();
        }
    }
}
