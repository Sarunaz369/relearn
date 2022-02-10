//! Recurrent neural networks
mod gru;
mod lstm;

pub use gru::{Gru, GruConfig};
pub use lstm::{Lstm, LstmConfig};

use super::super::{BuildModule, IterativeModule, Module};
use smallvec::SmallVec;
use std::marker::PhantomData;
use tch::{nn::Path, Cuda, Device, Tensor};

/// Basic recurrent neural network configuration
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct RnnBaseConfig<T> {
    /// Number of layers; each has size equal to the output size when built.
    pub num_layers: usize,
    /// Whether the layers include bias terms
    pub has_biases: bool,
    /// Phantom marker for the specific RNN type (`RNN`, `GRU`, `LSTM`, etc)
    type_: PhantomData<fn() -> T>,
}

impl<T> Default for RnnBaseConfig<T> {
    fn default() -> Self {
        Self {
            num_layers: 1,
            has_biases: true,
            type_: PhantomData,
        }
    }
}

impl<T: RnnImpl> BuildModule for RnnBaseConfig<T> {
    type Module = RnnBase<T>;

    fn build_module(&self, vs: &Path, in_dim: usize, out_dim: usize) -> Self::Module {
        RnnBase::new(vs, in_dim, out_dim, self)
    }
}

pub trait RnnImpl {
    /// State for one cell (single iterative layer)
    type CellState;

    fn type_() -> RnnType;

    fn initial_cell_state(rnn: &RnnBase<Self>, batch_size: i64) -> Self::CellState
    where
        Self: Sized;

    fn cell_batch_step(
        rnn: &RnnBase<Self>,
        state: &mut Self::CellState,
        weights: &RnnLayerWeights,
        batch_input: &Tensor,
    ) -> Tensor
    where
        Self: Sized;
}

#[derive(Debug)]
pub struct RnnBase<T> {
    weights: RnnWeights,
    hidden_size: i64,
    dropout: f64,
    device: Device,
    type_: PhantomData<fn() -> T>,
}

impl<T: RnnImpl> RnnBase<T> {
    pub fn new(vs: &Path, in_dim: usize, out_dim: usize, config: &RnnBaseConfig<T>) -> Self {
        Self {
            weights: RnnWeights::init(
                vs,
                T::type_(),
                in_dim,
                out_dim,
                config.num_layers,
                config.has_biases,
            ),
            hidden_size: out_dim.try_into().unwrap(),
            dropout: 0.0,
            device: vs.device(),
            type_: PhantomData,
        }
    }
}

impl<T> Module for RnnBase<T> {
    fn has_cudnn_second_derivatives(&self) -> bool {
        false
    }
}

impl<T: RnnImpl> IterativeModule for RnnBase<T> {
    // Hold up to 4 layers without allocationg
    type State = SmallVec<[T::CellState; 4]>;

    fn initial_state(&self) -> Self::State {
        let batch_size = 1;
        (0..self.weights.num_layers())
            .map(|_| T::initial_cell_state(self, batch_size))
            .collect()
    }

    fn step(&self, state: &mut Self::State, input: &Tensor) -> Tensor {
        let mut hidden = input.unsqueeze(0);
        for (layer_weights, layer_state) in self.weights.layers().zip(state) {
            hidden = T::cell_batch_step(self, layer_state, &layer_weights, &hidden);
        }
        hidden.squeeze_dim(0)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum RnnType {
    /// Recurrent neural unit with relu activation
    RnnRelu,
    /// Recurrent neural unit with tanh activation
    RnnTanh,
    /// Long Short Term Memory
    Lstm,
    /// Gated Recurrent Unit
    Gru,
}

impl RnnType {
    /// cuDNN RNN mode code
    ///
    /// See <https://github.com/pytorch/pytorch/blob/d6909732954ad182d13fa8ab9959502a386e9d3a/torch/csrc/api/src/nn/modules/rnn.cpp#L29>
    const fn cudnn_mode(&self) -> usize {
        match self {
            Self::RnnRelu => 0,
            Self::RnnTanh => 1,
            Self::Lstm => 2,
            Self::Gru => 3,
        }
    }
}

#[derive(Debug)]
struct RnnWeights {
    flat_weights: Vec<Tensor>,
    has_biases: bool,
}

impl RnnWeights {
    /// All weights an array
    pub fn flat_weights(&self) -> &[Tensor] {
        &self.flat_weights
    }

    pub fn num_layers(&self) -> usize {
        assert_eq!(self.flat_weights.len() % self.weights_per_layer(), 0);
        self.flat_weights.len() / self.weights_per_layer()
    }

    pub const fn weights_per_layer(&self) -> usize {
        if self.has_biases {
            4
        } else {
            2
        }
    }

    pub fn layers(&self) -> impl Iterator<Item = RnnLayerWeights<'_>> {
        self.flat_weights
            .chunks_exact(self.weights_per_layer())
            .map(|weights| RnnLayerWeights {
                weights,
                has_biases: self.has_biases,
            })
    }

    pub fn init(
        vs: &Path,
        type_: RnnType,
        in_dim: usize,
        out_dim: usize,
        num_layers: usize,
        with_bias: bool,
    ) -> Self {
        let in_dim: i64 = in_dim.try_into().unwrap();
        let hidden_size: i64 = out_dim.try_into().unwrap();
        let gates_size = match type_ {
            RnnType::RnnRelu => hidden_size,
            RnnType::RnnTanh => hidden_size,
            RnnType::Lstm => 4 * hidden_size,
            RnnType::Gru => 3 * hidden_size,
        };

        let mut flat_weights = Vec::new();
        for i in 0..num_layers {
            let layer_input_size = if i == 0 { in_dim } else { hidden_size };
            // TODO: Use the same initialization as PyTorch
            // <https://pytorch.org/docs/stable/_modules/torch/nn/modules/rnn.html>
            flat_weights.push(vs.kaiming_uniform(
                &format!("weight_ih_l{}", i),
                &[gates_size, layer_input_size],
            ));
            flat_weights
                .push(vs.kaiming_uniform(&format!("weight_hh_l{}", i), &[gates_size, hidden_size]));

            if with_bias {
                flat_weights.push(vs.zeros(&format!("bias_ih_l{}", i), &[gates_size]));
                flat_weights.push(vs.zeros(&format!("bias_hh_l{}", i), &[gates_size]));
            }
        }

        if vs.device().is_cuda() && Cuda::cudnn_is_available() {
            // Flatten the weights in-place
            // See https://github.com/pytorch/pytorch/blob/master/torch/csrc/api/src/nn/modules/rnn.cpp#L161
            let _no_grad = tch::no_grad_guard();
            let _ = Tensor::internal_cudnn_rnn_flatten_weight(
                &flat_weights,
                if with_bias { 4 } else { 2 },
                in_dim,
                type_.cudnn_mode() as i64,
                hidden_size,
                0,                 // No projections
                num_layers as i64, // Num layers
                true,              // Batch first
                false,             // Not bidirectional
            );
        }
        Self {
            flat_weights,
            has_biases: with_bias,
        }
    }
}

pub struct RnnLayerWeights<'a> {
    weights: &'a [Tensor],
    has_biases: bool,
}

impl<'a> RnnLayerWeights<'a> {
    pub const fn w_ih(&self) -> &Tensor {
        &self.weights[0]
    }

    pub const fn w_hh(&self) -> &Tensor {
        &self.weights[1]
    }

    pub const fn b_ih(&self) -> Option<&Tensor> {
        if self.has_biases {
            Some(&self.weights[2])
        } else {
            None
        }
    }

    pub const fn b_hh(&self) -> Option<&Tensor> {
        if self.has_biases {
            Some(&self.weights[3])
        } else {
            None
        }
    }
}