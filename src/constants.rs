//! Constants used in the Orchard protocol.
use ff::{Field, PrimeField};
use group::Curve;
use halo2::{
    arithmetic::{lagrange_interpolate, CurveAffine, FieldExt},
    pasta::pallas,
};

pub mod commit_ivk_r;
pub mod note_commit_r;
pub mod nullifier_k;
pub mod value_commit_r;
pub mod value_commit_v;

pub mod util;

/// $\ell^\mathsf{Orchard}_\mathsf{base}$
pub(crate) const L_ORCHARD_BASE: usize = 255;

// SWU hash-to-curve personalizations

/// SWU hash-to-curve personalization
/// This is used for the spending key base point and the nullifier base point K^Orchard
pub const ORCHARD_PERSONALIZATION: &str = "z.cash:Orchard";

/// SWU hash-to-curve personalization for the group hash for key diversification
pub const KEY_DIVERSIFICATION_PERSONALIZATION: &str = "z.cash:Orchard-gd";

/// SWU hash-to-curve personalization for the value commitment generator
pub const VALUE_COMMITMENT_PERSONALIZATION: &str = "z.cash:Orchard-cv";

/// SWU hash-to-curve personalization for the note commitment generator
pub const NOTE_COMMITMENT_PERSONALIZATION: &str = "z.cash:Orchard-NoteCommit";

/// SWU hash-to-curve personalization for the IVK commitment generator
pub const COMMIT_IVK_PERSONALIZATION: &str = "z.cash:Orchard-CommitIvk";

/// SWU hash-to-curve personalization for the Merkle CRH generator
pub const MERKLE_CRH_PERSONALIZATION: &str = "z.cash:Orchard-MerkleCRH";

/// Window size for fixed-base scalar multiplication
pub const FIXED_BASE_WINDOW_SIZE: usize = 3;

/// Number of windows
pub const NUM_WINDOWS: usize = pallas::Base::NUM_BITS as usize / FIXED_BASE_WINDOW_SIZE;

/// Number of bits used in complete addition (for variable-base scalar mul)
pub const NUM_COMPLETE_BITS: usize = 3;

#[derive(Copy, Clone, Debug)]
pub enum OrchardFixedBases<C: CurveAffine> {
    CommitIvkR(OrchardFixedBase<C>),
    NoteCommitR(OrchardFixedBase<C>),
    NullifierK(OrchardFixedBase<C>),
    ValueCommitR(OrchardFixedBase<C>),
    ValueCommitV(OrchardFixedBase<C>),
}

impl<C: CurveAffine> std::hash::Hash for OrchardFixedBases<C> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match *self {
            OrchardFixedBases::CommitIvkR(_) => {
                state.write(&format!("{:?}", "CommitIvkR").as_bytes())
            }
            OrchardFixedBases::NoteCommitR(_) => {
                state.write(&format!("{:?}", "NoteCommitR").as_bytes())
            }
            OrchardFixedBases::NullifierK(_) => {
                state.write(&format!("{:?}", "NullifierK").as_bytes())
            }
            OrchardFixedBases::ValueCommitR(_) => {
                state.write(&format!("{:?}", "ValueCommitR").as_bytes())
            }
            OrchardFixedBases::ValueCommitV(_) => {
                state.write(&format!("{:?}", "ValueCommitV").as_bytes())
            }
        }
    }
}

impl<C: CurveAffine> OrchardFixedBases<C> {
    pub fn inner(&self) -> OrchardFixedBase<C> {
        match self {
            Self::CommitIvkR(inner) => *inner,
            Self::NoteCommitR(inner) => *inner,
            Self::NullifierK(inner) => *inner,
            Self::ValueCommitR(inner) => *inner,
            Self::ValueCommitV(inner) => *inner,
        }
    }
}

impl<C: CurveAffine> PartialEq for OrchardFixedBases<C> {
    fn eq(&self, other: &Self) -> bool {
        self.inner() == other.inner()
    }
}

impl<C: CurveAffine> Eq for OrchardFixedBases<C> {}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct OrchardFixedBase<C: CurveAffine>(C);

impl<C: CurveAffine> OrchardFixedBase<C> {
    pub fn new(generator: C) -> Self {
        OrchardFixedBase(generator)
    }

    pub fn value(&self) -> C {
        self.0
    }
}

pub trait FixedBase<C: CurveAffine> {
    /// For each fixed base, we calculate its scalar multiples in three-bit windows.
    /// Each window will have 2^3 = 8 points.
    fn compute_window_table(&self) -> Vec<Vec<C>>;

    /// For each window, we interpolate the x-coordinate.
    /// Here, we pre-compute and store the coefficients of the interpolation polynomial.
    fn compute_lagrange_coeffs(&self) -> Vec<Vec<C::Base>>;

    /// For each window, z is a field element
    /// such that for each point (x, y) in the window:
    /// - z + y = u^2 (some square in the field); and
    /// - z - y is not a square.
    fn find_zs(&self) -> Option<Vec<u64>>;
}

impl<C: CurveAffine> FixedBase<C> for OrchardFixedBase<C> {
    fn compute_window_table(&self) -> Vec<Vec<C>> {
        let h: usize = 1 << FIXED_BASE_WINDOW_SIZE;
        let mut window_table: Vec<Vec<C>> = Vec::with_capacity(NUM_WINDOWS);

        // Generate window table entries for all windows but the last.
        // For these first 84 windows, we compute the multiple [(k+1)*(8^w)]B.
        // Here, w ranges from [0..84)
        for w in 0..(NUM_WINDOWS - 1) {
            window_table.push(
                (0..h)
                    .map(|k| {
                        // scalar = (k+1)*(8^w)
                        let scalar = C::ScalarExt::from_u64(k as u64 + 1)
                            * C::ScalarExt::from_u64(h as u64).pow(&[w as u64, 0, 0, 0]);
                        (self.0 * scalar).to_affine()
                    })
                    .collect(),
            );
        }

        // Generate window table entries for the last window, w = 84.
        // For the last window, we compute [k * (8^w) - sum]B, where sum is defined
        // as sum = \sum_{j = 0}^{83} 8^j
        let sum = (0..(NUM_WINDOWS - 1)).fold(C::ScalarExt::zero(), |acc, w| {
            acc + C::ScalarExt::from_u64(h as u64).pow(&[w as u64, 0, 0, 0])
        });
        window_table.push(
            (0..h)
                .map(|k| {
                    // scalar = k * (8^w) - sum, where w = 84
                    let scalar = C::ScalarExt::from_u64(k as u64)
                        * C::ScalarExt::from_u64(h as u64).pow(&[
                            (NUM_WINDOWS - 1) as u64,
                            0,
                            0,
                            0,
                        ])
                        - sum;
                    (self.0 * scalar).to_affine()
                })
                .collect(),
        );

        window_table
    }

    fn compute_lagrange_coeffs(&self) -> Vec<Vec<C::Base>> {
        let h: usize = 1 << FIXED_BASE_WINDOW_SIZE;

        // We are interpolating over the 3-bit window, k \in [0..8)
        let points: Vec<_> = (0..h).map(|i| C::Base::from_u64(i as u64)).collect();

        let window_table = self.compute_window_table();

        window_table
            .iter()
            .map(|window_points| {
                let x_window_points: Vec<_> = window_points
                    .iter()
                    .map(|point| point.get_xy().unwrap().0)
                    .collect();
                lagrange_interpolate(&points, &x_window_points)
            })
            .collect::<Vec<Vec<_>>>()
    }

    /// For each window, z is a field element
    /// such that for each point (x, y) in the window:
    /// - z + y = u^2 (some square in the field); and
    /// - z - y is not a square.
    fn find_zs(&self) -> Option<Vec<u64>> {
        // Closure to find z for one window
        let find_z = |window_points: &[C]| {
            let h: usize = 1 << FIXED_BASE_WINDOW_SIZE;
            assert_eq!(h, window_points.len());

            let ys: Vec<_> = window_points
                .iter()
                .map(|point| point.get_xy().unwrap().1)
                .collect();
            let z_for_single_y = |y: C::Base, z: u64| {
                let sum_y_is_square: bool = (y + C::Base::from_u64(z)).sqrt().is_some().into();
                let sum_neg_y_is_square: bool = (-y + C::Base::from_u64(z)).sqrt().is_some().into();
                (sum_y_is_square && !sum_neg_y_is_square) as usize
            };

            for z in 0..(1000 * (1 << (2 * h))) {
                if ys.iter().map(|y| z_for_single_y(*y, z)).sum::<usize>() == h {
                    return Some(z);
                }
            }

            None
        };

        let window_table = self.compute_window_table();
        window_table
            .iter()
            .map(|window_points| find_z(window_points))
            .collect()
    }
}

pub trait TestFixedBase<C: CurveAffine> {
    fn test_lagrange_coeffs(&self);
    fn test_z(&self, z: &[u64]);
}

impl<C: CurveAffine> TestFixedBase<C> for OrchardFixedBase<C> {
    fn test_lagrange_coeffs(&self) {
        let h = 1 << FIXED_BASE_WINDOW_SIZE;
        let lagrange_coeffs = self.compute_lagrange_coeffs();
        let mut points = Vec::<C::CurveExt>::with_capacity(NUM_WINDOWS);

        let scalar = C::Scalar::rand();
        let bits = util::decompose_scalar_fixed::<C>(
            scalar,
            C::Scalar::NUM_BITS as usize,
            FIXED_BASE_WINDOW_SIZE,
        );

        // Check first 84 windows, i.e. `k_0, k_1, ..., k_83`
        for ((idx, bits), coeffs) in bits[0..(NUM_WINDOWS - 1)]
            .iter()
            .enumerate()
            .zip(lagrange_coeffs[0..(NUM_WINDOWS - 1)].iter())
        {
            let interpolated_x = util::evaluate::<C>(*bits, coeffs);

            // [(k+1)*(8^w)]B
            let point = self.0
                * C::Scalar::from_u64(*bits as u64 + 1)
                * C::Scalar::from_u64(h as u64).pow(&[idx as u64, 0, 0, 0]);
            let x = point.to_affine().get_xy().unwrap().0;

            assert_eq!(x, interpolated_x);
            points.push(point);
        }

        // Check last window
        {
            let last_bits = bits[NUM_WINDOWS - 1];
            let interpolated_x = util::evaluate::<C>(last_bits, &lagrange_coeffs[NUM_WINDOWS - 1]);

            // [k * (8^w) - offset]B, where offset = \sum_{j = 0}^{83} 8^j
            let offset = (0..(NUM_WINDOWS - 1)).fold(C::Scalar::zero(), |acc, w| {
                acc + C::Scalar::from_u64(h as u64).pow(&[w as u64, 0, 0, 0])
            });
            let scalar = C::Scalar::from_u64(last_bits as u64)
                * C::Scalar::from_u64(h as u64).pow(&[(NUM_WINDOWS - 1) as u64, 0, 0, 0])
                - offset;
            let point = self.0 * scalar;
            let x = point.to_affine().get_xy().unwrap().0;

            assert_eq!(x, interpolated_x);
            points.push(point);
        }

        // Check the sum of all the window points
        let window_sum = points
            .iter()
            .fold(C::CurveExt::default(), |acc, point| acc + point);
        let multiple = self.0 * scalar;
        assert_eq!(window_sum, multiple);
    }

    fn test_z(&self, z: &[u64]) {
        let window_table = self.compute_window_table();

        for (z, window_points) in z.iter().zip(window_table) {
            for point in window_points.iter() {
                let y = point.get_xy().unwrap().1;
                assert_eq!((C::Base::from_u64(*z) + y).sqrt().is_some().unwrap_u8(), 1);
                assert_eq!((C::Base::from_u64(*z) - y).sqrt().is_some().unwrap_u8(), 0);
            }
        }
    }
}
