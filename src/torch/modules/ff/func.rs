use super::super::{FeedForwardModule, Module, ModuleExtras};
use std::iter;
use tch::{Device, Tensor};

/// Module view of a feed-forward tensor function.
#[derive(Copy, Clone)]
pub struct Func {
    f: fn(&Tensor) -> Tensor,
}

impl Func {
    #[inline]
    pub fn new(f: fn(&Tensor) -> Tensor) -> Self {
        Self { f }
    }
}

impl Module for Func {
    #[inline]
    fn shallow_clone(&self) -> Self
    where
        Self: Sized,
    {
        *self
    }

    #[inline]
    fn clone_to_device(&self, _: Device) -> Self
    where
        Self: Sized,
    {
        *self
    }

    #[inline]
    fn variables(&self) -> Box<dyn Iterator<Item = &Tensor> + '_> {
        Box::new(ModuleExtras::variables(self))
    }

    #[inline]
    fn trainable_variables(&self) -> Box<dyn Iterator<Item = &Tensor> + '_> {
        Box::new(ModuleExtras::trainable_variables(self))
    }
}

impl<'a> ModuleExtras<'a> for Func {
    type Variables = iter::Empty<&'a Tensor>;
    type TrainableVariables = Self::Variables;

    #[inline]
    fn variables(&'a self) -> Self::Variables {
        iter::empty()
    }

    #[inline]
    fn trainable_variables(&'a self) -> Self::TrainableVariables {
        iter::empty()
    }
}

impl FeedForwardModule for Func {
    #[inline]
    fn forward(&self, input: &Tensor) -> Tensor {
        (self.f)(input)
    }
}

/// Activation functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Activation {
    /// No transformation
    Identity,
    /// Rectified linear
    Relu,
    /// Sigmoid function
    Sigmoid,
    /// Hyperbolic tangent
    Tanh,
}

impl Default for Activation {
    fn default() -> Self {
        Self::Relu
    }
}

impl Activation {
    /// The function pointer for this activation function.
    #[inline]
    pub fn function(&self) -> fn(&Tensor) -> Tensor {
        use Activation::*;
        match self {
            Identity => Tensor::shallow_clone,
            Relu => Tensor::relu,
            Sigmoid => Tensor::sigmoid,
            Tanh => Tensor::tanh,
        }
    }

    /// The function pointer for this activation function if not the identity function.
    #[inline]
    pub fn maybe_function(&self) -> Option<fn(&Tensor) -> Tensor> {
        use Activation::*;
        match self {
            Identity => None,
            _ => Some(self.function()),
        }
    }

    /// Create a module for this activation function
    #[inline]
    pub fn module(&self) -> Func {
        Func::new(self.function())
    }

    /// Create a module for this activation function if not the identity function.
    #[inline]
    pub fn maybe_module(&self) -> Option<Func> {
        self.maybe_function().map(Func::new)
    }

    /// Apply this activation function to a tensor.
    #[inline]
    pub fn apply(&self, input: Tensor) -> Tensor {
        if let Some(f) = self.maybe_function() {
            f(&input)
        } else {
            input
        }
    }
}

#[cfg(test)]
mod activation {
    use super::*;
    use rstest::rstest;

    #[test]
    fn identity_maybe_module_none() {
        assert!(Activation::Identity.maybe_module().is_none());
    }

    #[rstest]
    #[case(Activation::Relu)]
    #[case(Activation::Sigmoid)]
    #[case(Activation::Tanh)]
    fn maybe_module_some(#[case] activation: Activation) {
        assert!(activation.maybe_module().is_some());
    }

    #[test]
    fn module_identity() {
        let x = Tensor::of_slice(&[-2.0, -1.0, 0.0, 1.0, 2.0]);
        let activation_fn = Activation::Identity.module();
        assert_eq!(activation_fn.forward(&x), x);
    }

    #[test]
    fn module_relu() {
        let x = Tensor::of_slice(&[-2.0, -1.0, 0.0, 1.0, 2.0]);
        let activation_fn = Activation::Relu.module();
        let expected = Tensor::of_slice(&[0.0, 0.0, 0.0, 1.0, 2.0]);
        assert_eq!(activation_fn.forward(&x), expected);
    }

    #[rstest]
    #[case(Activation::Relu, 0.0, f64::INFINITY)]
    #[case(Activation::Sigmoid, 0.0, 1.0)]
    #[case(Activation::Tanh, -1.0, 1.0)]
    fn module_bounds(
        #[case] activation: Activation,
        #[case] lower_bound: f64,
        #[case] upper_bound: f64,
    ) {
        let x = Tensor::of_slice(&[f64::NEG_INFINITY, -2.0, -1.0, 0.0, 1.0, 2.0, f64::INFINITY]);
        let y = activation.module().forward(&x);

        assert!(bool::from(y.greater_equal(lower_bound).all()));
        assert!(bool::from(y.less_equal(upper_bound).all()));
    }

    #[test]
    fn variables_count() {
        assert_eq!(
            Module::variables(&Activation::Relu.maybe_module().unwrap()).count(),
            0
        );
    }

    #[test]
    fn trainable_variables_count() {
        assert_eq!(
            Module::trainable_variables(&Activation::Relu.maybe_module().unwrap()).count(),
            0
        );
    }
}
