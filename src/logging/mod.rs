//! Logging statistics from simulation runs
mod chunk;
mod chunk_by_counter;
mod chunk_by_time;
mod display;
mod tensorboard;

pub use chunk::ChunkLogger;
pub use chunk_by_counter::ByCounter;
pub use chunk_by_time::ByTime;
pub use display::{DisplayBackend, DisplayLogger};
pub use tensorboard::{TensorBoardBackend, TensorBoardLogger};

use smallvec::SmallVec;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::ops::Drop;
use std::time::{Duration, Instant};
use std::{fmt, iter, slice};
use thiserror::Error;

/// Log a time series of statistics.
///
/// Statistics with the same name may be aggregated or summarized over some time period.
pub trait StatsLogger: Send {
    /// Log a value associated with an ID.
    ///
    /// # Args
    /// * `id` -
    ///     Unique identifier of the statistic to log. Used to track the value over time.
    ///     It is an error to use the same identifier with values that have different
    ///     [`LogValue`] variants or are otherwise structurally incompatible.
    ///
    ///     The ID can be created from a string with `into()`.
    ///     It is a hierarchical name, similar to a file system path.
    ///     Logger scope names (created with [`StatsLogger::with_scope`]) will be prepended to the
    ///     given ID.
    ///
    /// * `value` - The value to log.
    #[inline]
    fn log(&mut self, id: Id, value: LogValue) -> Result<(), LogError> {
        self.group_start();
        let result = self.group_log(id, value);
        self.group_end();
        result
    }

    /// Internal helper. Do not call directly. Handle the creation of a `LogGroup`. May flush.
    fn group_start(&mut self);
    /// Internal helper. Do not call directly. Log a value within a `LogGroup`. May not flush.
    fn group_log(&mut self, id: Id, value: LogValue) -> Result<(), LogError>;
    /// Internal helper. Do not call directly. Handle the drop of a `LogGroup`. May flush.
    fn group_end(&mut self);

    /// Record any remaining data in the logger that has not yet been recorded.
    fn flush(&mut self);

    /// Create a logger for a group of related values.
    ///
    /// Once the group has been created, no flushing will occur until the group is dropped.
    ///
    /// This can be called on a reference for a temporary group: `(&mut logger).group()`
    #[inline]
    fn group(self) -> LogGroup<Self>
    where
        Self: Sized,
    {
        LogGroup::new(self)
    }

    /// Wrap this logger such that an inner scope is added to all logged ids.
    ///
    /// This can be called on a reference for a temporary scope: `(&mut logger).with_scope(...)`
    #[inline]
    fn with_scope(self, scope: &'static str) -> ScopedLogger<Self>
    where
        Self: Sized,
    {
        ScopedLogger::new(scope, self)
    }

    // Convenience functions

    /// Log an increment to a named counter (convenience function).
    ///
    /// Panics if this name was previously used to log a value of a different type.
    #[inline]
    fn log_counter_increment(&mut self, name: &'static str, increment: u64) {
        self.log(name.into(), LogValue::CounterIncrement(increment))
            .unwrap()
    }

    /// Log a named duration (convenience function).
    ///
    /// Panics if this name was previously used to log a value of a different type.
    #[inline]
    fn log_duration(&mut self, name: &'static str, duration: Duration) {
        self.log(name.into(), LogValue::Duration(duration)).unwrap()
    }

    /// Log a named duration as the elapsed time in evaluating a closure.
    ///
    /// The closure is passed a mutable reference to this logger so that it has the opportunity to
    /// make its own logging calls.
    #[inline]
    fn log_elapsed<F, T>(&mut self, name: &'static str, f: F) -> T
    where
        F: FnOnce(&mut Self) -> T,
        Self: Sized,
    {
        let start = Instant::now();
        let result = f(self);
        self.log_duration(name, start.elapsed());
        result
    }

    /// Log a named scalar value (convenience function).
    ///
    /// Panics if this name was previously used to log a value of a different type.
    #[inline]
    fn log_scalar(&mut self, name: &'static str, value: f64) {
        self.log(name.into(), LogValue::Scalar(value)).unwrap()
    }

    /// Log a named index in `0` to `size - 1` (convenience function).
    ///
    /// Panics if this name was previously used to log a value of a different type
    /// or an index value with a different size.
    #[inline]
    fn log_index(&mut self, name: &'static str, value: usize, size: usize) {
        self.log(name.into(), LogValue::Index { value, size })
            .unwrap()
    }
}

/// Implement `StatsLogger` for a deref-able wrapper type generic over `T: StatsLogger + ?Sized`.
macro_rules! impl_wrapped_stats_logger {
    ($wrapper:ty) => {
        impl<T> StatsLogger for $wrapper
        where
            T: StatsLogger + ?Sized,
        {
            #[inline]
            fn group_start(&mut self) {
                T::group_start(self)
            }
            #[inline]
            fn group_log(&mut self, id: Id, value: LogValue) -> Result<(), LogError> {
                T::group_log(self, id, value)
            }
            #[inline]
            fn group_end(&mut self) {
                T::group_end(self)
            }
            #[inline]
            fn flush(&mut self) {
                T::flush(self)
            }
        }
    };
}
impl_wrapped_stats_logger!(&'_ mut T);
impl_wrapped_stats_logger!(Box<T>);

/// Value that can be logged.
///
/// # Design Note
/// This is an enum to simplify the logging interface.
/// The options are:
/// * multiple methods
///     - pro: no dynamic dispatch; supports trait objects
///     - con: duplicates similar functionality; cannot batch log different types
/// * enum
///     - pro: simple interface with few methods; supports trait objects; batch log `log_items`
///     - con: dynamic dispatch, can fail with errors; wasted space in loggable / summary
/// * traits
///     - pro: possibly less dynamic dispatch in some cases
///     - con: must downcast for backend; complex interface; hard to do trait objects
#[derive(Debug, Clone, PartialEq)]
pub enum LogValue {
    Nothing,
    CounterIncrement(u64),
    Duration(Duration),
    Scalar(f64),
    Index { value: usize, size: usize },
}

impl From<f64> for LogValue {
    fn from(scalar: f64) -> Self {
        Self::Scalar(scalar)
    }
}

impl From<Duration> for LogValue {
    fn from(duration: Duration) -> Self {
        Self::Duration(duration)
    }
}

impl LogValue {
    const fn variant_name(&self) -> &'static str {
        use LogValue::*;
        match self {
            Nothing => "Nothing",
            CounterIncrement(_) => "CounterIncrement",
            Duration(_) => "Duration",
            Scalar(_) => "Scalar",
            Index { value: _, size: _ } => "Index",
        }
    }
}

/// A type that can be logged to a [`StatsLogger`].
///
/// While [`LogValue`] are the core log value types, a `Loggable` decomposes itself into zero or
/// more `LogValues` in the process of logging.
pub trait Loggable {
    fn log<L: StatsLogger + ?Sized>(
        &self,
        name: &'static str,
        logger: &mut L,
    ) -> Result<(), LogError>;
}

/// A hierarchical identifier.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Id {
    /// Base (innermost) name of the identifier.
    name: Cow<'static, str>,
    /// Hierarchical namespace in reverse order from innermost to outermost (top-level)
    namespace: SmallVec<[&'static str; 6]>,
}

impl PartialOrd for Id {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Id {
    fn cmp(&self, other: &Self) -> Ordering {
        self.components().cmp(other.components())
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let total_len = self.name.len() + self.namespace.iter().map(|s| s.len() + 1).sum::<usize>();

        // Pad on the left for right alignment if necessary
        if let Some(width) = f.width() {
            if width > total_len && matches!(f.align(), Some(fmt::Alignment::Right)) {
                let c = f.fill();
                for _ in 0..(width - total_len) {
                    write!(f, "{}", c)?;
                }
            }
        }

        for scope in self.namespace.iter().rev() {
            write!(f, "{}/", scope)?;
        }
        write!(f, "{}", self.name)?;

        // Pad on the right for left alignment if necessary
        if let Some(width) = f.width() {
            if width > total_len && matches!(f.align(), Some(fmt::Alignment::Left)) {
                let c = f.fill();
                for _ in 0..(width - total_len) {
                    write!(f, "{}", c)?;
                }
            }
        }

        Ok(())
    }
}

impl<T> From<T> for Id
where
    T: Into<Cow<'static, str>>,
{
    #[inline]
    fn from(name: T) -> Self {
        let name = name.into();
        debug_assert!(
            !name.contains('/'),
            "path separators are not allowed in Id name; \
            use [...].collect() or logger.with_scope(...) instead"
        );
        Self {
            name,
            namespace: SmallVec::new(),
        }
    }
}

impl FromIterator<&'static str> for Id {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = &'static str>,
    {
        let mut rev_namespace: SmallVec<[&'static str; 6]> = iter.into_iter().collect();
        let name = rev_namespace
            .pop()
            .expect("must have at least one name")
            .into();
        Self {
            name,
            namespace: rev_namespace.into_iter().rev().collect(),
        }
    }
}

impl Id {
    /// Add a new top-level name to the namespace.
    #[must_use]
    pub fn with_prefix(mut self, scope: &'static str) -> Self {
        self.namespace.push(scope);
        self
    }

    /// Iterator over namespace / name components.
    pub fn components(
        &self,
    ) -> iter::Chain<iter::Copied<iter::Rev<slice::Iter<&str>>>, iter::Once<&str>> {
        self.namespace
            .iter()
            .rev()
            .copied() // &&str -> &str
            .chain(iter::once(self.name.as_ref()))
    }
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum LogError {
    #[error("incompatible value type; previously {prev} now {now}")]
    IncompatibleValue {
        prev: &'static str,
        now: &'static str,
    },
    #[error("incompatible index size; previously {prev} now {now}")]
    IncompatibleIndexSize { prev: usize, now: usize },
}

/// No-op logger
impl StatsLogger for () {
    #[inline]
    fn group_start(&mut self) {}
    #[inline]
    fn group_log(&mut self, _: Id, _: LogValue) -> Result<(), LogError> {
        Ok(())
    }
    #[inline]
    fn group_end(&mut self) {}
    #[inline]
    fn flush(&mut self) {}
}

/// Pair of loggers; logs to both.
impl<A, B> StatsLogger for (A, B)
where
    A: StatsLogger,
    B: StatsLogger,
{
    fn group_start(&mut self) {
        self.0.group_start();
        self.1.group_start();
    }
    fn group_log(&mut self, id: Id, value: LogValue) -> Result<(), LogError> {
        // Log to both even if one fails
        let r1 = self.0.group_log(id.clone(), value.clone());
        let r2 = self.1.group_log(id, value);
        r1.and(r2)
    }
    fn group_end(&mut self) {
        self.0.group_end();
        self.1.group_end();
    }
    fn flush(&mut self) {
        self.0.flush();
        self.1.flush();
    }
}

/// Wraps all logged names with a scope.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScopedLogger<L> {
    scope: &'static str,
    logger: L,
}

impl<L> ScopedLogger<L> {
    #[inline]
    pub const fn new(scope: &'static str, logger: L) -> Self {
        Self { scope, logger }
    }
}

impl<L: StatsLogger> StatsLogger for ScopedLogger<L> {
    #[inline]
    fn group_start(&mut self) {
        self.logger.group_start()
    }
    #[inline]
    fn group_log(&mut self, id: Id, value: LogValue) -> Result<(), LogError> {
        self.logger.group_log(id.with_prefix(self.scope), value)
    }
    #[inline]
    fn group_end(&mut self) {
        self.logger.group_end()
    }
    #[inline]
    fn flush(&mut self) {
        self.logger.flush()
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
/// Context manager for logging a group of related entries in rapid succession. Prevents flushing.
pub struct LogGroup<L: StatsLogger>(L);
impl<L: StatsLogger> LogGroup<L> {
    #[inline]
    pub fn new(mut logger: L) -> Self {
        logger.group_start();
        Self(logger)
    }
}
impl<L: StatsLogger> StatsLogger for LogGroup<L> {
    #[inline]
    fn group_start(&mut self) {}
    #[inline]
    fn group_log(&mut self, id: Id, value: LogValue) -> Result<(), LogError> {
        self.0.group_log(id, value)
    }
    #[inline]
    fn group_end(&mut self) {}
    #[inline]
    fn flush(&mut self) {}
}

impl<L: StatsLogger> Drop for LogGroup<L> {
    #[inline]
    fn drop(&mut self) {
        self.0.group_end()
    }
}
