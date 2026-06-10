// When `logging` feature is enabled, delegate to the `log` crate.
// When disabled, the no-op variants use `let _ = &$arg` to suppress
// unused-variable warnings without creating fmt::Arguments temporaries
// (which are !Send and would break tokio::spawn across .await points).

#[cfg(feature = "logging")]
macro_rules! tf_debug {
    ($($arg:tt)*) => { ::log::debug!($($arg)*) };
}
#[cfg(not(feature = "logging"))]
macro_rules! tf_debug {
    ($fmt:literal $(,)?) => {};
    ($fmt:literal, $($arg:expr),+ $(,)?) => { $( let _ = &$arg; )+ };
}
pub(crate) use tf_debug;

#[cfg(feature = "logging")]
macro_rules! tf_info {
    ($($arg:tt)*) => { ::log::info!($($arg)*) };
}
#[cfg(not(feature = "logging"))]
macro_rules! tf_info {
    ($fmt:literal $(,)?) => {};
    ($fmt:literal, $($arg:expr),+ $(,)?) => { $( let _ = &$arg; )+ };
}
pub(crate) use tf_info;

#[cfg(feature = "logging")]
macro_rules! tf_warn {
    ($($arg:tt)*) => { ::log::warn!($($arg)*) };
}
#[cfg(not(feature = "logging"))]
macro_rules! tf_warn {
    ($fmt:literal $(,)?) => {};
    ($fmt:literal, $($arg:expr),+ $(,)?) => { $( let _ = &$arg; )+ };
}
pub(crate) use tf_warn;

#[cfg(feature = "logging")]
macro_rules! tf_error {
    ($($arg:tt)*) => { ::log::error!($($arg)*) };
}
#[cfg(not(feature = "logging"))]
macro_rules! tf_error {
    ($fmt:literal $(,)?) => {};
    ($fmt:literal, $($arg:expr),+ $(,)?) => { $( let _ = &$arg; )+ };
}
pub(crate) use tf_error;