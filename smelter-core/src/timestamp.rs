use std::{
    fmt,
    ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign},
    time::{Duration, Instant},
};

const NANOS_PER_SEC: i64 = 1_000_000_000;
const NANOS_PER_MILLI: i64 = 1_000_000;
const NANOS_PER_MICRO: i64 = 1_000;

/// Signed counterpart of [`Duration`] with nanosecond precision. Intended for PTS/DTS
/// values that can be negative.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp {
    nanos: i64,
}

impl Timestamp {
    pub const ZERO: Timestamp = Timestamp { nanos: 0 };
    pub const MIN: Timestamp = Timestamp { nanos: i64::MIN };
    pub const MAX: Timestamp = Timestamp { nanos: i64::MAX };

    pub const fn from_secs(secs: i64) -> Self {
        Timestamp {
            nanos: secs * NANOS_PER_SEC,
        }
    }

    pub const fn from_millis(millis: i64) -> Self {
        Timestamp {
            nanos: millis * NANOS_PER_MILLI,
        }
    }

    pub const fn from_micros(micros: i64) -> Self {
        Timestamp {
            nanos: micros * NANOS_PER_MICRO,
        }
    }

    pub const fn from_nanos(nanos: i64) -> Self {
        Timestamp { nanos }
    }

    pub fn from_secs_f64(secs: f64) -> Self {
        Timestamp {
            nanos: (secs * NANOS_PER_SEC as f64).round() as i64,
        }
    }

    pub fn from_secs_f32(secs: f32) -> Self {
        Self::from_secs_f64(secs as f64)
    }

    /// Saturates at [`Timestamp::MAX`] if the duration does not fit.
    pub fn from_duration(duration: Duration) -> Self {
        Timestamp {
            nanos: i64::try_from(duration.as_nanos()).unwrap_or(i64::MAX),
        }
    }

    /// Time elapsed since `start`; the current position on a timeline that
    /// starts at `start` (e.g. the queue sync point).
    pub fn since(start: Instant) -> Self {
        Instant::now().timestamp_since(start)
    }

    /// Returns `None` if the timestamp is negative.
    pub fn to_duration(self) -> Option<Duration> {
        u64::try_from(self.nanos).ok().map(Duration::from_nanos)
    }

    /// Negative values are clamped to [`Duration::ZERO`].
    pub fn to_duration_saturating(self) -> Duration {
        Duration::from_nanos(u64::try_from(self.nanos).unwrap_or(0))
    }

    pub const fn as_secs(self) -> i64 {
        self.nanos / NANOS_PER_SEC
    }

    pub const fn as_millis(self) -> i64 {
        self.nanos / NANOS_PER_MILLI
    }

    pub const fn as_micros(self) -> i64 {
        self.nanos / NANOS_PER_MICRO
    }

    pub const fn as_nanos(self) -> i64 {
        self.nanos
    }

    pub fn as_secs_f64(self) -> f64 {
        self.nanos as f64 / NANOS_PER_SEC as f64
    }

    pub fn as_secs_f32(self) -> f32 {
        self.as_secs_f64() as f32
    }

    pub const fn is_zero(self) -> bool {
        self.nanos == 0
    }

    pub const fn is_negative(self) -> bool {
        self.nanos < 0
    }

    pub const fn is_positive(self) -> bool {
        self.nanos > 0
    }

    pub const fn abs(self) -> Self {
        Timestamp {
            nanos: self.nanos.abs(),
        }
    }

    /// Absolute value as [`Duration`].
    pub fn abs_duration(self) -> Duration {
        Duration::from_nanos(self.nanos.unsigned_abs())
    }

    pub const fn checked_add(self, rhs: Timestamp) -> Option<Self> {
        match self.nanos.checked_add(rhs.nanos) {
            Some(nanos) => Some(Timestamp { nanos }),
            None => None,
        }
    }

    pub const fn checked_sub(self, rhs: Timestamp) -> Option<Self> {
        match self.nanos.checked_sub(rhs.nanos) {
            Some(nanos) => Some(Timestamp { nanos }),
            None => None,
        }
    }

    pub const fn saturating_add(self, rhs: Timestamp) -> Self {
        Timestamp {
            nanos: self.nanos.saturating_add(rhs.nanos),
        }
    }

    pub const fn saturating_sub(self, rhs: Timestamp) -> Self {
        Timestamp {
            nanos: self.nanos.saturating_sub(rhs.nanos),
        }
    }

    pub fn mul_f64(self, rhs: f64) -> Self {
        Self::from_secs_f64(self.as_secs_f64() * rhs)
    }

    pub fn div_f64(self, rhs: f64) -> Self {
        Self::from_secs_f64(self.as_secs_f64() / rhs)
    }

    pub fn min(a: Timestamp, b: Timestamp) -> Timestamp {
        Ord::min(a, b)
    }

    pub fn max(a: Timestamp, b: Timestamp) -> Timestamp {
        Ord::max(a, b)
    }
}

impl From<Duration> for Timestamp {
    fn from(duration: Duration) -> Self {
        Self::from_duration(duration)
    }
}

/// Reading an [`Instant`] as a [`Timestamp`].
pub trait InstantExt {
    /// Position of this instant on a timeline that starts at `since`;
    /// negative when the instant is before `since`.
    fn timestamp_since(&self, since: Instant) -> Timestamp;
}

impl InstantExt for Instant {
    fn timestamp_since(&self, since: Instant) -> Timestamp {
        match self.checked_duration_since(since) {
            Some(elapsed) => Timestamp::from_duration(elapsed),
            None => -Timestamp::from_duration(since.duration_since(*self)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NegativeTimestampError(pub Timestamp);

impl fmt::Display for NegativeTimestampError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "cannot convert negative timestamp {:?} to Duration",
            self.0
        )
    }
}

impl std::error::Error for NegativeTimestampError {}

impl TryFrom<Timestamp> for Duration {
    type Error = NegativeTimestampError;

    fn try_from(timestamp: Timestamp) -> Result<Self, Self::Error> {
        timestamp
            .to_duration()
            .ok_or(NegativeTimestampError(timestamp))
    }
}

impl fmt::Debug for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.nanos < 0 {
            write!(f, "-")?;
        }
        fmt::Debug::fmt(&self.abs_duration(), f)
    }
}

impl Add for Timestamp {
    type Output = Timestamp;

    fn add(self, rhs: Timestamp) -> Timestamp {
        Timestamp {
            nanos: self.nanos + rhs.nanos,
        }
    }
}

impl Add<Duration> for Timestamp {
    type Output = Timestamp;

    fn add(self, rhs: Duration) -> Timestamp {
        self + Timestamp::from(rhs)
    }
}

impl Sub for Timestamp {
    type Output = Timestamp;

    fn sub(self, rhs: Timestamp) -> Timestamp {
        Timestamp {
            nanos: self.nanos - rhs.nanos,
        }
    }
}

impl Sub<Duration> for Timestamp {
    type Output = Timestamp;

    fn sub(self, rhs: Duration) -> Timestamp {
        self - Timestamp::from(rhs)
    }
}

impl AddAssign for Timestamp {
    fn add_assign(&mut self, rhs: Timestamp) {
        *self = *self + rhs;
    }
}

impl AddAssign<Duration> for Timestamp {
    fn add_assign(&mut self, rhs: Duration) {
        *self = *self + rhs;
    }
}

impl SubAssign for Timestamp {
    fn sub_assign(&mut self, rhs: Timestamp) {
        *self = *self - rhs;
    }
}

impl SubAssign<Duration> for Timestamp {
    fn sub_assign(&mut self, rhs: Duration) {
        *self = *self - rhs;
    }
}

impl Neg for Timestamp {
    type Output = Timestamp;

    fn neg(self) -> Timestamp {
        Timestamp { nanos: -self.nanos }
    }
}

impl<T: Into<i64>> Mul<T> for Timestamp {
    type Output = Timestamp;

    fn mul(self, rhs: T) -> Timestamp {
        Timestamp {
            nanos: self.nanos * rhs.into(),
        }
    }
}

impl Div<i64> for Timestamp {
    type Output = Timestamp;

    fn div(self, rhs: i64) -> Timestamp {
        Timestamp {
            nanos: self.nanos / rhs,
        }
    }
}

impl Div<u32> for Timestamp {
    type Output = Timestamp;

    fn div(self, rhs: u32) -> Timestamp {
        self / rhs as i64
    }
}

impl Add<Timestamp> for Instant {
    type Output = Instant;

    /// Instant `rhs` after `self`; before it when `rhs` is negative.
    fn add(self, rhs: Timestamp) -> Instant {
        match rhs.is_negative() {
            false => self + rhs.abs_duration(),
            true => self - rhs.abs_duration(),
        }
    }
}

impl Sub<Timestamp> for Instant {
    type Output = Instant;

    fn sub(self, rhs: Timestamp) -> Instant {
        self + (-rhs)
    }
}

impl std::iter::Sum for Timestamp {
    fn sum<I: Iterator<Item = Timestamp>>(iter: I) -> Timestamp {
        iter.fold(Timestamp::ZERO, Add::add)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_roundtrip() {
        let duration = Duration::from_nanos(1_500_000_777);
        let ts = Timestamp::from(duration);
        assert_eq!(Duration::try_from(ts), Ok(duration));
        assert_eq!(Timestamp::from(Duration::MAX), Timestamp::MAX);
    }

    #[test]
    fn negative() {
        let ts = Timestamp::from_millis(500) - Duration::from_secs(2);
        assert_eq!(ts, Timestamp::from_millis(-1500));
        assert!(ts.is_negative());
        assert_eq!(ts.to_duration(), None);
        assert_eq!(ts.to_duration_saturating(), Duration::ZERO);
        assert_eq!(ts.abs_duration(), Duration::from_millis(1500));
        assert_eq!(-ts, Timestamp::from_millis(1500));
        assert_eq!(ts.as_secs_f64(), -1.5);
        assert_eq!(format!("{ts:?}"), "-1.5s");
    }

    #[test]
    fn ordering() {
        assert!(Timestamp::from_secs(-1) < Timestamp::ZERO);
        assert!(Timestamp::ZERO < Timestamp::from_nanos(1));
        assert_eq!(
            Timestamp::min(Timestamp::from_secs(-3), Timestamp::from_secs(2)),
            Timestamp::from_secs(-3)
        );
    }
}
