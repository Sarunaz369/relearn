//! Module test utilities.
use super::{BuildModule, FeedForwardModule, IterativeModule, Module, SequenceModule};
use crate::torch::optimizers::{BuildOptimizer, OnceOptimizer, SgdConfig};
use std::fmt::Debug;
use std::iter;
use tch::{self, kind::Kind, Device, IndexOp, Tensor};

/// Basic structural check of [`FeedForwardModule::forward`].
pub fn check_forward<M: FeedForwardModule>(
    module: &M,
    in_dim: usize,
    out_dim: usize,
    batch_shape: &[usize],
    kind: Kind,
) {
    let _no_grad_guard = tch::no_grad_guard();
    let input_shape: Vec<_> = batch_shape
        .iter()
        .chain(iter::once(&in_dim))
        .map(|&d| d as i64)
        .collect();
    let input = Tensor::ones(&input_shape, (kind, Device::Cpu));
    let output = module.forward(&input);
    let mut output_shape = input_shape;
    *output_shape.last_mut().unwrap() = out_dim as i64;
    assert_eq!(output.size(), output_shape);
}

/// Basic check of [`SequenceModule::seq_serial`]
///
/// * Checks that the output size is correct.
/// * Checks that identical inner sequences produce identical output.
pub fn check_seq_serial<M: SequenceModule>(module: &M, in_dim: usize, out_dim: usize) {
    let _no_grad_guard = tch::no_grad_guard();
    let batch_size: usize = 4;

    // Step indices by sequence: 0 | 1 2 3 | 4 5
    let seq_lengths: [usize; 3] = [1, 3, 2];
    let total_num_steps: usize = seq_lengths.iter().sum();

    let inputs = Tensor::ones(
        &[batch_size as i64, total_num_steps as i64, in_dim as i64],
        (Kind::Float, Device::Cpu),
    );

    let output = module.seq_serial(&inputs, &seq_lengths);

    // Check shape
    assert_eq!(
        output.size(),
        vec![batch_size as i64, total_num_steps as i64, out_dim as i64]
    );

    // Compare the inner sequences. The output should be the same for each.
    assert_allclose(&output.i((.., 0, ..)), &output.i((.., 1, ..)));
    assert_allclose(&output.i((.., 1..3, ..)), &output.i((.., 4..6, ..)));
}

fn assert_allclose(input: &Tensor, other: &Tensor) {
    assert!(input.allclose(other, 1e-5, 1e-8, false))
}

/// Basic check of [`SequenceModule::seq_packed`]
///
/// * Checks that the output size is correct.
/// * Checks that identical inner sequences produce identical output.
pub fn check_seq_packed<M: SequenceModule>(module: &M, in_dim: usize, out_dim: usize) {
    let _no_grad_guard = tch::no_grad_guard();
    // Input consists of 3 sequences: [0.1, 0.2, 0.3, 0.4], [0.1, 0.2, 0.3], and [0.1].
    let data = [0.1_f32, 0.1, 0.1, 0.2, 0.2, 0.3, 0.3, 0.4];
    let inputs = Tensor::of_slice(&data)
        .unsqueeze(-1)
        .expand(&[-1, in_dim as i64], false);
    let batch_sizes = Tensor::of_slice(&[3_i64, 2, 2, 1]);

    let output = module.seq_packed(&inputs, &batch_sizes);

    // Check shape
    assert_eq!(output.size(), vec![data.len() as i64, out_dim as i64],);

    // Compare the packed sequences.
    // The output should be the same for each since they have the same values.
    let seq_1_indices: &[i64] = &[0, 3, 5, 7];
    let seq_2_indices: &[i64] = &[1, 4, 6];
    let seq_3_indices: &[i64] = &[2];

    assert_allclose(
        &output.i((&seq_1_indices[..3], ..)),
        &output.i((seq_2_indices, ..)),
    );
    assert_allclose(
        &output.i((&seq_1_indices[..1], ..)),
        &output.i((seq_3_indices, ..)),
    );
}

/// Basic structural check of [`IterativeModule::step`]
///
/// * Checks that the output size is correct.
/// * Checks that the output state of a step can be used in the next step.
pub fn check_step<M: IterativeModule>(module: &M, in_dim: usize, out_dim: usize) {
    let _no_grad_guard = tch::no_grad_guard();

    let mut state = module.initial_state();
    let input1 = Tensor::ones(&[in_dim as i64], (Kind::Float, Device::Cpu));
    let output1 = module.step(&mut state, &input1);
    assert_eq!(output1.size(), vec![out_dim as i64]);

    // Make sure the output state can be used as a new input state
    let input2 = -input1;
    let output2 = module.step(&mut state, &input2);
    assert_eq!(output2.size(), vec![out_dim as i64]);
}

/// Check that [`SequenceModule::seq_packed`] output matches [`IterativeModule::step`].
pub fn check_seq_packed_matches_iter_steps<M>(module: &M, in_dim: usize, out_dim: usize)
where
    M: SequenceModule + IterativeModule,
{
    let _no_grad_guard = tch::no_grad_guard();

    let seq_len = 5;
    let num_seqs = 2;
    let input = Tensor::rand(
        &[seq_len, num_seqs, in_dim as i64],
        (Kind::Float, Device::Cpu),
    );

    let packed_input = input.reshape(&[seq_len * num_seqs, in_dim as i64]);
    let batch_sizes = Tensor::full(&[seq_len], num_seqs, (Kind::Int64, Device::Cpu));
    let packed_output = module.seq_packed(&packed_input, &batch_sizes);
    let output = packed_output.reshape(&[seq_len, num_seqs, out_dim as i64]);

    for i in 0..num_seqs {
        let mut state = module.initial_state();
        for j in 0..seq_len {
            let step_output = module.step(&mut state, &input.i((j, i, ..)));
            let expected = output.i((j, i, ..));
            assert!(
                step_output.allclose(&expected, 1e-6, 1e-6, false),
                "seq {i}, step {j}; {step_output:?} != {:?}",
                expected
            );
        }
    }
}

/// Check that gradient descent improves the output of a forward model.
pub fn check_config_forward_gradient_descent<MC>(config: &MC)
where
    MC: BuildModule,
    MC::Module: FeedForwardModule,
{
    let in_dim = 2;
    let out_dim = 32; // needs to be large enough to avoid all 0 from ReLU by chance
    let kind = Kind::Float;
    let device = Device::Cpu;

    // Input batch consists of a row of zeros and a row of ones.
    let input = Tensor::stack(
        &[
            Tensor::zeros(&[in_dim as i64], (kind, device)),
            Tensor::ones(&[in_dim as i64], (kind, device)),
        ],
        0,
    );
    // Target is the identity function (except dimension size)
    let target = Tensor::stack(
        &[
            Tensor::zeros(&[out_dim as i64], (kind, device)),
            Tensor::ones(&[out_dim as i64], (kind, device)),
        ],
        0,
    );

    let model = config.build_module(in_dim, out_dim, device);
    let mut optimizer = SgdConfig::default()
        .build_optimizer(model.trainable_variables())
        .unwrap();

    let initial_output = model.forward(&input);

    let initial_loss = (&initial_output - &target).square().sum(kind);
    optimizer
        .backward_step_once(&initial_loss, &mut ())
        .unwrap();

    let final_output = model.forward(&input);
    assert_ne!(initial_output, final_output);

    let final_loss = (&final_output - &target).square().sum(kind);
    let initial_loss_value: f32 = initial_loss.into();
    let final_loss_value: f32 = final_loss.into();
    assert!(final_loss_value < initial_loss_value);
}

/// Check that gradient descent improves the output of a sequence model using `seq_packed`.
pub fn check_config_seq_packed_gradient_descent<MC>(config: &MC)
where
    MC: BuildModule,
    MC::Module: SequenceModule,
{
    let in_dim: usize = 2;
    let seq_dim = 8;
    let batch_size = 2;
    // out_dim * seq_dim * batch_size needs to be large enough to avoid all 0s by chance from relu
    let out_dim: usize = 8;
    let kind = Kind::Float;
    let device = Device::Cpu;

    let input = Tensor::rand(&[seq_dim * batch_size, in_dim as i64], (kind, device));
    let batch_sizes = Tensor::full(&[seq_dim], batch_size, (Kind::Int64, Device::Cpu));
    let target = Tensor::rand(&[seq_dim * batch_size, out_dim as i64], (kind, device));

    let model = config.build_module(in_dim, out_dim, device);
    let mut optimizer = SgdConfig::default()
        .build_optimizer(model.trainable_variables())
        .unwrap();

    let initial_output = model.seq_packed(&input, &batch_sizes);

    let initial_loss = (&initial_output - &target).square().sum(kind);
    optimizer
        .backward_step_once(&initial_loss, &mut ())
        .unwrap();

    let final_output = model.seq_packed(&input, &batch_sizes);
    assert_ne!(initial_output, final_output);

    let final_loss = (&final_output - &target).square().sum(kind);
    let initial_loss_value: f32 = initial_loss.into();
    let final_loss_value: f32 = final_loss.into();
    assert!(final_loss_value < initial_loss_value);
}

/// Basic check of cloning a `FeedForwardModule` to a new device.
///
/// Constructs a model on `Cuda` if available and clones to `Cpu`.
/// Ends immediately if `Cuda` is not available.
/// Checks that `forward` works on the new module and on the original module before and after
/// cloning.
pub fn check_config_forward_clone_to_new_device<MC>(config: &MC)
where
    MC: BuildModule,
    MC::Module: FeedForwardModule,
{
    let in_dim = 2;
    let out_dim = 3;
    let kind = Kind::Float;
    let initial_device = Device::cuda_if_available();
    let target_device = Device::Cpu;

    if initial_device == target_device {
        return;
    }

    let original_input = Tensor::ones(&[in_dim as i64], (kind, initial_device));
    let new_input = Tensor::ones(&[in_dim as i64], (kind, target_device));

    let original_module = config.build_module(in_dim, out_dim, initial_device);

    // Check that forward works without crashing
    let _ = original_module.forward(&original_input);

    // Clone to target device
    let new_module = original_module.clone_to_device(target_device);

    // Check that forward still works on the original module
    let original_output = original_module.forward(&original_input);

    // Check that forward works on the new module with the target device
    let new_output = new_module.forward(&new_input);

    // Check that the ouputs are equal
    assert_allclose(&original_output.to_device(target_device), &new_output);
}

/// Basic check of cloning a `SequenceModule` to a new device.
///
/// Constructs a model on `Cuda` if available and clones to `Cpu`.
/// Ends immediately if `Cuda` is not available.
/// Checks that `seq_packed` works on the new module and on the original module before and after
/// cloning.
pub fn check_config_seq_packed_clone_to_new_device<MC>(config: &MC)
where
    MC: BuildModule,
    MC::Module: SequenceModule,
{
    let in_dim = 2;
    let out_dim = 3;
    // A sequence of length 3 and one of length 1
    let batch_sizes_array = [2, 1, 1_i64];
    let batch_sizes = Tensor::of_slice(&batch_sizes_array); // Must always be on CPU
    let total_num_steps: i64 = batch_sizes_array.iter().sum();

    let kind = Kind::Float;
    let initial_device = Device::cuda_if_available();
    let target_device = Device::Cpu;

    if initial_device == target_device {
        return;
    }

    let original_input = Tensor::ones(&[total_num_steps, in_dim as i64], (kind, initial_device));
    let new_input = Tensor::ones(&[total_num_steps, in_dim as i64], (kind, target_device));

    let original_module = config.build_module(in_dim, out_dim, initial_device);

    // Check that forward works without crashing
    let _ = original_module.seq_packed(&original_input, &batch_sizes);

    // Clone to target device
    let new_module = original_module.clone_to_device(target_device);

    // Check that forward still works on the original module
    let original_output = original_module.seq_packed(&original_input, &batch_sizes);

    // Check that forward works on the new module with the target device
    let new_output = new_module.seq_packed(&new_input, &batch_sizes);

    // Check that the ouputs are equal
    assert_allclose(&original_output.to_device(target_device), &new_output);
}

/// Basic check of cloning a `FeedForwardModule` to the same device.
///
/// Constructs a module on `Cpu` and clones to `Cpu`.
/// Checks that `forward` works on the new module and on the original module before and after
/// cloning.
/// Checks that the weights are shared.
pub fn check_config_forward_clone_to_same_device<MC>(config: &MC)
where
    MC: BuildModule,
    MC::Module: FeedForwardModule + PartialEq + Debug,
{
    let in_dim = 2;
    let out_dim = 3;
    let kind = Kind::Float;
    let device = Device::Cpu;

    let input = Tensor::ones(&[in_dim as i64], (kind, device));

    let original_module = config.build_module(in_dim, out_dim, device);

    // Check that forward works without crashing
    let _ = original_module.forward(&input);

    // Clone to target device
    let new_module = original_module.clone_to_device(device);

    // Check that forward still works on the original module
    let original_output = original_module.forward(&input);

    // Check that forward works on the new module with the target device
    let new_output = new_module.forward(&input);

    // Check that the ouputs are equal
    assert_eq!(original_output, new_output);

    // Modify the variables of the original module and check that the modules are still equal.
    {
        let _no_grad_guard = tch::no_grad_guard();
        for tensor in original_module.variables() {
            let _ = tensor.shallow_clone().fill_(1);
        }
    }
    assert_eq!(original_module, new_module);
}

/// Basic check of cloning a `SequenceModule` to the same device.
///
/// Constructs a model on `Cpu` and clones to `Cpu`.
/// Ends immediately if `Cuda` is not available.
/// Checks that `seq_packed` works on the new module and on the original module before and after
/// cloning.
pub fn check_config_seq_packed_clone_to_same_device<MC>(config: &MC)
where
    MC: BuildModule,
    MC::Module: SequenceModule + PartialEq + Debug,
{
    let in_dim = 2;
    let out_dim = 3;
    // A sequence of length 3 and one of length 1
    let batch_sizes_array = [2, 1, 1_i64];
    let batch_sizes = Tensor::of_slice(&batch_sizes_array); // Must always be on CPU
    let total_num_steps: i64 = batch_sizes_array.iter().sum();

    let kind = Kind::Float;
    let device = Device::Cpu;

    let input = Tensor::ones(&[total_num_steps, in_dim as i64], (kind, device));

    let original_module = config.build_module(in_dim, out_dim, device);

    // Check that forward works without crashing
    let _ = original_module.seq_packed(&input, &batch_sizes);

    // Clone to target device
    let new_module = original_module.clone_to_device(device);

    // Check that forward still works on the original module
    let original_output = original_module.seq_packed(&input, &batch_sizes);

    // Check that forward works on the new module with the target device
    let new_output = new_module.seq_packed(&input, &batch_sizes);

    // Check that the ouputs are equal
    assert_allclose(&original_output.to_device(device), &new_output);

    // Modify the variables of the original module and check that the modules are still equal.
    {
        let _no_grad_guard = tch::no_grad_guard();
        for tensor in original_module.variables() {
            let _ = tensor.shallow_clone().fill_(1);
        }
    }
    assert_eq!(original_module, new_module);
}
