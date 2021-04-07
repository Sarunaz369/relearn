use crate::torch::optimizers::{AdamConfig, AdamWConfig, RmsPropConfig, SgdConfig};
use std::convert::{TryFrom, TryInto};
use tch::{COptimizer, TchError};

/// Torch optimizer definition
#[derive(Debug, Clone)]
pub enum OptimizerDef {
    Sgd(SgdConfig),
    RmsProp(RmsPropConfig),
    Adam(AdamConfig),
    AdamW(AdamWConfig),
}

impl Default for OptimizerDef {
    fn default() -> Self {
        OptimizerDef::Adam(Default::default())
    }
}

impl TryFrom<&OptimizerDef> for COptimizer {
    type Error = TchError;

    fn try_from(def: &OptimizerDef) -> Result<Self, Self::Error> {
        use OptimizerDef::*;
        match def {
            Sgd(config) => config.try_into(),
            RmsProp(config) => config.try_into(),
            Adam(config) => config.try_into(),
            AdamW(config) => config.try_into(),
        }
    }
}

impl OptimizerDef {
    /// Set the learning rate
    pub fn set_learning_rate(&mut self, learning_rate: f64) {
        use OptimizerDef::*;
        match self {
            Sgd(ref mut c) => {
                c.learning_rate = learning_rate;
            }
            RmsProp(ref mut c) => {
                c.learning_rate = learning_rate;
            }
            Adam(ref mut c) => {
                c.learning_rate = learning_rate;
            }
            AdamW(ref mut c) => {
                c.learning_rate = learning_rate;
            }
        }
    }

    /// Set the learning rate
    pub fn with_learning_rate(mut self, learning_rate: f64) -> Self {
        self.set_learning_rate(learning_rate);
        self
    }
}

#[cfg(test)]
mod optimizer_def {
    use super::*;
    use crate::torch::OptimizerBuilder;
    use tch::{nn::VarStore, Device};

    #[test]
    fn build_default() {
        let opt_def = OptimizerDef::default();
        let vs = VarStore::new(Device::Cpu);
        let _ = opt_def.build(&vs);
    }
}
