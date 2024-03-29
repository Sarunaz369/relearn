extern crate proc_macro;
use proc_macro::TokenStream;

mod indexed;
mod space;

use syn::{GenericParam, Generics, TypeParamBound};

/// Derive `relearn::spaces::Indexed` for an enum without internal data.
#[proc_macro_derive(Indexed)]
pub fn indexed_macro_derive(input: TokenStream) -> TokenStream {
    // Construct a representation of Rust code as a syntax tree
    // that we can manipulate
    let ast = syn::parse(input).unwrap();

    // Build the trait implementation
    indexed::impl_indexed_macro(&ast)
}

/// Derive `relearn::spaces::Space` for a struct as a Cartesian product space of its fields.
///
/// See [`ProductSpace`] to derive `Space` and all other basic space traits.
///
/// Each of the struct fields must also implement `Space`
/// (when all generic params are bounded as `Space`).
///
/// If the struct has named fields then you must also specify `#[element(ElementType)]`
/// where `ElementType` is the name of a struct with the same field names and types equal to the
/// corresponding space element. The element type may also contain generics, like
/// `#[element(ElementType<T::Element>)]`.
///
/// # Examples
/// ```
/// use relearn::spaces::{BooleanSpace, Space};
///
/// #[derive(Clone)]
/// struct MyElem<T> {
///     first: bool,
///     second: T,
/// }
///
/// #[derive(Space)]
/// #[element(MyElem<T::Element>)]
/// struct MySpace<T> {
///     first: BooleanSpace,
///     second: T,
/// }
/// ```
///
/// For structs with unnamed fields, the element is always a tuple
/// and `#[element(...)]` is ignored:
/// ```
/// # use relearn::spaces::{BooleanSpace, IndexSpace, Space};
/// #[derive(Space)]
/// struct PairSpace(BooleanSpace, IndexSpace);
/// ```
#[proc_macro_derive(Space, attributes(element))]
pub fn space_macro_derive(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    space::impl_space_trait_macro::<space::SpaceImpl>(ast)
}

/// Derive `relearn::spaces::SubsetOrd` for a struct as a Cartesian product space of its fields.
///
/// Expects that `Space` will be implemented according to `#[derive(Space)]`.
#[proc_macro_derive(SubsetOrd)]
pub fn subset_ord_macro_derive(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    space::impl_space_trait_macro::<space::SubsetOrdImpl>(ast)
}

/// Derive `relearn::spaces::FiniteSpace` for a struct as a Cartesian product space of its fields.
///
/// Expects that `Space` will be implemented according to `#[derive(Space)]`.
#[proc_macro_derive(FiniteSpace)]
pub fn finite_space_macro_derive(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    space::impl_space_trait_macro::<space::FiniteSpaceImpl>(ast)
}

/// Derive `relearn::spaces::NonEmptySpace` for a struct as a Cartesian product space of its fields.
///
/// Expects that `Space` will be implemented according to `#[derive(Space)]`.
#[proc_macro_derive(NonEmptySpace)]
pub fn non_empty_space_macro_derive(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    space::impl_space_trait_macro::<space::NonEmptySpaceImpl>(ast)
}

/// Derive `relearn::spaces::SampleSpace` for a struct as a Cartesian product space of its fields.
///
/// Actually implements `rand::distributions::Distribution<Self::Element>`,
/// for which there is a blanket implementation of `SampleSpace`.
///
/// Expects that `Space` will be implemented according to `#[derive(Space)]`.
#[proc_macro_derive(SampleSpace)]
pub fn sample_space_macro_derive(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    space::impl_space_trait_macro::<space::SampleSpaceImpl>(ast)
}

/// Derive `relearn::spaces::FeatureSpace` for a struct as a Cartesian product space of its fields.
///
/// Expects that `Space` will be implemented according to `#[derive(Space)]`.
#[proc_macro_derive(FeatureSpace)]
pub fn feature_space_macro_derive(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    space::impl_space_trait_macro::<space::FeatureSpaceImpl>(ast)
}

/// Derive `relearn::spaces::LogElementSpace` for a struct.
///
/// Logs the contents of each field under separate sub-fields.
#[proc_macro_derive(LogElementSpace)]
pub fn log_element_space_macro_derive(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    space::impl_space_trait_macro::<space::LogElementSpaceImpl>(ast)
}

/// Derive `Space` and other space traits for a struct as a Cartesian product space of its fields.
///
/// Derives the following traits:
/// [`Space`], [`SubsetOrd`], [`NonEmptySpace`], [`SampleSpace`], [`FeatureSpace`],
/// and [`LogElementSpace`].
///
/// Does not derive [`FiniteSpace`].
///
/// Each of the struct fields must also implement `Space`
/// (when all generic params are bounded as `Space`).
///
/// If the struct has named fields then you must also specify `#[element(ElementType)]`
/// where `ElementType` is the name of a struct with the same field names and types equal to the
/// corresponding space element. The element type may also contain generics, like
/// `#[element(ElementType<T::Element>)]`.
///
/// # Examples
/// ```
/// use relearn::spaces::{BooleanSpace, Space, ProductSpace};
///
/// #[derive(Clone)]
/// struct MyElem<T> {
///     first: bool,
///     second: T,
/// }
///
/// #[derive(PartialEq, ProductSpace)]
/// #[element(MyElem<T::Element>)]
/// struct MySpace<T> {
///     first: BooleanSpace,
///     second: T,
/// }
/// ```
///
/// For structs with unnamed fields, the element is always a tuple
/// and `#[element(...)]` is ignored:
/// ```
/// # use relearn::spaces::{BooleanSpace, IndexSpace, ProductSpace};
/// #[derive(PartialEq, ProductSpace)]
/// struct PairSpace(BooleanSpace, IndexSpace);
/// ```
#[proc_macro_derive(ProductSpace, attributes(element))]
pub fn product_space_macro_derive(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    space::impl_space_trait_macro::<space::ProductSpaceImpl>(ast)
}

fn add_trait_bounds(mut generics: Generics, bound: &TypeParamBound) -> Generics {
    for param in &mut generics.params {
        if let GenericParam::Type(ref mut type_param) = *param {
            type_param.bounds.push(bound.clone())
        }
    }
    generics
}
