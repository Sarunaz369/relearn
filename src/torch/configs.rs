//! Module configurations / builders.
use super::{Activation, ModuleBuilder};
use std::borrow::Borrow;
use std::iter;
use tch::nn;

/// Multi-Layer Perceptron Configuration
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MLPConfig {
    /// Sizes of the hidden layers
    hidden_sizes: Vec<usize>,
    /// Activation function between hidden layers.
    activation: Activation,
    /// Activation function on the output.
    output_activation: Activation,
}

impl Default for MLPConfig {
    fn default() -> Self {
        MLPConfig {
            hidden_sizes: vec![100],
            activation: Activation::Relu,
            output_activation: Activation::Identity,
        }
    }
}

impl ModuleBuilder for MLPConfig {
    type Module = nn::Sequential;

    fn build<'a, T: Borrow<nn::Path<'a>>>(
        &self,
        vs: T,
        input_dim: usize,
        output_dim: usize,
    ) -> Self::Module {
        let vs = vs.borrow();

        let iter_in_dim = iter::once(&input_dim).chain(self.hidden_sizes.iter());
        let iter_out_dim = self.hidden_sizes.iter().chain(iter::once(&output_dim));

        let mut layers = nn::seq();
        for (i, (&layer_in_dim, &layer_out_dim)) in iter_in_dim.zip(iter_out_dim).enumerate() {
            if i > 0 {
                if let Some(m) = self.activation.maybe_module() {
                    layers = layers.add(m);
                }
            }
            layers = layers.add(nn::linear(
                vs / format!("layer_{}", i),
                layer_in_dim as i64,
                layer_out_dim as i64,
                Default::default(),
            ));
        }

        if let Some(m) = self.output_activation.maybe_module() {
            layers = layers.add(m);
        }
        layers
    }
}