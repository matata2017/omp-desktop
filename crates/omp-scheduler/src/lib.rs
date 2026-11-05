//! Scheduled tasks: when a cron expression fires, and the `scheduler.db` behind a task list.
//!
//! The watch loop that spawns runs lives in the app; this crate owns the parts that do not need
//! a window or an agent: the clock arithmetic (`cron`) and the durable ledger (`db`, `store`).

pub mod cron;
pub mod insights;
pub mod db;
pub mod store;

pub use cron::CronSchedule;
pub use db::TaskStore;
pub use insights::{compute_insights, derive_notices};
