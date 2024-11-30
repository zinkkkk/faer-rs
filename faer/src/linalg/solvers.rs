use crate::{assert, get_global_parallelism, internal_prelude::*};
use dyn_stack::GlobalMemBuffer;
use faer_traits::math_utils;
use linalg::svd::ComputeSvdVectors;

pub use linalg::{
    cholesky::{ldlt::factor::LdltError, llt::factor::LltError},
    evd::EvdError,
    svd::SvdError,
};

pub trait ShapeCore {
    fn nrows(&self) -> usize;
    fn ncols(&self) -> usize;
}

pub trait SolveCore<T: ComplexField>: ShapeCore {
    fn solve_in_place_with_conj(&self, conj: Conj, rhs: MatMut<'_, T>);
    fn solve_transpose_in_place_with_conj(&self, conj: Conj, rhs: MatMut<'_, T>);
}
pub trait SolveLstsqCore<T: ComplexField>: ShapeCore {
    fn solve_lstsq_in_place_with_conj(&self, conj: Conj, rhs: MatMut<'_, T>);
}
pub trait DenseSolveCore<T: ComplexField>: SolveCore<T> {
    fn reconstruct(&self) -> Mat<T>;
    fn inverse(&self) -> Mat<T>;
}

pub trait Solve<T: ComplexField>: SolveCore<T> {}
pub trait SolveLstsq<T: ComplexField>: SolveLstsqCore<T> {}
pub trait DenseSolve<T: ComplexField>: DenseSolveCore<T> {}

impl<T: ComplexField, S: SolveCore<T>> Solve<T> for S {}
impl<T: ComplexField, S: SolveLstsqCore<T>> SolveLstsq<T> for S {}
impl<T: ComplexField, S: DenseSolveCore<T>> DenseSolve<T> for S {}

#[derive(Clone, Debug)]
pub struct Cholesky<T> {
    L: Mat<T>,
}

#[derive(Clone, Debug)]
pub struct Ldlt<T> {
    L: Mat<T>,
    D: Diag<T>,
}

#[derive(Clone, Debug)]
pub struct Lblt<T> {
    L: Mat<T>,
    B_diag: Diag<T>,
    B_subdiag: Diag<T>,
    P: Perm<usize>,
}

#[derive(Clone, Debug)]
pub struct PartialPivLu<T> {
    L: Mat<T>,
    U: Mat<T>,
    P: Perm<usize>,
}

#[derive(Clone, Debug)]
pub struct FullPivLu<T> {
    L: Mat<T>,
    U: Mat<T>,
    P: Perm<usize>,
    Q: Perm<usize>,
}

#[derive(Clone, Debug)]
pub struct Qr<T> {
    Q_basis: Mat<T>,
    Q_coeff: Mat<T>,
    R: Mat<T>,
}

#[derive(Clone, Debug)]
pub struct ColPivQr<T> {
    Q_basis: Mat<T>,
    Q_coeff: Mat<T>,
    R: Mat<T>,
    P: Perm<usize>,
}

#[derive(Clone, Debug)]
pub struct Svd<T> {
    U: Mat<T>,
    V: Mat<T>,
    S: Diag<T>,
}

#[derive(Clone, Debug)]
pub struct SelfAdjointEigen<T> {
    U: Mat<T>,
    S: Diag<T>,
}

#[derive(Clone, Debug)]
pub struct Eigen<T> {
    U: Mat<Complex<T>>,
    S: Diag<Complex<T>>,
}

impl<T: ComplexField> Cholesky<T> {
    #[track_caller]
    pub fn new<C: Conjugate<Canonical = T>>(
        A: MatRef<'_, C>,
        side: Side,
    ) -> Result<Self, LltError> {
        assert!(all(A.nrows() == A.ncols()));
        let n = A.nrows();

        let mut L = Mat::zeros(n, n);
        match side {
            Side::Lower => L.copy_from_triangular_lower(A),
            Side::Upper => L.copy_from_triangular_lower(A.adjoint()),
        }

        Self::new_imp(L)
    }

    #[track_caller]
    fn new_imp(mut L: Mat<T>) -> Result<Self, LltError> {
        let par = get_global_parallelism();

        let n = L.nrows();

        let mut mem = GlobalMemBuffer::new(
            linalg::cholesky::llt::factor::cholesky_in_place_scratch::<T>(n, par, auto!(T))
                .unwrap(),
        );
        let stack = DynStack::new(&mut mem);

        linalg::cholesky::llt::factor::cholesky_in_place(
            L.as_mut(),
            Default::default(),
            par,
            stack,
            auto!(T),
        )?;
        z!(&mut L).for_each_triangular_upper(linalg::zip::Diag::Skip, |uz!(x)| *x = zero());

        Ok(Self { L })
    }

    pub fn L(&self) -> MatRef<'_, T> {
        self.L.as_ref()
    }
}

impl<T: ComplexField> Ldlt<T> {
    #[track_caller]
    pub fn new<C: Conjugate<Canonical = T>>(
        A: MatRef<'_, C>,
        side: Side,
    ) -> Result<Self, LdltError> {
        assert!(all(A.nrows() == A.ncols()));
        let n = A.nrows();

        let mut L = Mat::zeros(n, n);
        match side {
            Side::Lower => L.copy_from_triangular_lower(A),
            Side::Upper => L.copy_from_triangular_lower(A.adjoint()),
        }

        Self::new_imp(L)
    }

    #[track_caller]
    fn new_imp(mut L: Mat<T>) -> Result<Self, LdltError> {
        let par = get_global_parallelism();

        let n = L.nrows();
        let mut D = Diag::zeros(n);

        let mut mem = GlobalMemBuffer::new(
            linalg::cholesky::llt::factor::cholesky_in_place_scratch::<T>(n, par, auto!(T))
                .unwrap(),
        );
        let stack = DynStack::new(&mut mem);

        linalg::cholesky::ldlt::factor::cholesky_in_place(
            L.as_mut(),
            Default::default(),
            par,
            stack,
            auto!(T),
        )?;

        D.copy_from(L.diagonal());
        L.diagonal_mut().fill(one());
        z!(&mut L).for_each_triangular_upper(linalg::zip::Diag::Skip, |uz!(x)| *x = zero());

        Ok(Self { L, D })
    }

    pub fn L(&self) -> MatRef<'_, T> {
        self.L.as_ref()
    }

    pub fn D(&self) -> DiagRef<'_, T> {
        self.D.as_ref()
    }
}

impl<T: ComplexField> Lblt<T> {
    #[track_caller]
    pub fn new<C: Conjugate<Canonical = T>>(A: MatRef<'_, C>, side: Side) -> Self {
        assert!(all(A.nrows() == A.ncols()));
        let n = A.nrows();

        let mut L = Mat::zeros(n, n);
        match side {
            Side::Lower => L.copy_from_triangular_lower(A),
            Side::Upper => L.copy_from_triangular_lower(A.adjoint()),
        }
        Self::new_imp(L)
    }

    #[track_caller]
    fn new_imp(mut L: Mat<T>) -> Self {
        let par = get_global_parallelism();

        let n = L.nrows();

        let mut diag = Diag::zeros(n);
        let mut subdiag = Diag::zeros(n);
        let mut perm_fwd = vec![0usize; n];
        let mut perm_bwd = vec![0usize; n];

        let mut mem = GlobalMemBuffer::new(
            linalg::cholesky::llt::factor::cholesky_in_place_scratch::<T>(n, par, auto!(T))
                .unwrap(),
        );
        let stack = DynStack::new(&mut mem);

        linalg::cholesky::bunch_kaufman::factor::cholesky_in_place(
            L.as_mut(),
            subdiag.as_mut(),
            Default::default(),
            &mut perm_fwd,
            &mut perm_bwd,
            par,
            stack,
            auto!(T),
        );

        diag.copy_from(L.diagonal());
        L.diagonal_mut().fill(one());
        z!(&mut L).for_each_triangular_upper(linalg::zip::Diag::Skip, |uz!(x)| *x = zero());

        Self {
            L,
            B_diag: diag,
            B_subdiag: subdiag,
            P: unsafe {
                Perm::new_unchecked(perm_fwd.into_boxed_slice(), perm_bwd.into_boxed_slice())
            },
        }
    }

    pub fn L(&self) -> MatRef<'_, T> {
        self.L.as_ref()
    }

    pub fn B_diag(&self) -> DiagRef<'_, T> {
        self.B_diag.as_ref()
    }

    pub fn B_subdiag(&self) -> DiagRef<'_, T> {
        self.B_subdiag.as_ref()
    }

    pub fn P(&self) -> PermRef<'_, usize> {
        self.P.as_ref()
    }
}

fn split_LU<T: ComplexField>(LU: Mat<T>) -> (Mat<T>, Mat<T>) {
    let (m, n) = LU.shape();
    let size = Ord::min(m, n);

    let (L, U) = if m >= n {
        let mut L = LU;
        let mut U = Mat::zeros(size, size);

        U.copy_from_triangular_upper(&L);

        z!(&mut L).for_each_triangular_upper(linalg::zip::Diag::Skip, |uz!(x)| *x = zero());
        L.diagonal_mut().fill(one());

        (L, U)
    } else {
        let mut U = LU;
        let mut L = Mat::zeros(size, size);

        L.copy_from_strict_triangular_lower(&U);

        z!(&mut U).for_each_triangular_lower(linalg::zip::Diag::Skip, |uz!(x)| *x = zero());
        L.diagonal_mut().fill(one());

        (L, U)
    };
    (L, U)
}

impl<T: ComplexField> PartialPivLu<T> {
    #[track_caller]
    pub fn new<C: Conjugate<Canonical = T>>(A: MatRef<'_, C>) -> Self {
        let LU = A.to_owned();
        Self::new_imp(LU)
    }
    #[track_caller]
    fn new_imp(mut LU: Mat<T>) -> Self {
        let par = get_global_parallelism();

        let (m, n) = LU.shape();
        let mut row_perm_fwd = vec![0usize; m];
        let mut row_perm_bwd = vec![0usize; m];

        linalg::lu::partial_pivoting::factor::lu_in_place(
            LU.as_mut(),
            &mut row_perm_fwd,
            &mut row_perm_bwd,
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::lu::partial_pivoting::factor::lu_in_place_scratch::<usize, T>(
                    m,
                    n,
                    par,
                    auto!(T),
                )
                .unwrap(),
            )),
            auto!(T),
        );

        let (L, U) = split_LU(LU);

        Self {
            L,
            U,
            P: unsafe {
                Perm::new_unchecked(
                    row_perm_fwd.into_boxed_slice(),
                    row_perm_bwd.into_boxed_slice(),
                )
            },
        }
    }

    pub fn L(&self) -> MatRef<'_, T> {
        self.L.as_ref()
    }

    pub fn U(&self) -> MatRef<'_, T> {
        self.U.as_ref()
    }

    pub fn P(&self) -> PermRef<'_, usize> {
        self.P.as_ref()
    }
}

impl<T: ComplexField> FullPivLu<T> {
    #[track_caller]
    pub fn new<C: Conjugate<Canonical = T>>(A: MatRef<'_, C>) -> Self {
        let LU = A.to_owned();
        Self::new_imp(LU)
    }
    #[track_caller]
    fn new_imp(mut LU: Mat<T>) -> Self {
        let par = get_global_parallelism();

        let (m, n) = LU.shape();
        let mut row_perm_fwd = vec![0usize; m];
        let mut row_perm_bwd = vec![0usize; m];
        let mut col_perm_fwd = vec![0usize; n];
        let mut col_perm_bwd = vec![0usize; n];

        linalg::lu::full_pivoting::factor::lu_in_place(
            LU.as_mut(),
            &mut row_perm_fwd,
            &mut row_perm_bwd,
            &mut col_perm_fwd,
            &mut col_perm_bwd,
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::lu::partial_pivoting::factor::lu_in_place_scratch::<usize, T>(
                    m,
                    n,
                    par,
                    auto!(T),
                )
                .unwrap(),
            )),
            auto!(T),
        );

        let (L, U) = split_LU(LU);

        Self {
            L,
            U,
            P: unsafe {
                Perm::new_unchecked(
                    row_perm_fwd.into_boxed_slice(),
                    row_perm_bwd.into_boxed_slice(),
                )
            },
            Q: unsafe {
                Perm::new_unchecked(
                    col_perm_fwd.into_boxed_slice(),
                    col_perm_bwd.into_boxed_slice(),
                )
            },
        }
    }

    pub fn L(&self) -> MatRef<'_, T> {
        self.L.as_ref()
    }

    pub fn U(&self) -> MatRef<'_, T> {
        self.U.as_ref()
    }

    pub fn P(&self) -> PermRef<'_, usize> {
        self.P.as_ref()
    }

    pub fn Q(&self) -> PermRef<'_, usize> {
        self.Q.as_ref()
    }
}

impl<T: ComplexField> Qr<T> {
    #[track_caller]
    pub fn new<C: Conjugate<Canonical = T>>(A: MatRef<'_, C>) -> Self {
        let QR = A.to_owned();
        Self::new_imp(QR)
    }
    #[track_caller]
    fn new_imp(mut QR: Mat<T>) -> Self {
        let par = get_global_parallelism();

        let (m, n) = QR.shape();
        let size = Ord::min(m, n);

        let blocksize = linalg::qr::no_pivoting::factor::recommended_blocksize::<T>(m, n);
        let mut Q_coeff = Mat::zeros(blocksize, size);

        linalg::qr::no_pivoting::factor::qr_in_place(
            QR.as_mut(),
            Q_coeff.as_mut(),
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::qr::no_pivoting::factor::qr_in_place_scratch::<T>(
                    m,
                    n,
                    blocksize,
                    par,
                    auto!(T),
                )
                .unwrap(),
            )),
            auto!(T),
        );

        let (Q_basis, R) = split_LU(QR);

        Self {
            Q_basis,
            Q_coeff,
            R,
        }
    }

    pub fn Q_basis(&self) -> MatRef<'_, T> {
        self.Q_basis.as_ref()
    }

    pub fn Q_coeff(&self) -> MatRef<'_, T> {
        self.Q_coeff.as_ref()
    }

    pub fn R(&self) -> MatRef<'_, T> {
        self.R.as_ref()
    }
}

impl<T: ComplexField> ColPivQr<T> {
    #[track_caller]
    pub fn new<C: Conjugate<Canonical = T>>(A: MatRef<'_, C>) -> Self {
        let QR = A.to_owned();
        Self::new_imp(QR)
    }
    #[track_caller]
    fn new_imp(mut QR: Mat<T>) -> Self {
        let par = get_global_parallelism();

        let (m, n) = QR.shape();
        let size = Ord::min(m, n);

        let mut col_perm_fwd = vec![0usize; n];
        let mut col_perm_bwd = vec![0usize; n];

        let blocksize = linalg::qr::no_pivoting::factor::recommended_blocksize::<T>(m, n);
        let mut Q_coeff = Mat::zeros(blocksize, size);

        linalg::qr::col_pivoting::factor::qr_in_place(
            QR.as_mut(),
            Q_coeff.as_mut(),
            &mut col_perm_fwd,
            &mut col_perm_bwd,
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::qr::col_pivoting::factor::qr_in_place_scratch::<usize, T>(
                    m,
                    n,
                    blocksize,
                    par,
                    auto!(T),
                )
                .unwrap(),
            )),
            auto!(T),
        );

        let (Q_basis, R) = split_LU(QR);

        Self {
            Q_basis,
            Q_coeff,
            R,
            P: unsafe {
                Perm::new_unchecked(
                    col_perm_fwd.into_boxed_slice(),
                    col_perm_bwd.into_boxed_slice(),
                )
            },
        }
    }

    pub fn Q_basis(&self) -> MatRef<'_, T> {
        self.Q_basis.as_ref()
    }

    pub fn Q_coeff(&self) -> MatRef<'_, T> {
        self.Q_coeff.as_ref()
    }

    pub fn R(&self) -> MatRef<'_, T> {
        self.R.as_ref()
    }

    pub fn P(&self) -> PermRef<'_, usize> {
        self.P.as_ref()
    }
}

impl<T: ComplexField> Svd<T> {
    #[track_caller]
    pub fn new<C: Conjugate<Canonical = T>>(A: MatRef<'_, C>) -> Result<Self, SvdError> {
        Self::new_imp(A.canonical(), Conj::get::<C>(), false)
    }

    #[track_caller]
    pub fn new_thin<C: Conjugate<Canonical = T>>(A: MatRef<'_, C>) -> Result<Self, SvdError> {
        Self::new_imp(A.canonical(), Conj::get::<C>(), true)
    }

    #[track_caller]
    fn new_imp(A: MatRef<'_, T>, conj: Conj, thin: bool) -> Result<Self, SvdError> {
        let par = get_global_parallelism();

        let (m, n) = A.shape();
        let size = Ord::min(m, n);

        let mut U = Mat::zeros(m, if thin { size } else { m });
        let mut V = Mat::zeros(n, if thin { size } else { n });
        let mut S = Diag::zeros(size);

        let compute = if thin {
            ComputeSvdVectors::Thin
        } else {
            ComputeSvdVectors::Full
        };

        linalg::svd::svd(
            A,
            S.as_mut(),
            Some(U.as_mut()),
            Some(V.as_mut()),
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::svd::svd_scratch::<T>(m, n, compute, compute, par, auto!(T)).unwrap(),
            )),
            auto!(T),
        )?;

        if conj == Conj::Yes {
            for c in U.col_iter_mut() {
                for x in c.iter_mut() {
                    *x = math_utils::conj(x);
                }
            }
            for c in V.col_iter_mut() {
                for x in c.iter_mut() {
                    *x = math_utils::conj(x);
                }
            }
        }

        Ok(Self { U, V, S })
    }

    pub fn U(&self) -> MatRef<'_, T> {
        self.U.as_ref()
    }

    pub fn V(&self) -> MatRef<'_, T> {
        self.V.as_ref()
    }

    pub fn S(&self) -> DiagRef<'_, T> {
        self.S.as_ref()
    }
}

impl<T: ComplexField> SelfAdjointEigen<T> {
    #[track_caller]
    pub fn new<C: Conjugate<Canonical = T>>(
        A: MatRef<'_, C>,
        side: Side,
    ) -> Result<Self, EvdError> {
        assert!(A.nrows() == A.ncols());

        match side {
            Side::Lower => Self::new_imp(A.canonical(), Conj::get::<C>()),
            Side::Upper => Self::new_imp(A.adjoint().canonical(), Conj::get::<C::Conj>()),
        }
    }

    #[track_caller]
    fn new_imp(A: MatRef<'_, T>, conj: Conj) -> Result<Self, EvdError> {
        let par = get_global_parallelism();

        let n = A.nrows();

        let mut U = Mat::zeros(n, n);
        let mut S = Diag::zeros(n);

        linalg::evd::self_adjoint_evd(
            A,
            S.as_mut(),
            Some(U.as_mut()),
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::evd::self_adjoint_evd_scratch::<T>(
                    n,
                    linalg::evd::ComputeEigenvectors::Yes,
                    par,
                    auto!(T),
                )
                .unwrap(),
            )),
            auto!(T),
        )?;

        if conj == Conj::Yes {
            for c in U.col_iter_mut() {
                for x in c.iter_mut() {
                    *x = math_utils::conj(x);
                }
            }
        }

        Ok(Self { U, S })
    }

    pub fn U(&self) -> MatRef<'_, T> {
        self.U.as_ref()
    }

    pub fn S(&self) -> DiagRef<'_, T> {
        self.S.as_ref()
    }
}

impl<T: RealField> Eigen<T> {
    #[track_caller]
    pub fn new<C: Conjugate<Canonical = Complex<T>>>(A: MatRef<'_, C>) -> Result<Self, EvdError> {
        assert!(A.nrows() == A.ncols());
        Self::new_imp(A.canonical(), Conj::get::<C>())
    }

    #[track_caller]
    pub fn new_from_real(A: MatRef<'_, T>) -> Result<Self, EvdError> {
        assert!(A.nrows() == A.ncols());

        let par = get_global_parallelism();

        let n = A.nrows();

        let mut U_real = Mat::zeros(n, n);
        let mut S_re = Diag::zeros(n);
        let mut S_im = Diag::zeros(n);

        linalg::evd::evd_real(
            A,
            S_re.as_mut(),
            S_im.as_mut(),
            None,
            Some(U_real.as_mut()),
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::evd::evd_scratch::<T>(
                    n,
                    linalg::evd::ComputeEigenvectors::No,
                    linalg::evd::ComputeEigenvectors::Yes,
                    par,
                    auto!(T),
                )
                .unwrap(),
            )),
            auto!(T),
        )?;

        let mut U = Mat::zeros(n, n);
        let mut S = Diag::zeros(n);

        let mut j = 0;
        while j < n {
            if S_im[j] == zero() {
                S[j] = Complex::new(S_re[j].clone(), zero());

                for i in 0..n {
                    U[(i, j)] = Complex::new(U_real[(i, j)].clone(), zero());
                }

                j += 1;
            } else {
                S[j] = Complex::new(S_re[j].clone(), S_im[j].clone());
                S[j + 1] = Complex::new(S_re[j].clone(), neg(&S_im[j]));

                for i in 0..n {
                    U[(i, j)] = Complex::new(U_real[(i, j)].clone(), U_real[(i, j + 1)].clone());
                    U[(i, j)] = Complex::new(U_real[(i, j)].clone(), neg(&U_real[(i, j + 1)]));
                }

                j += 2;
            }
        }

        Ok(Self { U, S })
    }

    fn new_imp(A: MatRef<'_, Complex<T>>, conj: Conj) -> Result<Self, EvdError> {
        let par = get_global_parallelism();

        let n = A.nrows();

        let mut U = Mat::zeros(n, n);
        let mut S = Diag::zeros(n);

        linalg::evd::evd_cplx(
            A,
            S.as_mut(),
            None,
            Some(U.as_mut()),
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::evd::evd_scratch::<Complex<T>>(
                    n,
                    linalg::evd::ComputeEigenvectors::No,
                    linalg::evd::ComputeEigenvectors::Yes,
                    par,
                    auto!(T),
                )
                .unwrap(),
            )),
            auto!(T),
        )?;

        if conj == Conj::Yes {
            for c in U.col_iter_mut() {
                for x in c.iter_mut() {
                    *x = math_utils::conj(x);
                }
            }
        }

        Ok(Self { U, S })
    }

    pub fn U(&self) -> MatRef<'_, Complex<T>> {
        self.U.as_ref()
    }

    pub fn S(&self) -> DiagRef<'_, Complex<T>> {
        self.S.as_ref()
    }
}

impl<T: ComplexField> ShapeCore for Cholesky<T> {
    #[inline]
    fn nrows(&self) -> usize {
        self.L().nrows()
    }
    #[inline]
    fn ncols(&self) -> usize {
        self.L().ncols()
    }
}
impl<T: ComplexField> ShapeCore for Ldlt<T> {
    #[inline]
    fn nrows(&self) -> usize {
        self.L().nrows()
    }
    #[inline]
    fn ncols(&self) -> usize {
        self.L().ncols()
    }
}
impl<T: ComplexField> ShapeCore for Lblt<T> {
    #[inline]
    fn nrows(&self) -> usize {
        self.L().nrows()
    }
    #[inline]
    fn ncols(&self) -> usize {
        self.L().ncols()
    }
}
impl<T: ComplexField> ShapeCore for PartialPivLu<T> {
    #[inline]
    fn nrows(&self) -> usize {
        self.L().nrows()
    }
    #[inline]
    fn ncols(&self) -> usize {
        self.U().ncols()
    }
}
impl<T: ComplexField> ShapeCore for FullPivLu<T> {
    #[inline]
    fn nrows(&self) -> usize {
        self.L().nrows()
    }
    #[inline]
    fn ncols(&self) -> usize {
        self.U().ncols()
    }
}
impl<T: ComplexField> ShapeCore for Qr<T> {
    #[inline]
    fn nrows(&self) -> usize {
        self.Q_basis().nrows()
    }
    #[inline]
    fn ncols(&self) -> usize {
        self.R().ncols()
    }
}
impl<T: ComplexField> ShapeCore for ColPivQr<T> {
    #[inline]
    fn nrows(&self) -> usize {
        self.Q_basis().nrows()
    }
    #[inline]
    fn ncols(&self) -> usize {
        self.R().ncols()
    }
}
impl<T: ComplexField> ShapeCore for Svd<T> {
    #[inline]
    fn nrows(&self) -> usize {
        self.U().nrows()
    }
    #[inline]
    fn ncols(&self) -> usize {
        self.V().nrows()
    }
}
impl<T: ComplexField> ShapeCore for SelfAdjointEigen<T> {
    #[inline]
    fn nrows(&self) -> usize {
        self.U().nrows()
    }
    #[inline]
    fn ncols(&self) -> usize {
        self.U().nrows()
    }
}
impl<T: RealField> ShapeCore for Eigen<T> {
    #[inline]
    fn nrows(&self) -> usize {
        self.U().nrows()
    }
    #[inline]
    fn ncols(&self) -> usize {
        self.U().nrows()
    }
}

impl<T: ComplexField> SolveCore<T> for Cholesky<T> {
    #[track_caller]
    fn solve_in_place_with_conj(&self, conj: Conj, rhs: MatMut<'_, T>) {
        let par = get_global_parallelism();

        let mut mem = GlobalMemBuffer::new(
            linalg::cholesky::llt::solve::solve_in_place_scratch::<T>(
                self.L.nrows(),
                rhs.ncols(),
                par,
            )
            .unwrap(),
        );
        let stack = DynStack::new(&mut mem);

        linalg::cholesky::llt::solve::solve_in_place_with_conj(
            self.L.as_ref(),
            conj,
            rhs,
            par,
            stack,
        );
    }

    #[track_caller]
    fn solve_transpose_in_place_with_conj(&self, conj: Conj, rhs: MatMut<'_, T>) {
        let par = get_global_parallelism();

        let mut mem = GlobalMemBuffer::new(
            linalg::cholesky::llt::solve::solve_in_place_scratch::<T>(
                self.L.nrows(),
                rhs.ncols(),
                par,
            )
            .unwrap(),
        );
        let stack = DynStack::new(&mut mem);

        linalg::cholesky::llt::solve::solve_in_place_with_conj(
            self.L.as_ref(),
            conj.compose(Conj::Yes),
            rhs,
            par,
            stack,
        );
    }
}

#[math]
fn make_self_adjoint<T: ComplexField>(mut A: MatMut<'_, T>) {
    assert!(A.nrows() == A.ncols());
    let n = A.nrows();
    for j in 0..n {
        A[(j, j)] = from_real(real(A[(j, j)]));
        for i in 0..j {
            A[(i, j)] = conj(A[(j, i)]);
        }
    }
}

impl<T: ComplexField> DenseSolveCore<T> for Cholesky<T> {
    #[track_caller]
    fn reconstruct(&self) -> Mat<T> {
        let par = get_global_parallelism();

        let n = self.L.nrows();
        let mut out = Mat::zeros(n, n);

        let mut mem = GlobalMemBuffer::new(
            linalg::cholesky::llt::reconstruct::reconstruct_scratch::<T>(n, par).unwrap(),
        );
        let stack = DynStack::new(&mut mem);

        linalg::cholesky::llt::reconstruct::reconstruct(out.as_mut(), self.L(), par, stack);

        make_self_adjoint(out.as_mut());
        out
    }

    #[track_caller]
    fn inverse(&self) -> Mat<T> {
        let par = get_global_parallelism();

        let n = self.L.nrows();
        let mut out = Mat::zeros(n, n);

        let mut mem = GlobalMemBuffer::new(
            linalg::cholesky::llt::inverse::inverse_scratch::<T>(n, par).unwrap(),
        );
        let stack = DynStack::new(&mut mem);

        linalg::cholesky::llt::inverse::inverse(out.as_mut(), self.L(), par, stack);

        make_self_adjoint(out.as_mut());
        out
    }
}

impl<T: ComplexField> SolveCore<T> for Ldlt<T> {
    #[track_caller]
    fn solve_in_place_with_conj(&self, conj: Conj, rhs: MatMut<'_, T>) {
        let par = get_global_parallelism();

        let mut mem = GlobalMemBuffer::new(
            linalg::cholesky::ldlt::solve::solve_in_place_scratch::<T>(
                self.L.nrows(),
                rhs.ncols(),
                par,
            )
            .unwrap(),
        );
        let stack = DynStack::new(&mut mem);

        linalg::cholesky::ldlt::solve::solve_in_place_with_conj(
            self.L.as_ref(),
            self.D.as_ref(),
            conj,
            rhs,
            par,
            stack,
        );
    }

    #[track_caller]
    fn solve_transpose_in_place_with_conj(&self, conj: Conj, rhs: MatMut<'_, T>) {
        let par = get_global_parallelism();

        let mut mem = GlobalMemBuffer::new(
            linalg::cholesky::ldlt::solve::solve_in_place_scratch::<T>(
                self.L.nrows(),
                rhs.ncols(),
                par,
            )
            .unwrap(),
        );
        let stack = DynStack::new(&mut mem);

        linalg::cholesky::ldlt::solve::solve_in_place_with_conj(
            self.L(),
            self.D(),
            conj.compose(Conj::Yes),
            rhs,
            par,
            stack,
        );
    }
}

impl<T: ComplexField> DenseSolveCore<T> for Ldlt<T> {
    #[track_caller]
    fn reconstruct(&self) -> Mat<T> {
        let par = get_global_parallelism();

        let n = self.L.nrows();
        let mut out = Mat::zeros(n, n);

        let mut mem = GlobalMemBuffer::new(
            linalg::cholesky::ldlt::reconstruct::reconstruct_scratch::<T>(n, par).unwrap(),
        );
        let stack = DynStack::new(&mut mem);

        linalg::cholesky::ldlt::reconstruct::reconstruct(
            out.as_mut(),
            self.L(),
            self.D(),
            par,
            stack,
        );

        make_self_adjoint(out.as_mut());
        out
    }

    #[track_caller]
    fn inverse(&self) -> Mat<T> {
        let par = get_global_parallelism();

        let n = self.L.nrows();
        let mut out = Mat::zeros(n, n);

        let mut mem = GlobalMemBuffer::new(
            linalg::cholesky::ldlt::inverse::inverse_scratch::<T>(n, par).unwrap(),
        );
        let stack = DynStack::new(&mut mem);

        linalg::cholesky::ldlt::inverse::inverse(out.as_mut(), self.L(), self.D(), par, stack);

        make_self_adjoint(out.as_mut());
        out
    }
}

impl<T: ComplexField> SolveCore<T> for Lblt<T> {
    #[track_caller]
    fn solve_in_place_with_conj(&self, conj: Conj, rhs: MatMut<'_, T>) {
        let par = get_global_parallelism();

        let mut mem = GlobalMemBuffer::new(
            linalg::cholesky::bunch_kaufman::solve::solve_in_place_scratch::<usize, T>(
                self.L.nrows(),
                rhs.ncols(),
                par,
            )
            .unwrap(),
        );
        let stack = DynStack::new(&mut mem);

        linalg::cholesky::bunch_kaufman::solve::solve_in_place_with_conj(
            self.L.as_ref(),
            self.B_diag(),
            self.B_subdiag(),
            conj,
            self.P(),
            rhs,
            par,
            stack,
        );
    }

    #[track_caller]
    fn solve_transpose_in_place_with_conj(&self, conj: Conj, rhs: MatMut<'_, T>) {
        let par = get_global_parallelism();

        let mut mem = GlobalMemBuffer::new(
            linalg::cholesky::bunch_kaufman::solve::solve_in_place_scratch::<usize, T>(
                self.L.nrows(),
                rhs.ncols(),
                par,
            )
            .unwrap(),
        );
        let stack = DynStack::new(&mut mem);

        linalg::cholesky::bunch_kaufman::solve::solve_in_place_with_conj(
            self.L(),
            self.B_diag(),
            self.B_subdiag(),
            conj.compose(Conj::Yes),
            self.P(),
            rhs,
            par,
            stack,
        );
    }
}

impl<T: ComplexField> DenseSolveCore<T> for Lblt<T> {
    #[track_caller]
    fn reconstruct(&self) -> Mat<T> {
        let par = get_global_parallelism();

        let n = self.L.nrows();
        let mut out = Mat::zeros(n, n);

        let mut mem = GlobalMemBuffer::new(
            linalg::cholesky::bunch_kaufman::reconstruct::reconstruct_scratch::<usize, T>(n, par)
                .unwrap(),
        );
        let stack = DynStack::new(&mut mem);

        linalg::cholesky::bunch_kaufman::reconstruct::reconstruct(
            out.as_mut(),
            self.L(),
            self.B_diag(),
            self.B_subdiag(),
            self.P(),
            par,
            stack,
        );

        make_self_adjoint(out.as_mut());
        out
    }

    #[track_caller]
    fn inverse(&self) -> Mat<T> {
        let par = get_global_parallelism();

        let n = self.L.nrows();
        let mut out = Mat::zeros(n, n);

        let mut mem = GlobalMemBuffer::new(
            linalg::cholesky::bunch_kaufman::inverse::inverse_scratch::<usize, T>(n, par).unwrap(),
        );
        let stack = DynStack::new(&mut mem);

        linalg::cholesky::bunch_kaufman::inverse::inverse(
            out.as_mut(),
            self.L(),
            self.B_diag(),
            self.B_subdiag(),
            self.P(),
            par,
            stack,
        );

        make_self_adjoint(out.as_mut());
        out
    }
}

impl<T: ComplexField> SolveCore<T> for PartialPivLu<T> {
    #[track_caller]
    fn solve_in_place_with_conj(&self, conj: Conj, rhs: MatMut<'_, T>) {
        let par = get_global_parallelism();

        assert!(all(
            self.nrows() == self.ncols(),
            self.nrows() == rhs.nrows(),
        ));

        let k = rhs.ncols();

        linalg::lu::partial_pivoting::solve::solve_in_place_with_conj(
            self.L(),
            self.U(),
            self.P(),
            conj,
            rhs,
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::lu::partial_pivoting::solve::solve_in_place_scratch::<usize, T>(
                    self.nrows(),
                    k,
                    par,
                )
                .unwrap(),
            )),
        );
    }

    #[track_caller]
    fn solve_transpose_in_place_with_conj(&self, conj: Conj, rhs: MatMut<'_, T>) {
        let par = get_global_parallelism();

        assert!(all(
            self.nrows() == self.ncols(),
            self.ncols() == rhs.nrows(),
        ));

        let k = rhs.ncols();

        linalg::lu::partial_pivoting::solve::solve_transpose_in_place_with_conj(
            self.L(),
            self.U(),
            self.P(),
            conj,
            rhs,
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::lu::partial_pivoting::solve::solve_transpose_in_place_scratch::<usize, T>(
                    self.nrows(),
                    k,
                    par,
                )
                .unwrap(),
            )),
        );
    }
}

impl<T: ComplexField> DenseSolveCore<T> for PartialPivLu<T> {
    fn reconstruct(&self) -> Mat<T> {
        let par = get_global_parallelism();
        let m = self.nrows();
        let n = self.ncols();

        let mut out = Mat::zeros(m, n);

        linalg::lu::partial_pivoting::reconstruct::reconstruct(
            out.as_mut(),
            self.L(),
            self.U(),
            self.P(),
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::lu::partial_pivoting::reconstruct::reconstruct_scratch::<usize, T>(
                    m, n, par,
                )
                .unwrap(),
            )),
        );

        out
    }

    #[track_caller]
    fn inverse(&self) -> Mat<T> {
        let par = get_global_parallelism();

        assert!(self.nrows() == self.ncols());

        let n = self.ncols();

        let mut out = Mat::zeros(n, n);

        linalg::lu::partial_pivoting::inverse::inverse(
            out.as_mut(),
            self.L(),
            self.U(),
            self.P(),
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::lu::partial_pivoting::inverse::inverse_scratch::<usize, T>(n, par).unwrap(),
            )),
        );

        out
    }
}

impl<T: ComplexField> SolveCore<T> for FullPivLu<T> {
    #[track_caller]
    fn solve_in_place_with_conj(&self, conj: Conj, rhs: MatMut<'_, T>) {
        let par = get_global_parallelism();

        assert!(all(
            self.nrows() == self.ncols(),
            self.nrows() == rhs.nrows(),
        ));

        let k = rhs.ncols();

        linalg::lu::full_pivoting::solve::solve_in_place_with_conj(
            self.L(),
            self.U(),
            self.P(),
            self.Q(),
            conj,
            rhs,
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::lu::full_pivoting::solve::solve_in_place_scratch::<usize, T>(
                    self.nrows(),
                    k,
                    par,
                )
                .unwrap(),
            )),
        );
    }

    #[track_caller]
    fn solve_transpose_in_place_with_conj(&self, conj: Conj, rhs: MatMut<'_, T>) {
        let par = get_global_parallelism();

        assert!(all(
            self.nrows() == self.ncols(),
            self.ncols() == rhs.nrows(),
        ));

        let k = rhs.ncols();

        linalg::lu::full_pivoting::solve::solve_transpose_in_place_with_conj(
            self.L(),
            self.U(),
            self.P(),
            self.Q(),
            conj,
            rhs,
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::lu::full_pivoting::solve::solve_transpose_in_place_scratch::<usize, T>(
                    self.nrows(),
                    k,
                    par,
                )
                .unwrap(),
            )),
        );
    }
}

impl<T: ComplexField> DenseSolveCore<T> for FullPivLu<T> {
    fn reconstruct(&self) -> Mat<T> {
        let par = get_global_parallelism();
        let m = self.nrows();
        let n = self.ncols();

        let mut out = Mat::zeros(m, n);

        linalg::lu::full_pivoting::reconstruct::reconstruct(
            out.as_mut(),
            self.L(),
            self.U(),
            self.P(),
            self.Q(),
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::lu::full_pivoting::reconstruct::reconstruct_scratch::<usize, T>(m, n, par)
                    .unwrap(),
            )),
        );

        out
    }

    #[track_caller]
    fn inverse(&self) -> Mat<T> {
        let par = get_global_parallelism();

        assert!(self.nrows() == self.ncols());

        let n = self.ncols();

        let mut out = Mat::zeros(n, n);

        linalg::lu::full_pivoting::inverse::inverse(
            out.as_mut(),
            self.L(),
            self.U(),
            self.P(),
            self.Q(),
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::lu::full_pivoting::inverse::inverse_scratch::<usize, T>(n, par).unwrap(),
            )),
        );

        out
    }
}

impl<T: ComplexField> SolveCore<T> for Qr<T> {
    #[track_caller]
    fn solve_in_place_with_conj(&self, conj: Conj, rhs: MatMut<'_, T>) {
        let par = get_global_parallelism();

        assert!(all(
            self.nrows() == self.ncols(),
            self.nrows() == rhs.nrows(),
        ));

        let n = self.nrows();
        let blocksize = self.Q_coeff().nrows();
        let k = rhs.ncols();

        linalg::qr::no_pivoting::solve::solve_in_place_with_conj(
            self.Q_basis(),
            self.Q_coeff(),
            self.R(),
            conj,
            rhs,
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::qr::no_pivoting::solve::solve_in_place_scratch::<T>(n, blocksize, k, par)
                    .unwrap(),
            )),
        );
    }

    #[track_caller]
    fn solve_transpose_in_place_with_conj(&self, conj: Conj, rhs: MatMut<'_, T>) {
        let par = get_global_parallelism();

        assert!(all(
            self.nrows() == self.ncols(),
            self.ncols() == rhs.nrows(),
        ));

        let n = self.nrows();
        let blocksize = self.Q_coeff().nrows();
        let k = rhs.ncols();

        linalg::qr::no_pivoting::solve::solve_transpose_in_place_with_conj(
            self.Q_basis(),
            self.Q_coeff(),
            self.R(),
            conj,
            rhs,
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::qr::no_pivoting::solve::solve_transpose_in_place_scratch::<T>(
                    n, blocksize, k, par,
                )
                .unwrap(),
            )),
        );
    }
}

impl<T: ComplexField> SolveLstsqCore<T> for Qr<T> {
    #[track_caller]
    fn solve_lstsq_in_place_with_conj(&self, conj: Conj, rhs: MatMut<'_, T>) {
        let par = get_global_parallelism();

        assert!(all(
            self.nrows() == rhs.nrows(),
            self.nrows() >= self.ncols(),
        ));

        let m = self.nrows();
        let n = self.ncols();
        let blocksize = self.Q_coeff().nrows();
        let k = rhs.ncols();

        linalg::qr::no_pivoting::solve::solve_lstsq_in_place_with_conj(
            self.Q_basis(),
            self.Q_coeff(),
            self.R(),
            conj,
            rhs,
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::qr::no_pivoting::solve::solve_lstsq_in_place_scratch::<T>(
                    m, n, blocksize, k, par,
                )
                .unwrap(),
            )),
        );
    }
}

impl<T: ComplexField> DenseSolveCore<T> for Qr<T> {
    fn reconstruct(&self) -> Mat<T> {
        let par = get_global_parallelism();
        let m = self.nrows();
        let n = self.ncols();
        let blocksize = self.Q_coeff().nrows();

        let mut out = Mat::zeros(m, n);

        linalg::qr::no_pivoting::reconstruct::reconstruct(
            out.as_mut(),
            self.Q_basis(),
            self.Q_coeff(),
            self.R(),
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::qr::no_pivoting::reconstruct::reconstruct_scratch::<T>(
                    m, n, blocksize, par,
                )
                .unwrap(),
            )),
        );

        out
    }

    fn inverse(&self) -> Mat<T> {
        let par = get_global_parallelism();
        assert!(self.nrows() == self.ncols());

        let n = self.ncols();
        let blocksize = self.Q_coeff().nrows();

        let mut out = Mat::zeros(n, n);

        linalg::qr::no_pivoting::inverse::inverse(
            out.as_mut(),
            self.Q_basis(),
            self.Q_coeff(),
            self.R(),
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::qr::no_pivoting::inverse::inverse_scratch::<T>(n, blocksize, par).unwrap(),
            )),
        );

        out
    }
}

impl<T: ComplexField> SolveCore<T> for ColPivQr<T> {
    #[track_caller]
    fn solve_in_place_with_conj(&self, conj: Conj, rhs: MatMut<'_, T>) {
        let par = get_global_parallelism();

        assert!(all(
            self.nrows() == self.ncols(),
            self.nrows() == rhs.nrows(),
        ));

        let n = self.nrows();
        let blocksize = self.Q_coeff().nrows();
        let k = rhs.ncols();

        linalg::qr::col_pivoting::solve::solve_in_place_with_conj(
            self.Q_basis(),
            self.Q_coeff(),
            self.R(),
            self.P(),
            conj,
            rhs,
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::qr::col_pivoting::solve::solve_in_place_scratch::<usize, T>(
                    n, blocksize, k, par,
                )
                .unwrap(),
            )),
        );
    }

    #[track_caller]
    fn solve_transpose_in_place_with_conj(&self, conj: Conj, rhs: MatMut<'_, T>) {
        let par = get_global_parallelism();

        assert!(all(
            self.nrows() == self.ncols(),
            self.ncols() == rhs.nrows(),
        ));

        let n = self.nrows();
        let blocksize = self.Q_coeff().nrows();
        let k = rhs.ncols();

        linalg::qr::col_pivoting::solve::solve_transpose_in_place_with_conj(
            self.Q_basis(),
            self.Q_coeff(),
            self.R(),
            self.P(),
            conj,
            rhs,
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::qr::col_pivoting::solve::solve_transpose_in_place_scratch::<usize, T>(
                    n, blocksize, k, par,
                )
                .unwrap(),
            )),
        );
    }
}

impl<T: ComplexField> SolveLstsqCore<T> for ColPivQr<T> {
    #[track_caller]
    fn solve_lstsq_in_place_with_conj(&self, conj: Conj, rhs: MatMut<'_, T>) {
        let par = get_global_parallelism();

        assert!(all(
            self.nrows() == rhs.nrows(),
            self.nrows() >= self.ncols(),
        ));

        let m = self.nrows();
        let n = self.ncols();
        let blocksize = self.Q_coeff().nrows();
        let k = rhs.ncols();

        linalg::qr::col_pivoting::solve::solve_lstsq_in_place_with_conj(
            self.Q_basis(),
            self.Q_coeff(),
            self.R(),
            self.P(),
            conj,
            rhs,
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::qr::col_pivoting::solve::solve_lstsq_in_place_scratch::<usize, T>(
                    m, n, blocksize, k, par,
                )
                .unwrap(),
            )),
        );
    }
}

impl<T: ComplexField> DenseSolveCore<T> for ColPivQr<T> {
    fn reconstruct(&self) -> Mat<T> {
        let par = get_global_parallelism();
        let m = self.nrows();
        let n = self.ncols();
        let blocksize = self.Q_coeff().nrows();

        let mut out = Mat::zeros(m, n);

        linalg::qr::col_pivoting::reconstruct::reconstruct(
            out.as_mut(),
            self.Q_basis(),
            self.Q_coeff(),
            self.R(),
            self.P(),
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::qr::col_pivoting::reconstruct::reconstruct_scratch::<usize, T>(
                    m, n, blocksize, par,
                )
                .unwrap(),
            )),
        );

        out
    }

    fn inverse(&self) -> Mat<T> {
        let par = get_global_parallelism();
        assert!(self.nrows() == self.ncols());

        let n = self.ncols();
        let blocksize = self.Q_coeff().nrows();

        let mut out = Mat::zeros(n, n);

        linalg::qr::col_pivoting::inverse::inverse(
            out.as_mut(),
            self.Q_basis(),
            self.Q_coeff(),
            self.R(),
            self.P(),
            par,
            DynStack::new(&mut GlobalMemBuffer::new(
                linalg::qr::col_pivoting::inverse::inverse_scratch::<usize, T>(n, blocksize, par)
                    .unwrap(),
            )),
        );

        out
    }
}

impl<T: ComplexField> SolveCore<T> for Svd<T> {
    #[track_caller]
    fn solve_in_place_with_conj(&self, conj: Conj, rhs: MatMut<'_, T>) {
        let par = get_global_parallelism();

        assert!(all(
            self.nrows() == self.ncols(),
            self.nrows() == rhs.nrows(),
        ));

        let mut rhs = rhs;
        let n = self.nrows();
        let k = rhs.ncols();
        let mut tmp = Mat::zeros(n, k);

        linalg::matmul::matmul_with_conj(
            tmp.as_mut(),
            Accum::Replace,
            self.U().transpose(),
            conj.compose(Conj::Yes),
            rhs.as_ref(),
            Conj::No,
            one(),
            par,
        );

        for j in 0..k {
            for i in 0..n {
                let s = recip(&real(&self.S()[i]));
                tmp[(i, j)] = mul_real(&tmp[(i, j)], &s);
            }
        }

        linalg::matmul::matmul_with_conj(
            rhs.as_mut(),
            Accum::Replace,
            self.V(),
            conj,
            tmp.as_ref(),
            Conj::No,
            one(),
            par,
        );
    }

    #[track_caller]
    fn solve_transpose_in_place_with_conj(&self, conj: Conj, rhs: MatMut<'_, T>) {
        let par = get_global_parallelism();

        assert!(all(
            self.nrows() == self.ncols(),
            self.ncols() == rhs.nrows(),
        ));

        let mut rhs = rhs;
        let n = self.nrows();
        let k = rhs.ncols();
        let mut tmp = Mat::zeros(n, k);

        linalg::matmul::matmul_with_conj(
            tmp.as_mut(),
            Accum::Replace,
            self.V().transpose(),
            conj,
            rhs.as_ref(),
            Conj::No,
            one(),
            par,
        );

        for j in 0..k {
            for i in 0..n {
                let s = recip(&real(&self.S()[i]));
                tmp[(i, j)] = mul_real(&tmp[(i, j)], &s);
            }
        }

        linalg::matmul::matmul_with_conj(
            rhs.as_mut(),
            Accum::Replace,
            self.U(),
            conj.compose(Conj::Yes),
            tmp.as_ref(),
            Conj::No,
            one(),
            par,
        );
    }
}

impl<T: ComplexField> SolveLstsqCore<T> for Svd<T> {
    #[track_caller]
    fn solve_lstsq_in_place_with_conj(&self, conj: Conj, rhs: MatMut<'_, T>) {
        let par = get_global_parallelism();

        assert!(all(
            self.nrows() == rhs.nrows(),
            self.nrows() >= self.ncols(),
        ));

        let m = self.nrows();
        let n = self.ncols();

        let size = Ord::min(m, n);

        let U = self.U().get(.., ..size);
        let V = self.V().get(.., ..size);

        let k = rhs.ncols();

        let mut rhs = rhs;
        let mut tmp = Mat::zeros(size, k);

        linalg::matmul::matmul_with_conj(
            tmp.as_mut(),
            Accum::Replace,
            U.transpose(),
            conj.compose(Conj::Yes),
            rhs.as_ref(),
            Conj::No,
            one(),
            par,
        );

        for j in 0..k {
            for i in 0..size {
                let s = recip(&real(&self.S()[i]));
                tmp[(i, j)] = mul_real(&tmp[(i, j)], &s);
            }
        }

        linalg::matmul::matmul_with_conj(
            rhs.as_mut(),
            Accum::Replace,
            V,
            conj,
            tmp.as_ref(),
            Conj::No,
            one(),
            par,
        );
    }
}

impl<T: ComplexField> DenseSolveCore<T> for Svd<T> {
    fn reconstruct(&self) -> Mat<T> {
        let par = get_global_parallelism();
        let m = self.nrows();
        let n = self.ncols();

        let size = Ord::min(m, n);

        let U = self.U().get(.., ..size);
        let V = self.V().get(.., ..size);
        let S = self.S();

        let mut UxS = Mat::zeros(m, size);
        for j in 0..size {
            let s = real(&S[j]);
            for i in 0..m {
                UxS[(i, j)] = mul_real(&U[(i, j)], &s);
            }
        }

        let mut out = Mat::zeros(m, n);

        linalg::matmul::matmul(
            out.as_mut(),
            Accum::Replace,
            UxS.as_ref(),
            V.adjoint(),
            one(),
            par,
        );

        out
    }

    #[track_caller]
    fn inverse(&self) -> Mat<T> {
        let par = get_global_parallelism();

        assert!(self.nrows() == self.ncols());
        let n = self.nrows();

        let U = self.U();
        let V = self.V();
        let S = self.S();

        let mut VxS = Mat::zeros(n, n);
        for j in 0..n {
            let s = recip(&real(&S[j]));

            for i in 0..n {
                VxS[(i, j)] = mul_real(&V[(i, j)], &s);
            }
        }

        let mut out = Mat::zeros(n, n);

        linalg::matmul::matmul(
            out.as_mut(),
            Accum::Replace,
            VxS.as_ref(),
            U.adjoint(),
            one(),
            par,
        );

        out
    }
}

impl<T: ComplexField> SolveCore<T> for SelfAdjointEigen<T> {
    #[track_caller]
    fn solve_in_place_with_conj(&self, conj: Conj, rhs: MatMut<'_, T>) {
        let par = get_global_parallelism();

        assert!(all(
            self.nrows() == self.ncols(),
            self.nrows() == rhs.nrows(),
        ));

        let mut rhs = rhs;
        let n = self.nrows();
        let k = rhs.ncols();
        let mut tmp = Mat::zeros(n, k);

        linalg::matmul::matmul_with_conj(
            tmp.as_mut(),
            Accum::Replace,
            self.U().transpose(),
            conj.compose(Conj::Yes),
            rhs.as_ref(),
            Conj::No,
            one(),
            par,
        );

        for j in 0..k {
            for i in 0..n {
                let s = recip(&real(&self.S()[i]));
                tmp[(i, j)] = mul_real(&tmp[(i, j)], &s);
            }
        }

        linalg::matmul::matmul_with_conj(
            rhs.as_mut(),
            Accum::Replace,
            self.U(),
            conj,
            tmp.as_ref(),
            Conj::No,
            one(),
            par,
        );
    }

    #[track_caller]
    fn solve_transpose_in_place_with_conj(&self, conj: Conj, rhs: MatMut<'_, T>) {
        let par = get_global_parallelism();

        assert!(all(
            self.nrows() == self.ncols(),
            self.ncols() == rhs.nrows(),
        ));

        let mut rhs = rhs;
        let n = self.nrows();
        let k = rhs.ncols();
        let mut tmp = Mat::zeros(n, k);

        linalg::matmul::matmul_with_conj(
            tmp.as_mut(),
            Accum::Replace,
            self.U().transpose(),
            conj,
            rhs.as_ref(),
            Conj::No,
            one(),
            par,
        );

        for j in 0..k {
            for i in 0..n {
                let s = recip(&real(&self.S()[i]));
                tmp[(i, j)] = mul_real(&tmp[(i, j)], &s);
            }
        }

        linalg::matmul::matmul_with_conj(
            rhs.as_mut(),
            Accum::Replace,
            self.U(),
            conj.compose(Conj::Yes),
            tmp.as_ref(),
            Conj::No,
            one(),
            par,
        );
    }
}

impl<T: ComplexField> DenseSolveCore<T> for SelfAdjointEigen<T> {
    fn reconstruct(&self) -> Mat<T> {
        let par = get_global_parallelism();
        let m = self.nrows();
        let n = self.ncols();

        let size = Ord::min(m, n);

        let U = self.U().get(.., ..size);
        let V = self.U().get(.., ..size);
        let S = self.S();

        let mut UxS = Mat::zeros(m, size);
        for j in 0..size {
            let s = real(&S[j]);
            for i in 0..m {
                UxS[(i, j)] = mul_real(&U[(i, j)], &s);
            }
        }

        let mut out = Mat::zeros(m, n);

        linalg::matmul::matmul(
            out.as_mut(),
            Accum::Replace,
            UxS.as_ref(),
            V.adjoint(),
            one(),
            par,
        );

        out
    }

    fn inverse(&self) -> Mat<T> {
        let par = get_global_parallelism();

        assert!(self.nrows() == self.ncols());
        let n = self.nrows();

        let U = self.U();
        let V = self.U();
        let S = self.S();

        let mut VxS = Mat::zeros(n, n);
        for j in 0..n {
            let s = recip(&real(&S[j]));

            for i in 0..n {
                VxS[(i, j)] = mul_real(&V[(i, j)], &s);
            }
        }

        let mut out = Mat::zeros(n, n);

        linalg::matmul::matmul(
            out.as_mut(),
            Accum::Replace,
            VxS.as_ref(),
            U.adjoint(),
            one(),
            par,
        );

        out
    }
}

#[cfg(test)]
mod tests {}
