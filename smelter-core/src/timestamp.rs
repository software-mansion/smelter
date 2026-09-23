use std::{
    fmt,
    ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign},
    time::{Duration, Instant},
};

use tracing::trace;

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

    /// Microseconds as u64, negative values saturate to 0.
    pub const fn as_micros_saturating(self) -> u64 {
        if self.nanos < 0 {
            0
        } else {
            (self.nanos / NANOS_PER_MICRO) as u64
        }
    }

    /// Nanoseconds as u64, negative values saturate to 0.
    pub const fn as_nanos_saturating(self) -> u64 {
        if self.nanos < 0 { 0 } else { self.nanos as u64 }
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

    pub fn clamp(value: Timestamp, min: Timestamp, max: Timestamp) -> Timestamp {
        Ord::clamp(value, min, max)
    }

    /// Offset that presents content at `input_pts` at `output_pts`; every other timestamp
    /// keeps its distance to that pair.
    pub(crate) fn offset(input_pts: Timestamp, output_pts: Timestamp) -> TimestampOffset {
        TimestampOffset(output_pts - input_pts)
    }
}

impl From<Duration> for Timestamp {
    fn from(duration: Duration) -> Self {
        Self::from_duration(duration)
    }
}

/// Reading an [`Instant`] as the start of a [`Timestamp`] timeline.
pub trait InstantExt {
    /// Position of `until` on a timeline that starts at this instant;
    /// negative when `until` is before it.
    fn timestamp_at(&self, until: Instant) -> Timestamp;

    /// Current position on a timeline that starts at this instant.
    fn timestamp_now(&self) -> Timestamp;
}

impl InstantExt for Instant {
    fn timestamp_at(&self, until: Instant) -> Timestamp {
        match until.checked_duration_since(*self) {
            Some(elapsed) => Timestamp::from_duration(elapsed),
            None => -Timestamp::from_duration(self.duration_since(until)),
        }
    }

    fn timestamp_now(&self) -> Timestamp {
        self.timestamp_at(Instant::now())
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

/// Timestamp + Timestamp -> Timestamp
impl Add<Timestamp> for Timestamp {
    type Output = Timestamp;

    fn add(self, rhs: Timestamp) -> Timestamp {
        Timestamp {
            nanos: self.nanos + rhs.nanos,
        }
    }
}

/// Timestamp + Duration -> Timestamp
impl Add<Duration> for Timestamp {
    type Output = Timestamp;

    fn add(self, rhs: Duration) -> Timestamp {
        self + Timestamp::from(rhs)
    }
}

/// Timestamp - Timestamp -> Timestamp
impl Sub<Timestamp> for Timestamp {
    type Output = Timestamp;

    fn sub(self, rhs: Timestamp) -> Timestamp {
        Timestamp {
            nanos: self.nanos - rhs.nanos,
        }
    }
}

/// Timestamp - Duration -> Timestamp
impl Sub<Duration> for Timestamp {
    type Output = Timestamp;

    fn sub(self, rhs: Duration) -> Timestamp {
        self - Timestamp::from(rhs)
    }
}

/// Timestamp += Timestamp
impl AddAssign<Timestamp> for Timestamp {
    fn add_assign(&mut self, rhs: Timestamp) {
        *self = *self + rhs;
    }
}

/// Timestamp += Duration
impl AddAssign<Duration> for Timestamp {
    fn add_assign(&mut self, rhs: Duration) {
        *self = *self + rhs;
    }
}

/// Timestamp -= Timestamp
impl SubAssign<Timestamp> for Timestamp {
    fn sub_assign(&mut self, rhs: Timestamp) {
        *self = *self - rhs;
    }
}

/// Timestamp -= Duration
impl SubAssign<Duration> for Timestamp {
    fn sub_assign(&mut self, rhs: Duration) {
        *self = *self - rhs;
    }
}

/// -Timestamp -> Timestamp
impl Neg for Timestamp {
    type Output = Timestamp;

    fn neg(self) -> Timestamp {
        Timestamp { nanos: -self.nanos }
    }
}

/// Timestamp * impl Into<i64> -> Timestamp
impl<T: Into<i64>> Mul<T> for Timestamp {
    type Output = Timestamp;

    fn mul(self, rhs: T) -> Timestamp {
        Timestamp {
            nanos: self.nanos * rhs.into(),
        }
    }
}

/// Timestamp / i64 -> Timestamp
impl Div<i64> for Timestamp {
    type Output = Timestamp;

    fn div(self, rhs: i64) -> Timestamp {
        Timestamp {
            nanos: self.nanos / rhs,
        }
    }
}

/// Timestamp / u32 -> Timestamp
impl Div<u32> for Timestamp {
    type Output = Timestamp;

    fn div(self, rhs: u32) -> Timestamp {
        self / rhs as i64
    }
}

/// Instant + Timestamp -> Instant
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

/// Instant - Timestamp -> Instant
impl Sub<Timestamp> for Instant {
    type Output = Instant;

    fn sub(self, rhs: Timestamp) -> Instant {
        self + (-rhs)
    }
}

/// Iterator<Item = Timestamp>::sum() -> Timestamp
impl std::iter::Sum for Timestamp {
    fn sum<I: Iterator<Item = Timestamp>>(iter: I) -> Timestamp {
        iter.fold(Timestamp::ZERO, Add::add)
    }
}

/// Mapping between two timelines: the offset that, added to a pts on the source timeline, gives
/// the pts on the destination timeline it is presented at. Usually a track's input and output
/// timelines.
///
/// Ordered by how late the same input pts is presented: `a > b` when `a` holds content back for
/// longer than `b`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct TimestampOffset(Timestamp);

impl TimestampOffset {
    pub(crate) const ZERO: TimestampOffset = TimestampOffset(Timestamp::ZERO);

    pub(crate) fn abs_duration(self) -> Duration {
        self.0.abs_duration()
    }

    pub(crate) fn as_secs_f64(self) -> f64 {
        self.0.as_secs_f64()
    }

    /// Maps a raw timestamp (pts or dts) onto the output timeline.
    pub(crate) fn to_output_pts(self, pts: Timestamp) -> Timestamp {
        pts + self.0
    }

    /// Input pts presented at `output_pts`.
    pub(crate) fn to_input_pts(self, output_pts: Timestamp) -> Timestamp {
        output_pts - self.0
    }

    /// Moves the mapping at most `step` toward `target`, i.e. toward
    /// presenting the same input pts at the same output pts. A no-op once both
    /// describe the same mapping.
    pub(crate) fn nudge_toward(&mut self, target: TimestampOffset, max_step: Timestamp) {
        let offset_to_target = target.0 - self.0;
        if offset_to_target == Timestamp::ZERO {
            return;
        }
        let change = Timestamp::clamp(offset_to_target, -max_step, max_step);
        self.0 += change;
        trace!(?change, anchor=?self.0, "Nudging anchor toward target");
    }
}

/// TimestampOffset + Timestamp -> TimestampOffset
impl Add<Timestamp> for TimestampOffset {
    type Output = TimestampOffset;

    fn add(self, rhs: Timestamp) -> Self::Output {
        TimestampOffset(self.0 + rhs)
    }
}

/// TimestampOffset + Duration -> TimestampOffset
impl Add<Duration> for TimestampOffset {
    type Output = TimestampOffset;

    fn add(self, rhs: Duration) -> Self::Output {
        TimestampOffset(self.0 + rhs)
    }
}

/// TimestampOffset - Timestamp -> TimestampOffset
impl Sub<Timestamp> for TimestampOffset {
    type Output = TimestampOffset;

    fn sub(self, rhs: Timestamp) -> Self::Output {
        TimestampOffset(self.0 - rhs)
    }
}

/// TimestampOffset + TimestampOffset -> TimestampOffset
impl Add<TimestampOffset> for TimestampOffset {
    type Output = TimestampOffset;

    fn add(self, rhs: TimestampOffset) -> Self::Output {
        TimestampOffset(self.0 + rhs.0)
    }
}

/// TimestampOffset - TimestampOffset -> TimestampOffset
impl Sub<TimestampOffset> for TimestampOffset {
    type Output = TimestampOffset;

    fn sub(self, rhs: TimestampOffset) -> Self::Output {
        TimestampOffset(self.0 - rhs.0)
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
