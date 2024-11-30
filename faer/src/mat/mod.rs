use crate::{Shape, Stride, Unbind};
use core::{marker::PhantomData, ptr::NonNull};
use faer_traits::Conjugate;
use reborrow::*;

pub(crate) struct MatView<T: ?Sized, Rows, Cols, RStride, CStride> {
    ptr: NonNull<T>,
    nrows: Rows,
    ncols: Cols,
    row_stride: RStride,
    col_stride: CStride,
}

pub trait MatIndex<RowRange, ColRange> {
    type Target;

    fn get(this: Self, row: RowRange, col: ColRange) -> Self::Target;

    unsafe fn get_unchecked(this: Self, row: RowRange, col: ColRange) -> Self::Target;
}

impl<T: ?Sized, Rows: Copy, Cols: Copy, RStride: Copy, CStride: Copy> Copy
    for MatView<T, Rows, Cols, RStride, CStride>
{
}
impl<T: ?Sized, Rows: Copy, Cols: Copy, RStride: Copy, CStride: Copy> Clone
    for MatView<T, Rows, Cols, RStride, CStride>
{
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

#[inline]
#[track_caller]
fn from_slice_assert(nrows: usize, ncols: usize, len: usize) {
    let size = usize::checked_mul(nrows, ncols);
    assert!(size == Some(len));
}

mod mat_index;

pub(crate) mod matmut;
pub(crate) mod matown;
pub(crate) mod matref;

pub use matmut::MatMut;
pub use matown::Mat;
pub use matref::MatRef;

pub trait AsMatMut: AsMatRef {
    fn as_mat_mut(&mut self) -> MatMut<Self::T, Self::Rows, Self::Cols>;
}
pub trait AsMatRef {
    type T;
    type Rows: Shape;
    type Cols: Shape;

    fn as_mat_ref(&self) -> MatRef<Self::T, Self::Rows, Self::Cols>;
}

impl<M: AsMatRef> AsMatRef for &M {
    type T = M::T;
    type Rows = M::Rows;
    type Cols = M::Cols;

    #[inline]
    fn as_mat_ref(&self) -> MatRef<Self::T, Self::Rows, Self::Cols> {
        (**self).as_mat_ref()
    }
}
impl<M: AsMatRef> AsMatRef for &mut M {
    type T = M::T;
    type Rows = M::Rows;
    type Cols = M::Cols;

    #[inline]
    fn as_mat_ref(&self) -> MatRef<Self::T, Self::Rows, Self::Cols> {
        (**self).as_mat_ref()
    }
}
impl<M: AsMatMut> AsMatMut for &mut M {
    #[inline]
    fn as_mat_mut(&mut self) -> MatMut<Self::T, Self::Rows, Self::Cols> {
        (**self).as_mat_mut()
    }
}

impl<T, Rows: Shape, Cols: Shape, RStride: Stride, CStride: Stride> AsMatRef
    for MatRef<'_, T, Rows, Cols, RStride, CStride>
{
    type T = T;
    type Rows = Rows;
    type Cols = Cols;

    #[inline]
    fn as_mat_ref(&self) -> MatRef<T, Rows, Cols> {
        self.as_dyn_stride()
    }
}

impl<T, Rows: Shape, Cols: Shape, RStride: Stride, CStride: Stride> AsMatRef
    for MatMut<'_, T, Rows, Cols, RStride, CStride>
{
    type T = T;
    type Rows = Rows;
    type Cols = Cols;

    #[inline]
    fn as_mat_ref(&self) -> MatRef<T, Rows, Cols> {
        self.rb().as_dyn_stride()
    }
}

impl<T, Rows: Shape, Cols: Shape, RStride: Stride, CStride: Stride> AsMatMut
    for MatMut<'_, T, Rows, Cols, RStride, CStride>
{
    #[inline]
    fn as_mat_mut(&mut self) -> MatMut<T, Rows, Cols> {
        self.rb_mut().as_dyn_stride_mut()
    }
}

impl<T, Rows: Shape, Cols: Shape> AsMatRef for Mat<T, Rows, Cols> {
    type T = T;
    type Rows = Rows;
    type Cols = Cols;

    #[inline]
    fn as_mat_ref(&self) -> MatRef<T, Rows, Cols> {
        self.as_dyn_stride()
    }
}

impl<T, Rows: Shape, Cols: Shape> AsMatMut for Mat<T, Rows, Cols> {
    #[inline]
    fn as_mat_mut(&mut self) -> MatMut<T, Rows, Cols> {
        self.as_dyn_stride_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn test_mat() {
        let _x = crate::mat![[0.0, 1.0]];
        let mat = Mat::from_fn(3, 4, |i, j| i as f64 + j as f64);

        let mat = mat.as_ref().cloned();
        let mat = mat.as_ref();

        for i in 0..3 {
            for j in 0..4 {
                zipped!(&mat).map(|x| x).as_ref().at(i, j);
            }
        }
    }

    #[test]
    fn test_mat_complex() {
        let _x = mat![[c64::new(0.0, 0.0), c64::new(1.0, 0.0)]];
        let mat = Mat::from_fn(3, 4, |i, j| c64::new(i as f64 + j as f64, 0.0));
        {
            let _conj = mat.as_ref().conjugate();
        }

        let mat = mat.as_ref().cloned();
        let mat = mat.as_ref();

        for i in 0..3 {
            for j in 0..4 {
                zipped!(&mat).map(|x| x).as_ref().at(i, j);
            }
        }
    }
}
