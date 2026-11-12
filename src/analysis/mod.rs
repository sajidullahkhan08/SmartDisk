//! Post-scan analysis: aggregation, treemap layout, search, duplicates.

pub mod duplicates;
pub mod file_types;
pub mod search;
pub mod stats;
pub mod treemap;

pub use duplicates::{find_duplicates, DuplicateFile, DuplicateGroup, DuplicateProgress, DuplicateSnapshot};
pub use file_types::{category_stats, extension_stats, CategoryStat, ExtensionStat};
pub use search::{search, SearchFilters};
pub use stats::{overview, OverviewStats, TopLevelEntry};
pub use treemap::{squarify, Rect};
