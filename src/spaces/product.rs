//! Tuple space (Cartesian product)
#![allow(clippy::type_complexity)] // Complexity generated by the impl_for_tuples macro
use super::{
    BaseFeatureSpace, BatchFeatureSpace, BatchFeatureSpaceOut, ElementRefInto, FeatureSpace,
    FeatureSpaceOut, FiniteSpace, SampleSpace, Space,
};
use crate::logging::Loggable;
use crate::utils::array::BasicArray;
use impl_trait_for_tuples::impl_for_tuples;
use rand::distributions::Distribution;
use rand::Rng;
use std::array::IntoIter;
use std::convert::TryInto;
use std::fmt;
use std::marker::PhantomData;
use tch::Tensor;

/// A Cartesian product of spaces.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProductSpace<T> {
    pub inner_spaces: T,
}

impl<T> ProductSpace<T> {
    /// Initialize from a tuple of spaces.
    pub const fn new(inner_spaces: T) -> Self {
        Self { inner_spaces }
    }
}

impl<T: DisplayForTuple> fmt::Display for ProductSpace<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.inner_spaces.fmt(f)
    }
}

impl<T: SpaceForTuples> Space for ProductSpace<T> {
    type Element = <T as SpaceForTuples>::Element;

    fn contains(&self, value: &Self::Element) -> bool {
        self.inner_spaces.contains(value)
    }
}

impl<T: FiniteSpaceForTuples> FiniteSpace for ProductSpace<T> {
    fn size(&self) -> usize {
        self.inner_spaces.size()
    }

    fn to_index(&self, element: &Self::Element) -> usize {
        self.inner_spaces.to_index(element)
    }

    fn from_index(&self, index: usize) -> Option<Self::Element> {
        self.inner_spaces.from_index(index)
    }

    fn from_index_unchecked(&self, index: usize) -> Option<Self::Element> {
        self.inner_spaces.from_index_unchecked(index)
    }
}

impl<T: BaseFeatureSpaceForTuples> BaseFeatureSpace for ProductSpace<T> {
    fn num_features(&self) -> usize {
        self.inner_spaces.num_features()
    }
}

impl<T, U> FeatureSpace<U> for ProductSpace<T>
where
    T: FeatureSpaceOutForTuples<U>,
    U: BasicArray<f32, 1>,
{
    fn features(&self, element: &Self::Element) -> U {
        let (mut out, zeroed) = U::allocate([self.num_features()]);
        self.features_out(element, &mut out, zeroed);
        out
    }
}

impl<T, U> FeatureSpaceOut<U> for ProductSpace<T>
where
    T: FeatureSpaceOutForTuples<U>,
{
    fn features_out(&self, element: &Self::Element, out: &mut U, zeroed: bool) {
        self.inner_spaces.features_out(element, out, zeroed);
    }
}

impl<T, U> BatchFeatureSpace<U> for ProductSpace<T>
where
    T: BatchFeatureSpaceOutForTuples<U>,
    U: BasicArray<f32, 2>,
{
    fn batch_features<'a, I>(&self, elements: I) -> U
    where
        I: IntoIterator<Item = &'a Self::Element>,
        <I as IntoIterator>::IntoIter: ExactSizeIterator,
        Self::Element: 'a,
    {
        let elements = elements.into_iter();
        let (mut out, zeroed) = U::allocate([elements.len(), self.num_features()]);
        self.batch_features_out(elements, &mut out, zeroed);
        out
    }
}

impl<T, U> BatchFeatureSpaceOut<U> for ProductSpace<T>
where
    T: BatchFeatureSpaceOutForTuples<U>,
{
    fn batch_features_out<'a, I>(&self, elements: I, out: &mut U, zeroed: bool)
    where
        I: IntoIterator<Item = &'a Self::Element>,
        Self::Element: 'a,
    {
        self.inner_spaces
            .batch_features_out(elements, out, zeroed, PhantomData);
    }
}

impl<T> Distribution<<Self as Space>::Element> for ProductSpace<T>
where
    T: SpaceForTuples + SampleSpaceForTuples,
{
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> <Self as Space>::Element {
        self.inner_spaces.sample(rng)
    }
}

impl<T: SpaceForTuples> ElementRefInto<Loggable> for ProductSpace<T> {
    fn elem_ref_into(&self, _element: &Self::Element) -> Loggable {
        Loggable::Nothing
    }
}

// The impl_trait_for_tuples crate helps with implementing traits for tuples.
// I use custom versions of the space traits so that `ProductSpace`
// will only be generic over the tuples defined here.
// Otherwise,
//  * ProductSpace would be generic over any space, not just tuples
//  * Tuples would be interpred as spaces.
//      Could cause confusion given that not typical traits (Distribution) are implemented.
//      Could also cause a tuple of spaces to be accidentally interpreted as a product space.
//
// These have to be listed as public because they are part of the interface of ProductSpace
// but they are not intended to be implemented by user types.

/// Private.
pub trait DisplayForTuple {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result;
}

/// Private. Use [`Space`] instead.
pub trait SpaceForTuples {
    type Element;
    fn contains(&self, value: &Self::Element) -> bool;
}

/// Private. Use [`FiniteSpace`] instead.
pub trait FiniteSpaceForTuples: SpaceForTuples {
    fn size(&self) -> usize;
    fn to_index(&self, element: &Self::Element) -> usize;
    fn from_index(&self, index: usize) -> Option<Self::Element>;
    fn from_index_unchecked(&self, index: usize) -> Option<Self::Element>;
}

/// Private. Use [`BaseFeatureSpace`] instead.
pub trait BaseFeatureSpaceForTuples {
    fn num_features(&self) -> usize;
}

/// Private. Use [`FeatureSpaceOut`] instead.
pub trait FeatureSpaceOutForTuples<T>: SpaceForTuples + BaseFeatureSpaceForTuples {
    fn features_out(&self, element: &Self::Element, out: &mut T, zeroed: bool);
}

/// Private. Use [`BatchFeatureSpaceOut`] instead.
pub trait BatchFeatureSpaceOutForTuples<T>: SpaceForTuples + BaseFeatureSpaceForTuples {
    /// Write a batch of features into an array.
    ///
    /// # Hack
    /// The `PhantomData` argument is a work-around for an issue where the compiler does not accept
    /// the correct lifetime bounds. By including `PhantomData` the bound is inferred instead.
    ///
    /// * <https://users.rust-lang.org/t/lifetime/59967>
    /// * <https://github.com/rust-lang/rust/issues/85451>
    fn batch_features_out<'a, I>(
        &self,
        elements: I,
        out: &mut T,
        zeroed: bool,
        marker: PhantomData<&'a Self::Element>,
    ) where
        I: IntoIterator<Item = &'a Self::Element>,
        Self::Element: 'a;
}

/// Private. Use [`Distribution`] instead.
pub trait SampleSpaceForTuples: SpaceForTuples {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> <Self as SpaceForTuples>::Element;
}

impl SpaceForTuples for () {
    type Element = ();

    fn contains(&self, _value: &Self::Element) -> bool {
        true
    }
}

#[impl_for_tuples(1, 12)]
#[tuple_types_custom_trait_bound(Space)]
impl SpaceForTuples for Tuple {
    for_tuples!( type Element = ( #( Tuple::Element ),* ); );

    fn contains(&self, value: &Self::Element) -> bool {
        for_tuples!( #( self.Tuple.contains(&value.Tuple) )&* )
    }
}

impl FiniteSpaceForTuples for () {
    fn size(&self) -> usize {
        1
    }

    fn to_index(&self, _element: &Self::Element) -> usize {
        0
    }

    fn from_index(&self, index: usize) -> Option<Self::Element> {
        if index == 0 {
            Some(())
        } else {
            None
        }
    }

    fn from_index_unchecked(&self, _index: usize) -> Option<Self::Element> {
        Some(())
    }
}

#[impl_for_tuples(1, 12)]
#[tuple_types_custom_trait_bound(FiniteSpace)]
impl FiniteSpaceForTuples for Tuple {
    fn size(&self) -> usize {
        for_tuples!( #( self.Tuple.size() )** )
    }

    fn to_index(&self, element: &Self::Element) -> usize {
        let mut index: usize = 0;
        for_tuples!( #(
            index *= self.Tuple.size();
            index += self.Tuple.to_index(&element.Tuple);
        )* );
        index
    }

    fn from_index(&self, mut index: usize) -> Option<Self::Element> {
        let sizes = [for_tuples!( #( self.Tuple.size() ),* )];
        let mut indices = [for_tuples!(#(0),*)];
        for (index_part, &size) in indices.iter_mut().rev().zip(sizes.iter().rev()) {
            *index_part = index % size;
            index /= size;
        }
        if index != 0 {
            return None;
        }

        let mut indices_iter = IntoIter::new(indices);
        Some(for_tuples!( ( #( self.Tuple.from_index(indices_iter.next().unwrap())? ),* ) ))
    }

    fn from_index_unchecked(&self, mut index: usize) -> Option<Self::Element> {
        self.from_index(index)
    }
}

#[impl_for_tuples(1, 12)]
#[tuple_types_custom_trait_bound(BaseFeatureSpace)]
impl BaseFeatureSpaceForTuples for Tuple {
    fn num_features(&self) -> usize {
        for_tuples!( #( self.Tuple.num_features() )+* )
    }
}

impl BaseFeatureSpaceForTuples for () {
    fn num_features(&self) -> usize {
        0
    }
}

#[impl_for_tuples(1, 12)]
#[tuple_types_custom_trait_bound(FeatureSpaceOut<Tensor>)]
impl FeatureSpaceOutForTuples<Tensor> for Tuple {
    fn features_out(&self, element: &Self::Element, out: &mut Tensor, zeroed: bool) {
        // Feature vectors are the concatenation of the inner feature vectors.
        // Partion the tensor into a view for each inner space feature vector.
        let sizes = [for_tuples!( #( self.Tuple.num_features() as i64 ),* )];
        let mut views_iter = out.split_with_sizes(&sizes, -1).into_iter();
        for_tuples!( #(
            self.Tuple.features_out(&element.Tuple, &mut views_iter.next().unwrap(), zeroed);
        )* );
    }
}

impl FeatureSpaceOutForTuples<Tensor> for () {
    fn features_out(&self, _element: &Self::Element, _out: &mut Tensor, _zeroed: bool) {}
}

#[impl_for_tuples(1, 12)]
#[tuple_types_custom_trait_bound(BatchFeatureSpaceOut<Tensor>)]
impl BatchFeatureSpaceOutForTuples<Tensor> for Tuple {
    fn batch_features_out<'a, I>(
        &self,
        elements: I,
        out: &mut Tensor,
        zeroed: bool,
        _marker: PhantomData<&'a Self::Element>,
    ) where
        I: IntoIterator<Item = &'a Self::Element>,
        Self::Element: 'a,
    {
        let elements = elements.into_iter();
        let num_elements: usize = out
            .size2()
            .expect("out must be a 2D tensor")
            .0
            .try_into()
            .unwrap();

        // Unzip and collect elements into vectors
        let mut split_elements = (for_tuples!( #( Vec::with_capacity(num_elements) ),* ));
        for element in elements.into_iter() {
            for_tuples!( #(
                split_elements.Tuple.push(&element.Tuple);
            )* );
        }

        // Partion the tensor into views and fill the inner batch features for each.
        let sizes = [for_tuples!( #( self.Tuple.num_features() as i64 ),* )];
        let mut views_iter = out.split_with_sizes(&sizes, -1).into_iter();
        for_tuples!( #(
            self.Tuple.batch_features_out(split_elements.Tuple, &mut views_iter.next().unwrap(), zeroed);
        )* );
    }
}

impl BatchFeatureSpaceOutForTuples<Tensor> for () {
    fn batch_features_out<'a, I>(
        &self,
        _elements: I,
        _out: &mut Tensor,
        _zeroed: bool,
        _marker: PhantomData<&'a Self::Element>,
    ) where
        I: IntoIterator<Item = &'a Self::Element>,
        Self::Element: 'a,
    {
    }
}

impl SampleSpaceForTuples for () {
    fn sample<R: Rng + ?Sized>(&self, _rng: &mut R) -> Self::Element {}
}

#[impl_for_tuples(1, 12)]
#[tuple_types_custom_trait_bound(SampleSpace)]
impl SampleSpaceForTuples for Tuple {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Self::Element {
        for_tuples!( ( #( self.Tuple.sample(rng) ),* ) )
    }
}

#[cfg(test)]
#[allow(clippy::needless_pass_by_value)]
mod space {
    use super::super::{testing, IndexSpace, SingletonSpace};
    use super::*;
    use rstest::{fixture, rstest};

    type IndexTriple = ProductSpace<(IndexSpace, IndexSpace, IndexSpace)>;

    #[fixture]
    const fn index_314() -> IndexTriple {
        ProductSpace::new((IndexSpace::new(3), IndexSpace::new(1), IndexSpace::new(4)))
    }

    #[rstest]
    fn index_triple_contains_zero(index_314: IndexTriple) {
        assert!(index_314.contains(&(0, 0, 0)));
    }

    #[rstest]
    fn index_triple_contains_nonzero(index_314: IndexTriple) {
        assert!(index_314.contains(&(2, 0, 1)));
    }

    #[rstest]
    fn index_triple_contains_samples(index_314: IndexTriple) {
        testing::check_contains_samples(&index_314, 100);
    }

    type UnitSpace = ProductSpace<()>;

    #[test]
    fn unit_contains_unit() {
        let space = UnitSpace::new(());
        assert!(space.contains(&()));
    }

    #[test]
    fn unit_contains_samples() {
        let space = UnitSpace::new(());
        testing::check_contains_samples(&space, 10);
    }

    #[test]
    fn mixed_contains() {
        let space = ProductSpace::new((IndexSpace::new(3), SingletonSpace::new()));
        assert!(space.contains(&(2, ())));
    }

    #[test]
    fn mixed_not_contains_invalid() {
        let space = ProductSpace::new((IndexSpace::new(3), SingletonSpace::new()));
        assert!(!space.contains(&(4, ())));
    }
}

#[cfg(test)]
#[allow(clippy::needless_pass_by_value)]
mod finite_space {
    use super::super::{testing, IndexSpace, SingletonSpace};
    use super::*;
    use rstest::{fixture, rstest};

    type IndexTriple = ProductSpace<(IndexSpace, IndexSpace, IndexSpace)>;

    #[fixture]
    const fn index_314() -> IndexTriple {
        ProductSpace::new((IndexSpace::new(3), IndexSpace::new(1), IndexSpace::new(4)))
    }

    #[rstest]
    fn index_triple_size(index_314: IndexTriple) {
        assert_eq!(index_314.size(), 12);
    }

    #[rstest]
    fn index_triple_to_index_zeros(index_314: IndexTriple) {
        assert_eq!(index_314.to_index(&(0, 0, 0)), 0);
    }

    #[rstest]
    fn index_triple_to_index_nonzero(index_314: IndexTriple) {
        assert_eq!(index_314.to_index(&(2, 0, 1)), 2 * 4 + 1);
    }

    #[rstest]
    fn index_triple_from_index_zero(index_314: IndexTriple) {
        assert_eq!(index_314.from_index(0), Some((0, 0, 0)));
    }

    #[rstest]
    fn index_triple_from_index_nonzero(index_314: IndexTriple) {
        assert_eq!(index_314.from_index(9), Some((2, 0, 1)));
    }

    #[rstest]
    fn index_triple_from_to_index_iter_size(index_314: IndexTriple) {
        testing::check_from_to_index_iter_size(&index_314);
    }

    #[rstest]
    fn index_triple_from_index_invalid(index_314: IndexTriple) {
        testing::check_from_index_invalid(&index_314);
    }

    type UnitSpace = ProductSpace<()>;

    #[test]
    fn unit_size() {
        let space = UnitSpace::new(());
        assert_eq!(space.size(), 1);
    }

    #[test]
    fn unit_from_to_index_iter_size() {
        let space = UnitSpace::new(());
        testing::check_from_to_index_iter_size(&space);
    }

    #[test]
    fn unit_from_index_sampled() {
        let space = UnitSpace::new(());
        testing::check_from_index_sampled(&space, 10);
    }

    #[test]
    fn unit_from_index_invalid() {
        let space = UnitSpace::new(());
        testing::check_from_index_invalid(&space);
    }

    type MixedSpace = ProductSpace<(IndexSpace, SingletonSpace)>;

    #[fixture]
    const fn mixed() -> MixedSpace {
        ProductSpace::new((IndexSpace::new(3), SingletonSpace::new()))
    }

    #[rstest]
    fn mixed_size(mixed: MixedSpace) {
        assert_eq!(mixed.size(), 3);
    }

    #[rstest]
    fn mixed_from_to_index_iter_size(mixed: MixedSpace) {
        testing::check_from_to_index_iter_size(&mixed);
    }

    #[rstest]
    fn mixed_from_index_sampled(mixed: MixedSpace) {
        testing::check_from_index_sampled(&mixed, 20);
    }

    #[rstest]
    fn mixed_from_index_invalid(mixed: MixedSpace) {
        testing::check_from_index_invalid(&mixed);
    }
}

#[cfg(test)]
mod base_feature_space {
    use super::super::{IndexSpace, SingletonSpace};
    use super::*;

    #[test]
    fn index_triple_num_features() {
        let space = ProductSpace::new((IndexSpace::new(3), IndexSpace::new(1), IndexSpace::new(4)));
        assert_eq!(space.num_features(), 8);
    }

    #[test]
    fn unit_num_features() {
        let space = ProductSpace::new(());
        assert_eq!(space.num_features(), 0);
    }

    #[test]
    fn mixed_num_features() {
        let space = ProductSpace::new((IndexSpace::new(3), SingletonSpace::new()));
        assert_eq!(space.num_features(), 3);
    }
}

#[cfg(test)]
mod feature_space {
    use super::super::{IndexSpace, SingletonSpace};
    use super::*;

    macro_rules! features_tests {
        ($label:ident, $inner:expr, $elem:expr, $expected:expr) => {
            mod $label {
                use super::*;

                #[test]
                fn tensor_features() {
                    let space = ProductSpace::new($inner);
                    let actual: Tensor = space.features(&$elem);
                    let expected_vec: &[f32] = &$expected;
                    assert_eq!(actual, Tensor::of_slice(expected_vec));
                }

                #[test]
                fn tensor_features_out() {
                    let space = ProductSpace::new($inner);
                    let expected_vec: &[f32] = &$expected;
                    let expected = Tensor::of_slice(&expected_vec);
                    let mut out = expected.empty_like();
                    space.features_out(&$elem, &mut out, false);
                    assert_eq!(out, expected);
                }
            }
        };
    }

    features_tests!(unit, (), (), []);
    features_tests!(
        index_triple,
        (IndexSpace::new(3), IndexSpace::new(1), IndexSpace::new(4)),
        (1, 0, 2),
        [0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0]
    );
    features_tests!(
        mixed,
        (IndexSpace::new(3), SingletonSpace::new()),
        (0, ()),
        [1.0, 0.0, 0.0]
    );
}

#[cfg(test)]
mod batch_feature_space {
    use super::super::{IndexSpace, SingletonSpace};
    use super::*;

    fn tensor_from_arrays<const N: usize, const M: usize>(data: [[f32; M]; N]) -> Tensor {
        let flat_data: Vec<f32> = IntoIter::new(data).map(IntoIter::new).flatten().collect();
        Tensor::of_slice(&flat_data).reshape(&[N as i64, M as i64])
    }

    macro_rules! batch_features_tests {
        ($label:ident, $inner:expr, $elems:expr, $expected:expr) => {
            mod $label {
                use super::*;

                #[test]
                fn tensor_batch_features() {
                    let space = ProductSpace::new($inner);
                    let actual: Tensor = space.batch_features(&$elems);
                    assert_eq!(actual, tensor_from_arrays($expected));
                }

                #[test]
                fn tensor_batch_features_out() {
                    let space = ProductSpace::new($inner);
                    let expected = tensor_from_arrays($expected);
                    let mut out = expected.empty_like();
                    space.batch_features_out(&$elems, &mut out, false);
                    assert_eq!(out, expected);
                }
            }
        };
    }

    batch_features_tests!(unit, (), [(), (), ()], [[], [], []]);
    batch_features_tests!(
        index_triple,
        (IndexSpace::new(3), IndexSpace::new(1), IndexSpace::new(4)),
        [(0, 0, 0), (1, 0, 2)],
        [
            [1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0]
        ]
    );
    batch_features_tests!(
        mixed,
        (IndexSpace::new(3), SingletonSpace::new()),
        [(0, ()), (2, ())],
        [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]
    );
}
