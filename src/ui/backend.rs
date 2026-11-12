//! Slint controller: converts the analysis core into UI models and routes
//! user actions to scanners, analysis helpers and protected file operations.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use super::{
    AppWindow, CategoryRow, ContextAction, CrumbRow, DriveOption, DuplicateRow, EvidenceRow,
    ExtensionRow, GroupRow, HistoryRow, ItemRow, PlanRow, TopRow, TreemapCell, TreeRow,
};

use crate::analysis::duplicates::{describe_group, DuplicateGroup, DuplicateSnapshot};
use crate::analysis::file_types::{category_stats, extension_stats};
use crate::analysis::search::{search, SearchFilters};
use crate::analysis::stats::{overview, OverviewStats};
use crate::analysis::treemap::squarify;
use crate::cleanup::{build_plan, recommendations, total_recommended, CleanupGroup, CleanupPlan};
use crate::core::models::{
    Category, NodeId, RiskLevel, ScanResult, ScanStatus, INVALID_NODE,
};
use crate::core::util::{
    format_count, format_duration_ms, format_size, format_size_exact, format_timestamp,
    normalize_display_root, now_unix, percent,
};
use crate::file_operations::confirmation::describe_delete;
use crate::file_operations::operations::OperationOutcome;
use crate::file_operations::{clipboard, explorer, operations, FileClipboard};
use crate::intelligence::{assess_detailed, classify_detailed};
use crate::platform::drives::{list_drives, volume_info, DriveInfo};
use crate::platform::shell;
use crate::scanner::ScanOptions;
use crate::services::{DuplicateService, ScanService};
use crate::storage::database::{default_data_dir, Database, ScanHistoryEntry};
use crate::storage::settings::AppSettings;

const ACTION_OPEN: i32 = 1;
const ACTION_REVEAL: i32 = 2;
const ACTION_COPY: i32 = 3;
const ACTION_CUT: i32 = 4;
const ACTION_COPY_PATH: i32 = 5;
const ACTION_RENAME: i32 = 6;
const ACTION_RECYCLE: i32 = 7;
const ACTION_DELETE: i32 = 8;
const ACTION_PROPERTIES: i32 = 9;
const ACTION_SCAN_FOLDER: i32 = 10;

const MAX_EXPLORER_ITEMS: usize = 20_000;
const MAX_LARGEST_ROWS: usize = 1_000;
const MAX_SEARCH_ROWS: usize = 2_000;
const MAX_TREEMAP_CELLS: usize = 180;
const MAX_DUP_GROUPS: usize = 400;
const MAX_DUP_FILES_PER_GROUP: usize = 20;

#[derive(Clone, Debug)]
enum Pending {
    Rename(i32),
    Recycle(i32),
    Delete(i32),
    Cleanup(CleanupPlan),
    DupRecycle(usize, usize),
    DupExtras(usize),
    ClearData,
}

pub struct Backend {
    window: slint::Weak<AppWindow>,
    db: Database,
    settings: RefCell<AppSettings>,
    drives: Vec<DriveInfo>,
    scan: RefCell<Option<ScanService>>,
    result: RefCell<Option<Arc<ScanResult>>>,
    selected: Cell<i32>,
    current_folder: Cell<NodeId>,
    treemap_path: RefCell<Vec<NodeId>>,
    expanded: RefCell<HashSet<NodeId>>,
    deleted: RefCell<HashSet<NodeId>>,
    largest_ids: RefCell<Vec<NodeId>>,
    item_sort: Cell<(i32, bool)>,
    largest_sort: Cell<(i32, bool)>,
    search_sort: Cell<(i32, bool)>,
    search_ids: RefCell<Vec<NodeId>>,
    cleanup_groups: RefCell<Vec<CleanupGroup>>,
    cleanup_selected: RefCell<HashSet<String>>,
    cleanup_initialized: Cell<bool>,
    plan: RefCell<Option<CleanupPlan>>,
    dup_service: RefCell<Option<DuplicateService>>,
    dup_groups: RefCell<Vec<DuplicateGroup>>,
    clipboard: RefCell<FileClipboard>,
    pending: RefCell<Option<Pending>>,
    toast_until: Cell<Option<Instant>>,
    history: RefCell<Vec<ScanHistoryEntry>>,
    suppress_drive_callback: Cell<bool>,
    startup_guard_until: Cell<Option<Instant>>,
    db_path_text: String,
    // Models are kept alive so a selection change can update the highlight in
    // place. Replacing a model on every click recreates the row elements and
    // breaks double-click detection (the second click lands on a new item).
    explorer_model: RefCell<Option<ModelRc<ItemRow>>>,
    largest_model: RefCell<Option<ModelRc<ItemRow>>>,
    search_model: RefCell<Option<ModelRc<ItemRow>>>,
    treemap_model: RefCell<Option<ModelRc<TreemapCell>>>,
    file_op: RefCell<Option<std::sync::mpsc::Receiver<(bool, Result<OperationOutcome, String>)>>>,
}

fn rgb(color: (u8, u8, u8)) -> slint::Color {
    slint::Color::from_rgb_u8(color.0, color.1, color.2)
}

fn ss(text: impl Into<String>) -> SharedString {
    SharedString::from(text.into())
}

fn model<T: Clone + 'static>(rows: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(rows))
}

fn risk_color(level: RiskLevel) -> slint::Color {
    rgb(level.color())
}

/// Colour a treemap cell by the category that owns most of its bytes.
///
/// Directories carry a category inferred from their own name, which is often
/// `Unknown`; using the dominant descendant category makes a folder cell match
/// the content it represents while remaining a cheap, bounded computation.
fn treemap_color(result: &ScanResult, id: NodeId) -> slint::Color {
    let node = &result.nodes[id as usize];
    if !node.is_dir {
        return rgb(node.category.color());
    }
    let mut totals: HashMap<Category, u64> = HashMap::new();
    let mut stack = vec![id];
    let mut budget = 40_000usize;
    while let Some(current) = stack.pop() {
        for child in result.child_ids(current) {
            if budget == 0 {
                break;
            }
            budget -= 1;
            let child_node = &result.nodes[child as usize];
            if child_node.is_dir {
                stack.push(child);
            } else {
                let slot = totals.entry(child_node.category).or_insert(0);
                *slot = slot.saturating_add(child_node.size);
            }
        }
        if budget == 0 {
            break;
        }
    }
    totals
        .into_iter()
        .max_by_key(|(_, bytes)| *bytes)
        .map(|(category, _)| rgb(category.color()))
        .unwrap_or_else(|| rgb(node.category.color()))
}

impl Backend {
    fn window(&self) -> Option<AppWindow> {
        self.window.upgrade()
    }

    fn current_result(&self) -> Option<Arc<ScanResult>> {
        self.result.borrow().clone()
    }

    fn exists(&self, id: i32) -> bool {
        id >= 0
    }

    /// Build and retain a row model so later selection updates can be applied
    /// in place instead of recreating every row.
    fn own_rows<T: Clone + 'static>(
        &self,
        slot: &RefCell<Option<ModelRc<T>>>,
        rows: Vec<T>,
    ) -> ModelRc<T> {
        let model = ModelRc::new(VecModel::from(rows));
        *slot.borrow_mut() = Some(model.clone());
        model
    }

    /// Recolour the selected row in every file table without rebuilding them.
    fn sync_selection(&self) {
        let selected = self.selected.get();
        for slot in [
            &self.explorer_model,
            &self.largest_model,
            &self.search_model,
        ] {
            let guard = slot.borrow();
            let Some(model) = guard.as_ref() else { continue };
            for index in 0..model.row_count() {
                let Some(mut row) = model.row_data(index) else {
                    continue;
                };
                let want = row.id == selected;
                if row.selected != want {
                    row.selected = want;
                    model.set_row_data(index, row);
                }
            }
        }
    }

    fn is_deleted(&self, id: NodeId) -> bool {
        self.deleted.borrow().contains(&id)
    }

    // ---------------------------------------------------------------- startup

    pub fn initialize(&self) {
        let Some(window) = self.window() else { return };

        window.set_app_version(ss(env!("CARGO_PKG_VERSION")));
        window.set_dark(self.settings.borrow().is_dark());
        window.set_settings_dark(self.settings.borrow().is_dark());
        window.set_show_info(self.settings.borrow().show_info_panel);
        window.set_info_panel_open(false);
        window.set_status_right(ss(format!(
            "SmartDisk {}",
            env!("CARGO_PKG_VERSION")
        )));

        // The header dropdown shows compact drive letters (e.g. `C:`) next to
        // the path field, so the long `display()` name would duplicate it.
        let labels: Vec<String> = self.drives.iter().map(DriveInfo::short).collect();
        let options: Vec<DriveOption> = self
            .drives
            .iter()
            .map(|d| DriveOption {
                label: ss(d.display()),
                root: ss(d.root.clone()),
            })
            .collect();
        window.set_drive_labels(model(
            labels.into_iter().map(SharedString::from).collect(),
        ));
        window.set_drives(model(options));

        let initial = self.initial_path();
        window.set_path_text(ss(initial.clone()));
        // Setting the ComboBox index can emit a deferred `changed` callback once
        // the event loop starts; ignore drive callbacks briefly so the restored
        // path is not replaced by the drive root on launch.
        self.startup_guard_until
            .set(Some(Instant::now() + Duration::from_millis(1200)));
        self.sync_drive_index_to(&initial);
        self.refresh_settings_props();
        self.refresh_history();
        self.refresh_static_options();
        self.update_status_bar();
    }

    fn initial_path(&self) -> String {
        if self.settings.borrow().remember_last_location {
            let settings = self.settings.borrow();
            let last = settings.last_scan_path.trim();
            if !last.is_empty() && Path::new(last).exists() {
                return normalize_display_root(Path::new(last));
            }
        }
        self.drives
            .iter()
            .find(|d| d.is_fixed)
            .or_else(|| self.drives.first())
            .map(|d| d.root.clone())
            .unwrap_or_else(|| "C:\\".to_string())
    }

    fn sync_drive_index_to(&self, path: &str) {
        let root = path
            .get(..3)
            .filter(|s| s.as_bytes().get(1) == Some(&b':'))
            .map(|s| s.to_ascii_uppercase().replace('/', "\\"));
        let Some(root) = root else { return };
        if let Some(index) = self
            .drives
            .iter()
            .position(|d| d.root.eq_ignore_ascii_case(&root))
        {
            if let Some(window) = self.window() {
                self.suppress_drive_callback.set(true);
                window.set_drive_index(index as i32);
                self.suppress_drive_callback.set(false);
            }
        }
    }

    fn refresh_static_options(&self) {
        let Some(window) = self.window() else { return };
        let mut categories = vec![SharedString::from("All categories")];
        categories.extend(
            Category::ALL
                .iter()
                .map(|c| SharedString::from(c.label())),
        );
        window.set_category_options(model(categories));

        window.set_risk_options(model(vec![
            "All risk levels".into(),
            "Lower".into(),
            "Review".into(),
            "High".into(),
            "Critical".into(),
            "Unknown".into(),
        ]));
        window.set_kind_options(model(vec![
            "Files and folders".into(),
            "Files only".into(),
            "Folders only".into(),
        ]));
        window.set_search_cat_index(0);
        window.set_search_risk_index(0);
        window.set_search_kind_index(0);
    }

    // ---------------------------------------------------------------- scan

    fn scan_requested(&self) {
        let Some(window) = self.window() else { return };
        if window.get_scanning() {
            self.toast("A scan is already running.", true);
            return;
        }
        let raw = window.get_path_text().to_string();
        self.start_scan_from_text(&raw);
    }

    fn start_scan_from_text(&self, raw: &str) {
        let trimmed = raw.trim().trim_matches('"');
        if trimmed.is_empty() {
            self.toast("Choose a drive or folder to scan first.", true);
            return;
        }
        let path = PathBuf::from(trimmed);
        if !path.exists() {
            self.toast(&format!("The location does not exist: {trimmed}"), true);
            return;
        }
        if !path.is_dir() {
            self.toast("SmartDisk scans folders. Select a folder or drive.", true);
            return;
        }
        self.start_scan(path);
    }

    fn start_scan(&self, root: PathBuf) {
        let Some(window) = self.window() else { return };
        if self.scan.borrow().is_some() {
            self.toast("A scan is already running.", true);
            return;
        }
        let settings = self.settings.borrow();
        let missing_workers = settings.workers;
        let volume = volume_info(&root);
        let cluster = volume
            .as_ref()
            .map(|v| v.cluster_size)
            .filter(|c| *c > 0)
            .unwrap_or(4096);
        let exclusions = settings
            .exclusions
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .collect();
        drop(settings);
        let options = ScanOptions {
            root: root.clone(),
            workers: missing_workers,
            low_resource: self.settings.borrow().low_resource,
            follow_reparse_points: self.settings.borrow().follow_reparse_points,
            exclusions,
            cluster_size: cluster,
            volume,
        };

        let display = normalize_display_root(&root);
        window.set_scanning(true);
        window.set_scan_paused(false);
        window.set_can_scan(false);
        window.set_scan_status(ss("Starting scan…"));
        window.set_scan_current(ss(""));
        window.set_scan_files(ss("0 files"));
        window.set_scan_size(ss("0 B"));
        window.set_scan_elapsed(ss("0 ms"));
        window.set_result_title(ss(display.clone()));
        window.set_path_text(ss(display.clone()));
        self.sync_drive_index_to(&display);
        self.update_status_bar();

        *self.scan.borrow_mut() = Some(ScanService::start(options));
        let _ = self.db.add_recent_location(&display);
    }

    fn pause_requested(&self) {
        let Some(window) = self.window() else { return };
        if let Some(scan) = self.scan.borrow().as_ref() {
            if scan.is_paused() {
                scan.resume();
                window.set_scan_paused(false);
                window.set_scan_status(ss("Scan resumed…"));
            } else {
                scan.pause();
                window.set_scan_paused(true);
                window.set_scan_status(ss("Scan paused. Filesystem workers are idle."));
            }
        }
    }

    fn cancel_requested(&self) {
        if let Some(scan) = self.scan.borrow().as_ref() {
            scan.cancel();
        }
        if let Some(window) = self.window() {
            window.set_scan_status(ss("Cancelling scan…"));
        }
    }

    fn drive_selected(&self) {
        if self.suppress_drive_callback.get() {
            return;
        }
        if let Some(until) = self.startup_guard_until.get() {
            if Instant::now() < until {
                return;
            }
            self.startup_guard_until.set(None);
        }
        let Some(window) = self.window() else { return };
        let index = window.get_drive_index();
        if let Some(drive) = self.drives.get(index.max(0) as usize) {
            window.set_path_text(ss(drive.root.clone()));
            window.set_can_scan(true);
        }
    }

    fn choose_folder(&self) {
        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
            if let Some(window) = self.window() {
                let display = normalize_display_root(&folder);
                window.set_path_text(ss(display.clone()));
                self.sync_drive_index_to(&display);
            }
        }
    }

    fn tick_scan(&self) {
        let snapshot = {
            let scan_ref = self.scan.borrow();
            scan_ref.as_ref().map(|scan| {
                let snapshot = scan.progress().snapshot();
                (snapshot, scan.try_finish())
            })
        };
        let Some((snapshot, finished)) = snapshot else { return };

        if let Some(window) = self.window() {
            window.set_scan_files(ss(format!(
                "{} files · {} folders",
                format_count(snapshot.files),
                format_count(snapshot.dirs)
            )));
            window.set_scan_size(ss(format_size(snapshot.bytes)));
            window.set_scan_elapsed(ss(format_duration_ms(snapshot.elapsed_ms)));
            window.set_scan_current(ss(snapshot.current_path.clone()));
            window.set_scan_status(ss(format!(
                "{} files · {} folders · {} access errors · {} reparse points skipped",
                format_count(snapshot.files),
                format_count(snapshot.dirs),
                format_count(snapshot.errors),
                format_count(snapshot.skipped_reparse)
            )));
            window.set_status_middle(ss(format!(
                "{} · {}",
                format_size(snapshot.bytes),
                format_count(snapshot.files)
            )));
        }

        if let Some(outcome) = finished {
            let service = self.scan.borrow_mut().take();
            drop(service);
            match outcome {
                Ok(result) => self.finish_scan(result),
                Err(error) => {
                    if let Some(window) = self.window() {
                        window.set_scanning(false);
                        window.set_scan_paused(false);
                        window.set_can_scan(true);
                    }
                    self.update_status_bar();
                    self.toast(&format!("Scan failed: {error}"), true);
                }
            }
        }
    }

    fn finish_scan(&self, result: ScanResult) {
        let Some(window) = self.window() else { return };
        let display = normalize_display_root(Path::new(&result.root_path));
        let mut result = result;
        result.root_path = display.clone();

        let arc = Arc::new(result);
        *self.result.borrow_mut() = Some(arc.clone());
        self.deleted.borrow_mut().clear();
        self.selected.set(-1);
        self.current_folder.set(arc.root);
        self.treemap_path.borrow_mut().clear();
        self.treemap_path.borrow_mut().push(arc.root);
        self.expanded.borrow_mut().clear();
        self.expanded.borrow_mut().insert(arc.root);
        self.search_ids.borrow_mut().clear();
        self.cleanup_groups.borrow_mut().clear();
        self.cleanup_selected.borrow_mut().clear();
        self.cleanup_initialized.set(false);
        *self.plan.borrow_mut() = None;
        *self.dup_groups.borrow_mut() = Vec::new();
        window.set_dup_rows(model(Vec::new()));
        window.set_dup_has_run(false);
        window.set_dup_busy(false);
        window.set_dup_summary(ss(""));
        window.set_dup_total(ss(""));
        window.set_dup_progress_percent(0.0);
        window.set_plan_visible(false);

        self.largest_ids.borrow_mut().clear();
        let mut files: Vec<(NodeId, u64)> = arc
            .nodes
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, n)| !n.is_dir)
            .map(|(i, n)| (i as NodeId, n.size))
            .collect();
        files.sort_by(|a, b| b.1.cmp(&a.1));
        files.truncate(MAX_LARGEST_ROWS);
        *self.largest_ids.borrow_mut() = files.into_iter().map(|(id, _)| id).collect();

        window.set_scanning(false);
        window.set_scan_paused(false);
        window.set_can_scan(true);
        window.set_has_result(true);
        window.set_path_text(ss(display.clone()));
        self.sync_drive_index_to(&display);

        {
            let mut settings = self.settings.borrow_mut();
            settings.last_scan_path = display.clone();
            if settings.remember_last_location {
                let _ = self.db.save_settings(&settings);
            }
        }
        if let Err(error) = self.db.record_scan(&arc) {
            log::warn!("could not record scan history: {error}");
        }

        window.set_result_title(ss(display));
        window.set_result_path(ss(arc.root_path.clone()));
        window.set_result_meta(ss(format!(
            "{} files · {} folders · {} on disk",
            format_count(arc.stats.files),
            format_count(arc.stats.dirs),
            format_size(arc.stats.total_allocated)
        )));
        window.set_result_status(ss(arc.status.label()));
        window.set_result_status_color(match arc.status {
            ScanStatus::Complete => rgb((5, 150, 105)),
            ScanStatus::Partial => rgb((217, 119, 6)),
            ScanStatus::Cancelled => rgb((100, 116, 139)),
            ScanStatus::Failed => rgb((220, 38, 38)),
        });

        self.refresh_all();
        self.refresh_history();
        self.refresh_settings_props();
        self.update_status_bar();
        self.toast(
            &format!(
                "Scan complete: {} files, {}.",
                format_count(arc.stats.files),
                format_size(arc.stats.total_size)
            ),
            false,
        );
    }

    fn refresh_all(&self) {
        self.refresh_overview();
        self.refresh_explorer();
        self.refresh_treemap();
        self.refresh_largest();
        self.refresh_file_types();
        self.refresh_cleanup();
        self.refresh_info();
    }

    fn update_status_bar(&self) {
        let Some(window) = self.window() else { return };
        if window.get_scanning() {
            window.set_status_left(ss("Scanning…"));
        } else if let Some(result) = self.current_result() {
            window.set_status_left(ss(format!(
                "{} · {} items",
                result.root_path,
                format_count(result.node_count().saturating_sub(1) as u64)
            )));
        } else {
            window.set_status_left(ss("Ready"));
        }
        if !window.get_scanning() {
            window.set_status_middle(ss(""));
        }
        window.set_status_right(ss(format!(
            "SmartDisk {}",
            env!("CARGO_PKG_VERSION")
        )));
    }

    // ---------------------------------------------------------------- overview

    fn refresh_overview(&self) {
        let Some(window) = self.window() else { return };
        let Some(result) = self.current_result() else {
            return;
        };
        let stats: OverviewStats = overview(&result);

        window.set_stat_size(ss(format_size(stats.total_size)));
        window.set_stat_allocated(ss(format_size(stats.total_allocated)));
        window.set_stat_files(ss(format_count(stats.files)));
        window.set_stat_dirs(ss(format_count(stats.dirs)));
        window.set_stat_errors(ss(format_count(stats.errors)));
        window.set_stat_elapsed(ss(format_duration_ms(stats.elapsed_ms)));
        window.set_stat_average(ss(format_size(stats.average_file_size)));
        window.set_stat_categories(ss(format_count(stats.category_count as u64)));
        window.set_overview_summary(ss(format!(
            "{} files and {} folders. Logical size {}; estimated allocation {}. {} access issue(s), {} reparse point(s) skipped.",
            format_count(stats.files),
            format_count(stats.dirs),
            format_size(stats.total_size),
            format_size(stats.total_allocated),
            format_count(stats.errors),
            format_count(stats.skipped_reparse)
        )));

        if let Some(volume) = result.volume.clone() {
            window.set_has_volume(true);
            let label = if volume.label.trim().is_empty() {
                volume.drive_type.clone()
            } else {
                volume.label.clone()
            };
            window.set_volume_title(ss(format!("{}  ({})", label, volume.root)));
            window.set_volume_detail(ss(format!(
                "{} · cluster {}",
                if volume.filesystem.is_empty() {
                    "Unknown filesystem"
                } else {
                    volume.filesystem.as_str()
                },
                format_size(volume.cluster_size)
            )));
            window.set_volume_free_text(ss(format!(
                "{} free of {}",
                format_size(volume.free),
                format_size(volume.total)
            )));
            window.set_volume_used_percent(volume.used_percent() as f32);
        } else {
            window.set_has_volume(false);
        }

        if let Some((_, path, size)) = &stats.largest_file {
            let name = Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.clone());
            window.set_largest_file_name(ss(name));
            window.set_largest_file_size(ss(format_size(*size)));
            window.set_largest_file_path(ss(path.clone()));
        } else {
            window.set_largest_file_name(ss("—"));
            window.set_largest_file_size(ss("—"));
            window.set_largest_file_path(ss(""));
        }
        if let Some((_, path, size)) = &stats.largest_folder {
            let name = Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.clone());
            window.set_largest_folder_name(ss(name));
            window.set_largest_folder_size(ss(format_size(*size)));
            window.set_largest_folder_path(ss(path.clone()));
        } else {
            window.set_largest_folder_name(ss("—"));
            window.set_largest_folder_size(ss("—"));
            window.set_largest_folder_path(ss(""));
        }

        let categories: Vec<CategoryRow> = category_stats(&result)
            .into_iter()
            .map(|stat| CategoryRow {
                key: ss(stat.category.key()),
                name: ss(stat.category.label()),
                size: ss(format_size(stat.size)),
                percent: percent(stat.size, result.stats.total_size) as f32,
                color: rgb(stat.category.color()),
                count: ss(format_count(stat.count)),
            })
            .collect();
        window.set_categories(model(categories));

        let total = result.stats.total_size.max(1);
        let top: Vec<TopRow> = stats
            .top_level
            .iter()
            .map(|entry| TopRow {
                id: entry.id.map(|id| id as i32).unwrap_or(-1),
                name: ss(entry.name.clone()),
                size: ss(format_size(entry.size)),
                percent: percent(entry.size, total) as f32,
                color: entry
                    .id
                    .and_then(|id| result.node(id))
                    .map(|n| rgb(n.category.color()))
                    .unwrap_or(rgb(Category::Other.color())),
                is_dir: entry.is_dir,
            })
            .collect();
        window.set_top_level(model(top));
    }

    // ---------------------------------------------------------------- explorer

    fn refresh_explorer(&self) {
        let Some(window) = self.window() else { return };
        let Some(result) = self.current_result() else {
            return;
        };
        let folder = self.current_folder.get();
        if result.node(folder).is_none() {
            self.current_folder.set(result.root);
        }
        let folder = self.current_folder.get();
        let selected = self.selected.get();
        let deleted = self.deleted.borrow();

        let mut ids: Vec<NodeId> = result
            .child_ids(folder)
            .into_iter()
            .filter(|id| !deleted.contains(id))
            .collect();
        let (column, asc) = self.item_sort.get();
        let mut sortable = ids.clone();
        sort_ids(&result, &mut sortable, column, asc);
        ids = sortable;

        let total = ids.len();
        ids.truncate(MAX_EXPLORER_ITEMS);
        let rows: Vec<ItemRow> = ids
            .iter()
            .map(|id| self.item_row(&result, *id, selected == *id as i32))
            .collect();
        window.set_items(self.own_rows(&self.explorer_model, rows));
        window.set_item_sort_column(column);
        window.set_item_sort_asc(asc);

        let node_name = result
            .node(folder)
            .map(|n| {
                if folder == result.root {
                    result.root_path.clone()
                } else {
                    n.name.clone()
                }
            })
            .unwrap_or_default();
        window.set_explorer_title(ss(node_name));
        window.set_explorer_path(ss(result.path_of(folder)));
        window.set_item_count_text(ss(if total > MAX_EXPLORER_ITEMS {
            format!(
                "{} items · showing first {}",
                format_count(total as u64),
                format_count(MAX_EXPLORER_ITEMS as u64)
            )
        } else {
            format!("{} items", format_count(total as u64))
        }));

        drop(deleted);
        window.set_tree_rows(model(self.tree_rows(&result)));
        let folders = result
            .nodes
            .iter()
            .skip(1)
            .filter(|n| n.is_dir)
            .count();
        window.set_tree_summary(ss(format!("{} folders", format_count(folders as u64))));
    }

    fn tree_rows(&self, result: &ScanResult) -> Vec<TreeRow> {
        let selected = self.selected.get();
        let expanded = self.expanded.borrow().clone();
        let deleted = self.deleted.borrow();
        let mut rows = Vec::new();
        let mut stack: Vec<(NodeId, i32)> = vec![(result.root, 0)];
        while let Some((id, depth)) = stack.pop() {
            let Some(node) = result.node(id) else { continue };
            if deleted.contains(&id) {
                continue;
            }
            let mut children: Vec<NodeId> = result
                .child_ids(id)
                .into_iter()
                .filter(|child| {
                    result
                        .node(*child)
                        .map(|n| n.is_dir && !deleted.contains(child))
                        .unwrap_or(false)
                })
                .collect();
            children.sort_by(|a, b| {
                result.nodes[*b as usize]
                    .size
                    .cmp(&result.nodes[*a as usize].size)
            });
            rows.push(TreeRow {
                id: id as i32,
                depth,
                name: ss(if id == result.root {
                    result.root_path.clone()
                } else {
                    node.name.clone()
                }),
                size: ss(format_size(node.size)),
                is_dir: node.is_dir,
                expanded: expanded.contains(&id),
                has_children: !children.is_empty(),
                selected: selected == id as i32,
                color: rgb(node.category.color()),
            });
            if expanded.contains(&id) {
                for child in children.into_iter().rev() {
                    stack.push((child, depth + 1));
                }
            }
        }
        rows
    }

    fn item_row(&self, result: &ScanResult, id: NodeId, selected: bool) -> ItemRow {
        let node = &result.nodes[id as usize];
        let extension = node.extension();
        let type_label = if node.is_dir {
            "Folder".to_string()
        } else if extension.is_empty() {
            "File".to_string()
        } else {
            format!("{} file", extension.to_ascii_uppercase())
        };
        ItemRow {
            id: id as i32,
            name: ss(node.name.clone()),
            path: ss(result.path_of(id)),
            type_label: ss(type_label),
            size: ss(format_size(node.size)),
            allocated: ss(format_size(node.allocated)),
            modified: ss(node
                .modified
                .map(format_timestamp)
                .unwrap_or_else(|| "—".into())),
            category: ss(node.category.label()),
            category_color: rgb(node.category.color()),
            risk: ss(node.risk.short_label()),
            risk_color: risk_color(node.risk),
            is_dir: node.is_dir,
            selected,
        }
    }

    fn select_node(&self, id: i32, expand_ancestors: bool) {
        if id < 0 {
            return;
        }
        let Some(result) = self.current_result() else { return };
        if result.node(id as NodeId).is_none() {
            return;
        }
        self.selected.set(id);
        if expand_ancestors {
            let mut current = id as NodeId;
            let mut guard = 0;
            while let Some(parent) = result.parent_of(current) {
                self.expanded.borrow_mut().insert(parent);
                current = parent;
                guard += 1;
                if guard > 4096 {
                    break;
                }
            }
        }
        self.refresh_info();
        self.sync_selection();
        self.sync_treemap_selection();
    }

    /// Update the treemap highlight in place. Rebuilding the cell model here
    /// would destroy the touch areas between the two clicks of a double-click.
    fn sync_treemap_selection(&self) {
        let Some(model) = self.treemap_model.borrow().clone() else {
            self.refresh_treemap();
            return;
        };
        let selected = self.selected.get();
        for index in 0..model.row_count() {
            let Some(mut cell) = model.row_data(index) else {
                continue;
            };
            let want = cell.id == selected;
            if cell.selected != want {
                cell.selected = want;
                model.set_row_data(index, cell);
            }
        }
    }

    fn activate_node(&self, id: i32) {
        let Some(result) = self.current_result() else { return };
        let id = id.max(0) as NodeId;
        let Some(node) = result.node(id) else { return };
        if node.is_dir {
            self.expanded.borrow_mut().insert(id);
            self.current_folder.set(id);
            self.selected.set(-1);
            self.refresh_explorer();
            self.refresh_info();
            self.sync_selection();
            self.sync_treemap_selection();
            self.update_status_bar();
        } else {
            let path = result.path_of(id);
            match shell::open_path(Path::new(&path)) {
                Ok(()) => {}
                Err(error) => self.toast(&error, true),
            }
        }
    }

    fn go_up(&self) {
        let Some(result) = self.current_result() else { return };
        let Some(window) = self.window() else { return };
        if window.get_page() == 2 {
            let mut path = self.treemap_path.borrow_mut();
            if path.len() > 1 {
                path.pop();
            }
            drop(path);
            self.refresh_treemap();
            return;
        }
        let folder = self.current_folder.get();
        if let Some(parent) = result.parent_of(folder) {
            self.current_folder.set(parent);
            self.selected.set(-1);
            self.refresh_explorer();
            self.refresh_info();
        }
    }

    fn crumb_clicked(&self, id: i32) {
        let Some(result) = self.current_result() else { return };
        let id = id.max(0) as NodeId;
        if result.node(id).is_none() {
            return;
        }
        let mut path = self.treemap_path.borrow_mut();
        if let Some(position) = path.iter().position(|n| *n == id) {
            path.truncate(position + 1);
        } else {
            path.clear();
            path.push(result.root);
            path.push(id);
        }
        drop(path);
        self.refresh_treemap();
    }

    fn tree_toggle(&self, id: i32) {
        if id < 0 {
            return;
        }
        let id = id as NodeId;
        let mut expanded = self.expanded.borrow_mut();
        if !expanded.remove(&id) {
            expanded.insert(id);
        }
        drop(expanded);
        if let Some(result) = self.current_result() {
            if let Some(window) = self.window() {
                window.set_tree_rows(model(self.tree_rows(&result)));
            }
        }
    }

    fn top_level_clicked(&self, id: i32) {
        if id >= 0 {
            let id = id as NodeId;
            self.expanded.borrow_mut().insert(id);
            self.current_folder.set(id);
            self.selected.set(-1);
            if let Some(window) = self.window() {
                window.set_page(1);
            }
            self.refresh_explorer();
            self.refresh_info();
        }
    }

    fn overview_open_largest(&self, index: i32, _is_dir: bool) {
        if index < 0 {
            return;
        }
        let Some(result) = self.current_result() else { return };
        let id = self.largest_ids.borrow().first().copied();
        let Some(id) = id else { return };
        if let Some(parent) = result.parent_of(id) {
            self.current_folder.set(parent);
        }
        if let Some(window) = self.window() {
            window.set_page(1);
        }
        self.select_node(id as i32, true);
    }

    // ---------------------------------------------------------------- treemap

    fn refresh_treemap(&self) {
        let Some(window) = self.window() else { return };
        let Some(result) = self.current_result() else {
            return;
        };
        let mut path = self.treemap_path.borrow_mut();
        if path.is_empty() {
            path.push(result.root);
        }
        while path.len() > 1 && result.node(*path.last().unwrap()).is_none() {
            path.pop();
        }
        let folder = *path.last().unwrap();
        let breadcrumbs: Vec<CrumbRow> = path
            .iter()
            .map(|id| CrumbRow {
                id: *id as i32,
                name: ss(if *id == result.root {
                    result.root_path.clone()
                } else {
                    result
                        .node(*id)
                        .map(|n| n.name.clone())
                        .unwrap_or_default()
                }),
            })
            .collect();
        drop(path);
        window.set_breadcrumbs(model(breadcrumbs));

        let selected = self.selected.get();
        let deleted = self.deleted.borrow();
        let mut children: Vec<NodeId> = result
            .child_ids(folder)
            .into_iter()
            .filter(|id| {
                !deleted.contains(id)
                    && result
                        .node(*id)
                        .map(|n| n.size > 0)
                        .unwrap_or(false)
            })
            .collect();
        children.sort_by(|a, b| {
            result.nodes[*b as usize]
                .size
                .cmp(&result.nodes[*a as usize].size)
        });
        let total_children = children.len();
        let has_subfolder = children
            .iter()
            .any(|id| result.node(*id).map(|node| node.is_dir).unwrap_or(false));
        children.truncate(MAX_TREEMAP_CELLS);
        let sizes: Vec<f64> = children
            .iter()
            .map(|id| result.nodes[*id as usize].size as f64)
            .collect();
        let rects = squarify(&sizes);
        let cells: Vec<TreemapCell> = children
            .iter()
            .zip(rects.iter())
            .map(|(id, rect)| {
                let node = &result.nodes[*id as usize];
                TreemapCell {
                    id: *id as i32,
                    name: ss(node.name.clone()),
                    size: ss(format_size(node.size)),
                    x: rect.x as f32,
                    y: rect.y as f32,
                    w: rect.w as f32,
                    h: rect.h as f32,
                    color: treemap_color(&result, *id),
                    is_dir: node.is_dir,
                    selected: selected == *id as i32,
                }
            })
            .collect();
        let empty = cells.is_empty();
        window.set_treemap_cells(self.own_rows(&self.treemap_model, cells));
        window.set_treemap_empty(empty);
        window.set_treemap_total(ss(result
            .node(folder)
            .map(|n| format_size(n.size))
            .unwrap_or_else(|| "0 B".into())));
        if empty {
            window.set_treemap_info(ss("This folder has no items with a measurable size."));
        } else if total_children > MAX_TREEMAP_CELLS {
            window.set_treemap_info(ss(format!(
                "Showing the {} largest of {} items. Double-click a folder to zoom in.",
                MAX_TREEMAP_CELLS, total_children
            )));
        } else if has_subfolder {
            window.set_treemap_info(ss(
                "Double-click a folder to zoom in. Right-click for file actions.",
            ));
        } else {
            window.set_treemap_info(ss(
                "This folder contains files only. Click to select, right-click for actions.",
            ));
        }
    }

    fn treemap_zoom(&self, id: i32) {
        let Some(result) = self.current_result() else { return };
        let id = id.max(0) as NodeId;
        let Some(node) = result.node(id) else { return };
        if !node.is_dir {
            self.select_node(id as i32, false);
            return;
        }
        self.treemap_path.borrow_mut().push(id);
        self.selected.set(id as i32);
        self.refresh_treemap();
        self.refresh_info();
        self.sync_selection();
    }

    // ---------------------------------------------------------------- largest

    fn refresh_largest(&self) {
        let Some(window) = self.window() else { return };
        let Some(result) = self.current_result() else {
            return;
        };
        let selected = self.selected.get();
        let deleted = self.deleted.borrow();
        let (column, asc) = self.largest_sort.get();
        let mut ids: Vec<NodeId> = self
            .largest_ids
            .borrow()
            .iter()
            .copied()
            .filter(|id| !deleted.contains(id))
            .collect();
        sort_ids(&result, &mut ids, column, asc);
        let rows: Vec<ItemRow> = ids
            .iter()
            .map(|id| self.item_row(&result, *id, selected == *id as i32))
            .collect();
        window.set_largest_rows(self.own_rows(&self.largest_model, rows));
        window.set_largest_sort_column(column);
        window.set_largest_sort_asc(asc);
        window.set_largest_summary(ss(format!(
            "The {} largest files in this scan. Double-click a file to open it; use the context menu for safe actions.",
            format_count(ids.len() as u64)
        )));
    }

    // ---------------------------------------------------------------- file types

    fn refresh_file_types(&self) {
        let Some(window) = self.window() else { return };
        let Some(result) = self.current_result() else {
            return;
        };
        let stats = category_stats(&result);
        let categories: Vec<CategoryRow> = stats
            .iter()
            .map(|stat| CategoryRow {
                key: ss(stat.category.key()),
                name: ss(stat.category.label()),
                size: ss(format_size(stat.size)),
                percent: percent(stat.size, result.stats.total_size) as f32,
                color: rgb(stat.category.color()),
                count: ss(format_count(stat.count)),
            })
            .collect();
        window.set_categories(model(categories));
        window.set_file_types_summary(ss(format!(
            "File sizes grouped by likely purpose. These totals cover files only, so they add up to the scanned file size ({}).",
            format_size(result.stats.total_size)
        )));

        let extensions = extension_stats(&result, 60);
        let extension_rows: Vec<ExtensionRow> = extensions
            .iter()
            .map(|ext| ExtensionRow {
                extension: ss(if ext.extension == "(none)" {
                    "(no extension)".to_string()
                } else {
                    format!(".{}", ext.extension)
                }),
                count: ss(format_count(ext.count)),
                size: ss(format_size(ext.size)),
            })
            .collect();
        window.set_extension_rows(model(extension_rows));
        window.set_extension_summary(ss(format!(
            "Top {} extensions by size",
            extensions.len()
        )));
    }

    // ---------------------------------------------------------------- search

    fn run_search(&self) {
        let Some(window) = self.window() else { return };
        let Some(result) = self.current_result() else {
            return;
        };
        let query = window.get_search_query().to_string();
        let category_index = window.get_search_cat_index().max(0) as usize;
        let risk_index = window.get_search_risk_index().max(0) as usize;
        let kind_index = window.get_search_kind_index().max(0) as usize;
        let min_text = window.get_search_min_text().to_string();
        let extension = window.get_search_extension().to_string();

        let min_size = min_text
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| *v >= 0.0)
            .map(|mb| (mb * 1024.0 * 1024.0) as u64);
        let category = if category_index == 0 {
            None
        } else {
            Category::ALL.get(category_index - 1).copied()
        };
        let risk = match risk_index {
            1 => Some(RiskLevel::Lower),
            2 => Some(RiskLevel::Review),
            3 => Some(RiskLevel::High),
            4 => Some(RiskLevel::Critical),
            5 => Some(RiskLevel::Unknown),
            _ => None,
        };
        let filters = SearchFilters {
            query,
            min_size,
            max_size: None,
            older_than_days: None,
            newer_than_days: None,
            category,
            risk,
            files_only: kind_index == 1,
            folders_only: kind_index == 2,
            extension: if extension.trim().is_empty() {
                None
            } else {
                Some(extension)
            },
            limit: MAX_SEARCH_ROWS,
        };
        let mut ids = search(&result, &filters, now_unix());
        ids.retain(|id| !self.is_deleted(*id));
        let total_found = ids.len();
        *self.search_ids.borrow_mut() = ids;
        self.refresh_search();
        window.set_search_summary(ss(if total_found >= MAX_SEARCH_ROWS {
            format!(
                "Showing the first {} matches. Narrow the query or filters to see more.",
                format_count(MAX_SEARCH_ROWS as u64)
            )
        } else if total_found == 1 {
            "1 matching item.".to_string()
        } else {
            format!("{} matching items.", format_count(total_found as u64))
        }));
    }

    fn refresh_search(&self) {
        let Some(window) = self.window() else { return };
        let Some(result) = self.current_result() else {
            return;
        };
        let selected = self.selected.get();
        let deleted = self.deleted.borrow();
        let (column, asc) = self.search_sort.get();
        let mut ids: Vec<NodeId> = self
            .search_ids
            .borrow()
            .iter()
            .copied()
            .filter(|id| !deleted.contains(id))
            .collect();
        sort_ids(&result, &mut ids, column, asc);
        let rows: Vec<ItemRow> = ids
            .iter()
            .map(|id| self.item_row(&result, *id, selected == *id as i32))
            .collect();
        window.set_search_rows(self.own_rows(&self.search_model, rows));
        window.set_search_sort_column(column);
        window.set_search_sort_asc(asc);
    }

    fn search_reset(&self) {
        let Some(window) = self.window() else { return };
        window.set_search_query(ss(""));
        window.set_search_cat_index(0);
        window.set_search_risk_index(0);
        window.set_search_kind_index(0);
        window.set_search_min_text(ss(""));
        window.set_search_extension(ss(""));
        self.search_ids.borrow_mut().clear();
        window.set_search_rows(model(Vec::new()));
        window.set_search_summary(ss("Enter a query or choose filters, then press Search."));
    }

    fn filter_by_category(&self, key: String) {
        let Some(window) = self.window() else { return };
        let index = Category::ALL
            .iter()
            .position(|category| category.key() == key)
            .map(|i| i as i32 + 1)
            .unwrap_or(0);
        window.set_search_cat_index(index);
        window.set_page(7);
        self.run_search();
    }

    // ---------------------------------------------------------------- cleanup

    fn refresh_cleanup(&self) {
        let Some(window) = self.window() else { return };
        let Some(result) = self.current_result() else {
            window.set_cleanup_has_data(false);
            window.set_cleanup_summary(ss("No scan yet. Run a scan to see cleanup suggestions."));
            return;
        };
        let deleted = self.deleted.borrow();
        let groups: Vec<CleanupGroup> = recommendations(&result)
            .into_iter()
            .filter_map(|mut group| {
                group.items.retain(|item| !deleted.contains(&item.node));
                if group.items.is_empty() {
                    return None;
                }
                group.size = group.items.iter().map(|i| i.size).sum();
                group.count = group.items.len() as u64;
                Some(group)
            })
            .collect();
        drop(deleted);

        if !self.cleanup_initialized.get() {
            self.cleanup_initialized.set(true);
            let keys: HashSet<String> = groups.iter().map(|g| g.key.to_string()).collect();
            *self.cleanup_selected.borrow_mut() = keys;
        }
        let keys: HashSet<String> = groups.iter().map(|g| g.key.to_string()).collect();
        self.cleanup_selected
            .borrow_mut()
            .retain(|key| keys.contains(key));

        let (total_size, total_count) = total_recommended(&groups);
        let rows: Vec<GroupRow> = groups
            .iter()
            .map(|group| GroupRow {
                key: ss(group.key),
                title: ss(group.title),
                description: ss(group.description),
                size: ss(format_size(group.size)),
                count: ss(format!("{} files", format_count(group.count))),
                color: rgb(group.color()),
                selected: self.cleanup_selected.borrow().contains(group.key),
            })
            .collect();

        *self.cleanup_groups.borrow_mut() = groups;
        window.set_cleanup_groups(model(rows));
        window.set_cleanup_has_data(total_count > 0);
        window.set_cleanup_total(ss(format_size(total_size)));
        if total_count == 0 {
            window.set_cleanup_summary(ss(
                "No recognized temporary, cache, log, update or Recycle Bin files were found. Nothing is suggested for cleanup.",
            ));
        } else {
            window.set_cleanup_summary(ss(format!(
                "{} candidate files in recognized locations. Review the selection and preview before anything is moved.",
                format_count(total_count)
            )));
        }
        self.refresh_cleanup_selection();
    }

    fn refresh_cleanup_selection(&self) {
        let Some(window) = self.window() else { return };
        let selected = self.cleanup_selected.borrow();
        let groups = self.cleanup_groups.borrow();
        let mut size = 0u64;
        let mut count = 0u64;
        for group in groups.iter() {
            if selected.contains(group.key) {
                size = size.saturating_add(group.size);
                count += group.count;
            }
        }
        window.set_cleanup_selected_size(ss(format!(
            "{} selected · {}",
            format_size(size),
            format_count(count)
        )));
        let rows: Vec<GroupRow> = groups
            .iter()
            .map(|group| GroupRow {
                key: ss(group.key),
                title: ss(group.title),
                description: ss(group.description),
                size: ss(format_size(group.size)),
                count: ss(format!("{} files", format_count(group.count))),
                color: rgb(group.color()),
                selected: selected.contains(group.key),
            })
            .collect();
        drop(groups);
        drop(selected);
        window.set_cleanup_groups(model(rows));
    }

    fn cleanup_toggle(&self, key: String, checked: bool) {
        {
            let mut selected = self.cleanup_selected.borrow_mut();
            if checked {
                selected.insert(key);
            } else {
                selected.remove(&key);
            }
        }
        self.refresh_cleanup_selection();
    }

    fn cleanup_select_all(&self) {
        let keys: HashSet<String> = self
            .cleanup_groups
            .borrow()
            .iter()
            .map(|g| g.key.to_string())
            .collect();
        *self.cleanup_selected.borrow_mut() = keys;
        self.refresh_cleanup_selection();
    }

    fn cleanup_clear(&self) {
        self.cleanup_selected.borrow_mut().clear();
        self.refresh_cleanup_selection();
    }

    fn cleanup_preview(&self) {
        let Some(window) = self.window() else { return };
        let Some(result) = self.current_result() else {
            self.toast("Run a scan before previewing cleanup.", true);
            return;
        };
        let selected = self.cleanup_selected.borrow().clone();
        let ids: Vec<NodeId> = self
            .cleanup_groups
            .borrow()
            .iter()
            .filter(|group| selected.contains(group.key))
            .flat_map(|group| group.items.iter().map(|item| item.node))
            .collect();
        if ids.is_empty() {
            self.toast("Select at least one cleanup group first.", true);
            return;
        }
        let plan = build_plan(&result, &ids, window.get_plan_recycle());
        self.show_plan(plan);
    }

    fn show_plan(&self, plan: CleanupPlan) {
        let Some(window) = self.window() else { return };
        let rows: Vec<PlanRow> = plan
            .items
            .iter()
            .map(|item| PlanRow {
                name: ss(item.name.clone()),
                path: ss(item.path.clone()),
                size: ss(format_size(item.size)),
                risk: ss(item.risk.short_label()),
                risk_color: risk_color(item.risk),
            })
            .collect();
        window.set_plan_rows(model(rows));
        window.set_plan_total(ss(format_size(plan.total_size)));
        let item_label = if plan.items.len() == 1 { "item" } else { "items" };
        window.set_plan_count_text(ss(format!(
            "{} {} · {} blocked",
            format_count(plan.items.len() as u64),
            item_label,
            format_count(plan.blocked.len() as u64)
        )));
        window.set_plan_summary(ss(plan.estimate.summary.clone()));
        window.set_plan_warning(ss(plan.estimate.warnings.join("\n")));
        window.set_plan_recycle(plan.use_recycle_bin);
        window.set_plan_confirm_label(ss(if plan.use_recycle_bin {
            "Move to Recycle Bin"
        } else {
            "Delete permanently"
        }));
        *self.plan.borrow_mut() = Some(plan);
        window.set_plan_visible(true);
    }

    fn plan_recycle_toggled(&self, recycle: bool) {
        let Some(window) = self.window() else { return };
        let Some(result) = self.current_result() else { return };
        let current = self.plan.borrow().clone();
        let ids: Vec<NodeId> = current
            .as_ref()
            .map(|plan| plan.items.iter().map(|item| item.node).collect())
            .unwrap_or_default();
        if ids.is_empty() {
            return;
        }
        let plan = build_plan(&result, &ids, recycle);
        window.set_plan_recycle(recycle);
        self.show_plan(plan);
    }

    fn cleanup_close_plan(&self) {
        if let Some(window) = self.window() {
            window.set_plan_visible(false);
        }
        *self.plan.borrow_mut() = None;
    }

    fn cleanup_confirm_plan(&self) {
        let Some(plan) = self.plan.borrow().clone() else { return };
        if plan.items.is_empty() {
            self.toast("The cleanup plan is empty.", true);
            return;
        }
        let permanent = !plan.use_recycle_bin;
        if permanent || self.settings.borrow().confirm_destructive {
            let paths: Vec<&Path> = plan
                .items
                .iter()
                .map(|item| Path::new(item.path.as_str()))
                .collect();
            let sizes: Vec<u64> = plan.items.iter().map(|item| item.size).collect();
            let confirmation = describe_delete(&paths, &sizes, permanent);
            self.show_dialog(
                &confirmation.title,
                &confirmation.message,
                if permanent { "Delete permanently" } else { "Move to Recycle Bin" },
                "Cancel",
                permanent,
                false,
                "",
                "",
                Pending::Cleanup(plan),
            );
        } else {
            self.perform_cleanup(plan);
        }
    }

    fn perform_cleanup(&self, plan: CleanupPlan) {
        let paths: Vec<PathBuf> = plan
            .items
            .iter()
            .map(|item| PathBuf::from(&item.path))
            .collect();
        let outcome = if plan.use_recycle_bin {
            clipboard::recycle_items(&paths)
        } else {
            operations::delete_permanent(&paths)
        };
        if let Some(window) = self.window() {
            window.set_plan_visible(false);
        }
        *self.plan.borrow_mut() = None;
        match outcome {
            Ok(outcome) => {
                if outcome.failed.is_empty() {
                    let count = outcome.succeeded.len();
                    self.toast(
                        &format!(
                            "{} {} processed. Reclaimable space is an estimate; rescanning for an updated view.",
                            count,
                            if count == 1 { "item" } else { "items" }
                        ),
                        false,
                    );
                    if let Some(result) = self.current_result() {
                        let root = PathBuf::from(result.root_path.clone());
                        self.start_scan(root);
                    }
                } else {
                    let first = outcome
                        .failed
                        .first()
                        .map(|(path, error)| format!("{}: {error}", path.display()))
                        .unwrap_or_default();
                    self.toast(
                        &format!(
                            "{} item(s) processed, {} failed. {first}",
                            outcome.succeeded.len(),
                            outcome.failed.len()
                        ),
                        true,
                    );
                }
            }
            Err(error) => self.toast(&error.to_string(), true),
        }
    }

    // ---------------------------------------------------------------- duplicates

    fn duplicates_scan(&self) {
        let Some(window) = self.window() else { return };
        let Some(result) = self.current_result() else {
            self.toast("Run a scan before looking for duplicates.", true);
            return;
        };
        if self.dup_service.borrow().is_some() {
            return;
        }
        let minimum = self.settings.borrow().min_duplicate_size.max(1);
        *self.dup_service.borrow_mut() = Some(DuplicateService::start(result, minimum));
        window.set_dup_busy(true);
        window.set_dup_has_run(false);
        window.set_dup_stage(ss("Collecting candidate files"));
        window.set_dup_progress(ss(""));
        window.set_dup_summary(ss(""));
        window.set_dup_total(ss(""));
        window.set_dup_progress_percent(0.0);
        window.set_dup_rows(model(Vec::new()));
    }

    fn duplicates_cancel(&self) {
        if let Some(service) = self.dup_service.borrow().as_ref() {
            service.cancel();
        }
        if let Some(window) = self.window() {
            window.set_dup_stage(ss("Cancelling…"));
        }
    }

    fn tick_duplicates(&self) {
        let snapshot: Option<(DuplicateSnapshot, Option<Vec<DuplicateGroup>>)> = {
            let service = self.dup_service.borrow();
            service.as_ref().map(|service| {
                let snapshot = service.progress().snapshot();
                (snapshot, service.try_finish())
            })
        };
        let Some((snapshot, finished)) = snapshot else { return };

        if let Some(window) = self.window() {
            window.set_dup_stage(ss(snapshot.stage.clone()));
            window.set_dup_progress(ss(format!(
                "{} / {} hashed · {} · {} error(s)",
                format_count(snapshot.files_hashed),
                format_count(snapshot.total_candidates),
                format_size(snapshot.bytes_hashed),
                format_count(snapshot.errors)
            )));
            let percent = if snapshot.total_candidates > 0 {
                (snapshot.files_hashed as f32 / snapshot.total_candidates as f32) * 100.0
            } else {
                0.0
            };
            window.set_dup_progress_percent(percent.clamp(0.0, 100.0));
        }

        if let Some(groups) = finished {
            let service = self.dup_service.borrow_mut().take();
            drop(service);
            *self.dup_groups.borrow_mut() = groups;
            if let Some(window) = self.window() {
                window.set_dup_busy(false);
                window.set_dup_has_run(true);
                window.set_dup_stage(ss(if snapshot.stage == "Cancelled" {
                    "Cancelled"
                } else {
                    "Complete"
                }));
                window.set_dup_progress(ss(""));
                window.set_dup_progress_percent(100.0);
            }
            self.refresh_duplicates();
        }
    }

    fn refresh_duplicates(&self) {
        let Some(window) = self.window() else { return };
        let groups = self.dup_groups.borrow();
        let mut rows = Vec::new();
        let mut recoverable = 0u64;
        let mut shown_files = 0u64;
        for (group_index, group) in groups.iter().enumerate() {
            if group_index >= MAX_DUP_GROUPS {
                break;
            }
            recoverable = recoverable.saturating_add(group.recoverable);
            rows.push(DuplicateRow {
                group: group_index as i32,
                file: -1,
                is_header: true,
                name: ss(describe_group(group)),
                path: ss(format!("SHA-256 prefix {}", group.hash)),
                size: ss(if group.recoverable > 0 {
                    format!("{} recoverable", format_size(group.recoverable))
                } else {
                    "hard links only".to_string()
                }),
                detail: ss(if group.hard_link_count > 0 {
                    format!("{} hard link(s)", group.hard_link_count)
                } else {
                    String::new()
                }),
                can_recycle: group.recoverable > 0,
                selected: false,
            });
            for (file_index, file) in group.files.iter().take(MAX_DUP_FILES_PER_GROUP).enumerate() {
                shown_files += 1;
                rows.push(DuplicateRow {
                    group: group_index as i32,
                    file: file_index as i32,
                    is_header: false,
                    name: ss(Path::new(&file.path)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| file.path.clone())),
                    path: ss(file.path.clone()),
                    size: ss(format_size(file.size)),
                    detail: ss(if file.is_hard_link {
                        format!("hard link · {} links", file.link_count)
                    } else {
                        "independent copy".to_string()
                    }),
                    can_recycle: !file.is_hard_link,
                    selected: false,
                });
            }
        }
        drop(groups);

        let total_groups = self.dup_groups.borrow().len();
        window.set_dup_rows(model(rows));
        window.set_dup_summary(ss(if total_groups == 0 {
            "No duplicate groups found above the configured minimum size.".to_string()
        } else if total_groups > MAX_DUP_GROUPS {
            format!(
                "{} duplicate groups · showing the first {}",
                format_count(total_groups as u64),
                format_count(MAX_DUP_GROUPS as u64)
            )
        } else if total_groups == 1 {
            "1 duplicate group".to_string()
        } else {
            format!("{} duplicate groups", format_count(total_groups as u64))
        }));
        window.set_dup_total(ss(if total_groups == 0 {
            String::new()
        } else {
            format!(
                "up to {} potentially recoverable · {} rows shown",
                format_size(recoverable),
                format_count(shown_files)
            )
        }));
    }

    fn dup_recycle(&self, group_index: i32, file_index: i32) {
        let Some(result) = self.current_result() else { return };
        if group_index < 0 || file_index < 0 {
            return;
        }
        let path = {
            let groups = self.dup_groups.borrow();
            groups
                .get(group_index as usize)
                .and_then(|g| g.files.get(file_index as usize))
                .map(|f| (f.path.clone(), f.size, f.is_hard_link))
        };
        let Some((path, size, is_hard_link)) = path else { return };
        if is_hard_link {
            self.toast(
                "This is a hard link to the same file. Removing it does not free space.",
                true,
            );
            return;
        }
        let _ = result;
        if self.settings.borrow().confirm_destructive {
            let confirmation = describe_delete(&[Path::new(&path)], &[size], false);
            self.show_dialog(
                &confirmation.title,
                &confirmation.message,
                "Move to Recycle Bin",
                "Cancel",
                false,
                false,
                "",
                "",
                Pending::DupRecycle(group_index as usize, file_index as usize),
            );
        } else {
            self.perform_dup_recycle(group_index as usize, file_index as usize);
        }
    }

    fn perform_dup_recycle(&self, group_index: usize, file_index: usize) {
        let path = {
            let groups = self.dup_groups.borrow();
            groups
                .get(group_index)
                .and_then(|g| g.files.get(file_index))
                .map(|f| (f.path.clone(), f.node))
        };
        let Some((path, node)) = path else { return };
        match clipboard::recycle_items(&[PathBuf::from(&path)]) {
            Ok(outcome) if outcome.failed.is_empty() => {
                self.deleted.borrow_mut().insert(node);
                {
                    let mut groups = self.dup_groups.borrow_mut();
                    if let Some(group) = groups.get_mut(group_index) {
                        group.files.remove(file_index);
                        if group.files.len() < 2 {
                            groups.remove(group_index);
                        }
                    }
                }
                self.refresh_duplicates();
                self.refresh_explorer();
                self.refresh_largest();
                self.refresh_treemap();
                self.toast(
                    "Moved to the Recycle Bin. Press F5 to refresh cached totals.",
                    false,
                );
            }
            Ok(outcome) => {
                let message = outcome
                    .failed
                    .first()
                    .map(|(_, error)| error.clone())
                    .unwrap_or_else(|| "Unknown error".into());
                self.toast(&message, true);
            }
            Err(error) => self.toast(&error.to_string(), true),
        }
    }

    fn dup_recycle_extras(&self, group_index: i32) {
        if group_index < 0 {
            return;
        }
        let group_index = group_index as usize;
        let paths: Vec<(PathBuf, u64, NodeId)> = {
            let groups = self.dup_groups.borrow();
            groups
                .get(group_index)
                .map(|g| {
                    g.files
                        .iter()
                        .skip(1)
                        .filter(|f| !f.is_hard_link)
                        .map(|f| (PathBuf::from(&f.path), f.size, f.node))
                        .collect()
                })
                .unwrap_or_default()
        };
        if paths.is_empty() {
            self.toast("There are no redundant physical copies to remove.", true);
            return;
        }
        let sizes: Vec<u64> = paths.iter().map(|(_, size, _)| *size).collect();
        let refs: Vec<&Path> = paths.iter().map(|(path, _, _)| path.as_path()).collect();
        let confirmation = describe_delete(&refs, &sizes, false);
        let node_ids: Vec<NodeId> = paths.iter().map(|(_, _, node)| *node).collect();
        let _ = node_ids;
        self.show_dialog(
            &confirmation.title,
            &confirmation.message,
            "Move extras to Recycle Bin",
            "Cancel",
            false,
            false,
            "",
            "",
            Pending::DupExtras(group_index),
        );
    }

    fn perform_dup_extras(&self, group_index: usize) {
        let (paths, nodes): (Vec<PathBuf>, Vec<NodeId>) = {
            let groups = self.dup_groups.borrow();
            groups
                .get(group_index)
                .map(|g| {
                    g.files
                        .iter()
                        .skip(1)
                        .filter(|f| !f.is_hard_link)
                        .map(|f| (PathBuf::from(&f.path), f.node))
                        .unzip()
                })
                .unwrap_or_default()
        };
        if paths.is_empty() {
            return;
        }
        match clipboard::recycle_items(&paths) {
            Ok(outcome) => {
                let failed_paths: HashSet<&Path> =
                    outcome.failed.iter().map(|(p, _)| p.as_path()).collect();
                let mut deleted = self.deleted.borrow_mut();
                for (path, node) in paths.iter().zip(nodes.iter()) {
                    if !failed_paths.contains(path.as_path()) {
                        deleted.insert(*node);
                    }
                }
                drop(deleted);
                {
                    let mut groups = self.dup_groups.borrow_mut();
                    if let Some(group) = groups.get_mut(group_index) {
                        group.files.truncate(1);
                    }
                    groups.retain(|g| g.files.len() >= 2);
                }
                self.refresh_duplicates();
                self.refresh_explorer();
                self.refresh_largest();
                self.refresh_treemap();
                self.toast(
                    &format!(
                        "{} item(s) moved to the Recycle Bin. Press F5 to refresh cached totals.",
                        outcome.succeeded.len()
                    ),
                    outcome.failed.is_empty(),
                );
            }
            Err(error) => self.toast(&error.to_string(), true),
        }
    }

    fn dup_reveal(&self, group_index: i32, file_index: i32) {
        let groups = self.dup_groups.borrow();
        let path = groups
            .get(group_index.max(0) as usize)
            .and_then(|g| {
                if file_index >= 0 {
                    g.files.get(file_index as usize)
                } else {
                    g.files.first()
                }
            })
            .map(|f| f.path.clone());
        drop(groups);
        if let Some(path) = path {
            if let Err(error) = explorer::reveal(Path::new(&path)) {
                self.toast(&error, true);
            }
        }
    }

    // ---------------------------------------------------------------- info

    fn refresh_info(&self) {
        let Some(window) = self.window() else { return };
        let Some(result) = self.current_result() else {
            window.set_info_visible(false);
            return;
        };
        let id = self.selected.get();
        if id < 0 {
            window.set_info_visible(false);
            window.set_info_node(-1);
            return;
        }
        let node_id = id as NodeId;
        let Some(node) = result.node(node_id) else {
            window.set_info_visible(false);
            return;
        };
        let path = result.path_of(node_id);
        let classification = classify_detailed(
            Path::new(&path),
            &node.name,
            node.is_dir,
            node.attributes,
        );
        let risk = assess_detailed(
            Path::new(&path),
            &node.name,
            node.is_dir,
            node.attributes,
            node.size,
            node.modified,
            node.category,
            now_unix(),
        );
        let kind = if node.is_dir {
            "Folder".to_string()
        } else {
            let extension = node.extension();
            if extension.is_empty() {
                "File".to_string()
            } else {
                format!("{} file", extension.to_ascii_uppercase())
            }
        };
        let mut evidence: Vec<String> = Vec::new();
        for line in classification.evidence.iter().chain(risk.evidence.iter()) {
            if !evidence.contains(line) {
                evidence.push(line.clone());
            }
            if evidence.len() >= 14 {
                break;
            }
        }

        window.set_info_node(id);
        window.set_info_name(ss(node.name.clone()));
        window.set_info_path(ss(path.clone()));
        window.set_info_kind(ss(format!(
            "{} · {}",
            kind,
            if node.is_dir {
                format!(
                    "{} files · {} folders",
                    format_count(node.file_count),
                    format_count(node.dir_count)
                )
            } else {
                format_size_exact(node.size)
            }
        )));
        window.set_info_size(ss(format!(
            "{}  ·  {} on disk",
            format_size(node.size),
            format_size(node.allocated)
        )));
        window.set_info_modified(ss(format!(
            "Modified {}",
            node.modified
                .map(format_timestamp)
                .unwrap_or_else(|| "unknown".into())
        )));
        window.set_info_category(ss(node.category.label()));
        window.set_info_category_color(rgb(node.category.color()));
        window.set_info_confidence(ss(format!("{} confidence", node.confidence.label())));
        window.set_info_risk(ss(risk.level.label()));
        window.set_info_risk_color(risk_color(risk.level));
        window.set_info_summary(ss(risk.summary.clone()));
        window.set_info_consequences(ss(risk.consequences.clone()));
        window.set_info_recommendation(ss(risk.recommendation.clone()));
        window.set_info_missing(ss(risk.missing_evidence.clone().unwrap_or_default()));
        let evidence_rows: Vec<EvidenceRow> = evidence
            .into_iter()
            .map(|text| EvidenceRow { text: ss(text) })
            .collect();
        window.set_info_evidence(model(evidence_rows));
        window.set_info_actions_enabled(Path::new(&path).exists() && node_id != result.root);
        window.set_info_visible(true);
        if window.get_show_info() {
            window.set_info_panel_open(true);
        }
    }

    // ---------------------------------------------------------------- file actions

    fn selected_path(&self) -> Option<(PathBuf, u64, bool)> {
        let result = self.current_result()?;
        let id = self.selected.get();
        if id < 0 {
            return None;
        }
        let node = result.node(id as NodeId)?;
        Some((PathBuf::from(result.path_of(id as NodeId)), node.size, node.is_dir))
    }

    fn open_selected(&self) {
        let Some((path, _, is_dir)) = self.selected_path() else {
            return;
        };
        if is_dir {
            self.activate_node(self.selected.get());
            return;
        }
        if let Err(error) = explorer::open(&path) {
            self.toast(&error, true);
        }
    }

    fn reveal_selected(&self) {
        let Some((path, _, _)) = self.selected_path() else {
            return;
        };
        if let Err(error) = explorer::reveal(&path) {
            self.toast(&error, true);
        }
    }

    fn properties_selected(&self) {
        let Some((path, _, _)) = self.selected_path() else {
            return;
        };
        if let Err(error) = explorer::properties(&path) {
            self.toast(&error, true);
        }
    }

    fn copy_selected(&self, cut: bool) {
        let Some((path, _, _)) = self.selected_path() else {
            return;
        };
        if let Err(error) = clipboard::copy_paths_text(std::slice::from_ref(&path)) {
            self.toast(&error, true);
        }
        *self.clipboard.borrow_mut() = if cut {
            FileClipboard::cut(vec![path])
        } else {
            FileClipboard::copy(vec![path])
        };
        self.toast(
            if cut {
                "Path copied. Use Paste to move it, or paste in Explorer."
            } else {
                "Path copied. Use Paste to copy it, or paste in Explorer."
            },
            false,
        );
    }

    fn paste_into_current(&self) {
        if self.clipboard.borrow().is_empty() {
            self.toast("The application clipboard is empty. Copy or cut an item first.", true);
            return;
        }
        let Some(result) = self.current_result() else {
            return;
        };
        let folder = self.current_folder.get();
        let destination = PathBuf::from(result.path_of(folder));
        if !destination.is_dir() {
            self.toast("The destination folder no longer exists.", true);
            return;
        }
        if self.file_op.borrow().is_some() {
            self.toast("A file operation is still running. Wait for it to finish.", true);
            return;
        }
        // The Windows shell shows its own progress and conflict UI and blocks
        // the calling thread for the duration, so run it off the UI thread.
        let clipboard = self.clipboard.borrow().clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let outcome = match clipboard.paste_into(&destination) {
                None => Err("Nothing to paste.".to_string()),
                Some(Ok(outcome)) => Ok(outcome),
                Some(Err(error)) => Err(error.to_string()),
            };
            let _ = sender.send((clipboard.cut, outcome));
        });
        *self.file_op.borrow_mut() = Some(receiver);
        self.toast("File operation started. The Windows shell may show progress.", false);
    }

    fn rename_selected(&self) {
        let Some((path, _, _)) = self.selected_path() else {
            return;
        };
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let title = "Rename item";
        let message = format!(
            "Enter a new name for:\n{}",
            path.to_string_lossy()
        );
        let id = self.selected.get();
        self.show_dialog(
            title,
            &message,
            "Rename",
            "Cancel",
            false,
            true,
            "New name",
            &name,
            Pending::Rename(id),
        );
    }

    fn recycle_selected(&self) {
        let Some((path, size, _)) = self.selected_path() else {
            self.toast("Select an item first.", true);
            return;
        };
        let id = self.selected.get();
        if self.settings.borrow().confirm_destructive {
            let confirmation = describe_delete(&[Path::new(&path)], &[size], false);
            self.show_dialog(
                &confirmation.title,
                &confirmation.message,
                "Move to Recycle Bin",
                "Cancel",
                false,
                false,
                "",
                "",
                Pending::Recycle(id),
            );
        } else {
            self.recycle_node(id, &path);
        }
    }

    fn recycle_node(&self, id: i32, path: &Path) {
        match clipboard::recycle_items(&[path.to_path_buf()]) {
            Ok(outcome) if outcome.failed.is_empty() => {
                self.mark_deleted(id);
                self.toast(
                    "Moved to the Recycle Bin. Press F5 to refresh totals.",
                    false,
                );
            }
            Ok(outcome) => {
                let message = outcome
                    .failed
                    .first()
                    .map(|(_, error)| error.clone())
                    .unwrap_or_else(|| "The item could not be recycled.".into());
                self.toast(&message, true);
            }
            Err(error) => self.toast(&error.to_string(), true),
        }
    }

    fn delete_selected(&self) {
        let Some((path, size, _)) = self.selected_path() else {
            self.toast("Select an item first.", true);
            return;
        };
        let id = self.selected.get();
        let confirmation = describe_delete(&[Path::new(&path)], &[size], true);
        self.show_dialog(
            &confirmation.title,
            &confirmation.message,
            "Delete permanently",
            "Cancel",
            true,
            false,
            "",
            "",
            Pending::Delete(id),
        );
    }

    fn delete_node(&self, id: i32, path: &Path) {
        match operations::delete_permanent(&[path.to_path_buf()]) {
            Ok(outcome) if outcome.failed.is_empty() => {
                self.mark_deleted(id);
                self.toast("Deleted permanently. Press F5 to refresh totals.", false);
            }
            Ok(outcome) => {
                let message = outcome
                    .failed
                    .first()
                    .map(|(_, error)| error.clone())
                    .unwrap_or_else(|| "The item could not be deleted.".into());
                self.toast(&message, true);
            }
            Err(error) => self.toast(&error.to_string(), true),
        }
    }

    fn mark_deleted(&self, id: i32) {
        if id < 0 {
            return;
        }
        let id = id as NodeId;
        self.deleted.borrow_mut().insert(id);
        if self.selected.get() == id as i32 {
            self.selected.set(-1);
        }
        self.refresh_explorer();
        self.refresh_largest();
        self.refresh_search();
        self.refresh_treemap();
        self.refresh_file_types();
        self.refresh_info();
        self.refresh_duplicates();
    }

    // ---------------------------------------------------------------- context menu

    fn show_context(&self, id: i32, x: f32, y: f32) {
        let Some(window) = self.window() else { return };
        if id >= 0 {
            self.selected.set(id);
            self.refresh_info();
            self.refresh_explorer();
            self.refresh_treemap();
            self.refresh_largest();
            self.refresh_search();
        }
        let Some(result) = self.current_result() else { return };
        let id = id.max(0) as NodeId;
        let Some(node) = result.node(id) else { return };
        let path = result.path_of(id);
        let exists = Path::new(&path).exists();
        let actions = vec![
            ContextAction {
                id: ACTION_OPEN,
                label: ss(if node.is_dir {
                    "Open folder"
                } else {
                    "Open with default app"
                }),
                enabled: exists && self.exists(id as i32),
                destructive: false,
                separator_before: false,
            },
            ContextAction {
                id: ACTION_REVEAL,
                label: ss("Show in Explorer"),
                enabled: exists,
                destructive: false,
                separator_before: false,
            },
            ContextAction {
                id: ACTION_COPY,
                label: ss("Copy"),
                enabled: true,
                destructive: false,
                separator_before: true,
            },
            ContextAction {
                id: ACTION_CUT,
                label: ss("Cut"),
                enabled: true,
                destructive: false,
                separator_before: false,
            },
            ContextAction {
                id: ACTION_COPY_PATH,
                label: ss("Copy path"),
                enabled: true,
                destructive: false,
                separator_before: false,
            },
            ContextAction {
                id: ACTION_RENAME,
                label: ss("Rename…"),
                enabled: exists && id != result.root,
                destructive: false,
                separator_before: true,
            },
            ContextAction {
                id: ACTION_RECYCLE,
                label: ss("Move to Recycle Bin"),
                enabled: exists && id != result.root,
                destructive: true,
                separator_before: true,
            },
            ContextAction {
                id: ACTION_DELETE,
                label: ss("Delete permanently…"),
                enabled: exists && id != result.root,
                destructive: true,
                separator_before: false,
            },
            ContextAction {
                id: ACTION_PROPERTIES,
                label: ss("Properties"),
                enabled: exists,
                destructive: false,
                separator_before: true,
            },
            ContextAction {
                id: ACTION_SCAN_FOLDER,
                label: ss(if node.is_dir {
                    "Scan this folder"
                } else {
                    "Scan containing folder"
                }),
                enabled: true,
                destructive: false,
                separator_before: true,
            },
        ];
        let height = 8.0
            + 24.0
            + 16.0
            + actions
                .iter()
                .map(|a| if a.separator_before { 31.0 } else { 29.0 })
                .sum::<f32>();
        window.set_context_title(ss(node.name.clone()));
        window.set_context_subtitle(ss(path));
        window.set_context_actions(model(actions));
        window.set_context_height(height as f32);
        window.set_context_x(x);
        window.set_context_y(y);
        window.set_context_visible(true);
    }

    fn context_action(&self, action_id: i32) {
        if let Some(window) = self.window() {
            window.set_context_visible(false);
        }
        match action_id {
            ACTION_OPEN => self.open_selected(),
            ACTION_REVEAL => self.reveal_selected(),
            ACTION_COPY => self.copy_selected(false),
            ACTION_CUT => self.copy_selected(true),
            ACTION_COPY_PATH => {
                if let Some((path, _, _)) = self.selected_path() {
                    match clipboard::copy_path_text(&path) {
                        Ok(()) => self.toast("Path copied to the clipboard.", false),
                        Err(error) => self.toast(&error, true),
                    }
                }
            }
            ACTION_RENAME => self.rename_selected(),
            ACTION_RECYCLE => self.recycle_selected(),
            ACTION_DELETE => self.delete_selected(),
            ACTION_PROPERTIES => self.properties_selected(),
            ACTION_SCAN_FOLDER => {
                if let Some((path, _, is_dir)) = self.selected_path() {
                    let root = if is_dir {
                        path
                    } else {
                        path.parent().map(Path::to_path_buf).unwrap_or(path)
                    };
                    if let Some(window) = self.window() {
                        window.set_path_text(ss(normalize_display_root(&root)));
                    }
                    self.start_scan(root);
                }
            }
            _ => {}
        }
    }

    // ---------------------------------------------------------------- dialogs

    #[allow(clippy::too_many_arguments)]
    fn show_dialog(
        &self,
        title: &str,
        message: &str,
        primary: &str,
        secondary: &str,
        danger: bool,
        input: bool,
        input_label: &str,
        input_text: &str,
        pending: Pending,
    ) {
        let Some(window) = self.window() else { return };
        window.set_dialog_title(ss(title));
        window.set_dialog_message(ss(message));
        window.set_dialog_primary(ss(primary));
        window.set_dialog_secondary(ss(secondary));
        window.set_dialog_danger(danger);
        window.set_dialog_input(input);
        window.set_dialog_input_label(ss(input_label));
        window.set_dialog_input_text(ss(input_text));
        window.set_dialog_visible(true);
        *self.pending.borrow_mut() = Some(pending);
    }

    fn dialog_cancelled(&self) {
        *self.pending.borrow_mut() = None;
        if let Some(window) = self.window() {
            window.set_dialog_visible(false);
        }
    }

    fn dialog_accepted(&self, text: String) {
        let pending = self.pending.borrow_mut().take();
        if let Some(window) = self.window() {
            window.set_dialog_visible(false);
        }
        let Some(pending) = pending else { return };
        match pending {
            Pending::Rename(id) => {
                if id < 0 {
                    return;
                }
                let Some(result) = self.current_result() else { return };
                let path = PathBuf::from(result.path_of(id as NodeId));
                match operations::rename_item(&path, &text) {
                    Ok(_) => {
                        self.toast("Renamed. Press F5 to refresh the scan.", false);
                        self.mark_stale_after_change();
                    }
                    Err(error) => self.toast(&error.to_string(), true),
                }
            }
            Pending::Recycle(id) => {
                if let Some((path, _, _)) = self.selected_path() {
                    self.recycle_node(id, &path);
                }
            }
            Pending::Delete(id) => {
                if let Some((path, _, _)) = self.selected_path() {
                    self.delete_node(id, &path);
                }
            }
            Pending::Cleanup(plan) => self.perform_cleanup(plan),
            Pending::DupRecycle(group, file) => self.perform_dup_recycle(group, file),
            Pending::DupExtras(group) => self.perform_dup_extras(group),
            Pending::ClearData => {
                match self.db.clear_all_scan_data() {
                    Ok(()) => {
                        let _ = self.db.vacuum();
                        self.refresh_history();
                        self.refresh_settings_props();
                        self.toast("Local scan history and recent locations cleared.", false);
                    }
                    Err(error) => self.toast(&error.to_string(), true),
                }
            }
        }
    }

    fn mark_stale_after_change(&self) {
        self.refresh_explorer();
        self.refresh_largest();
        self.refresh_search();
        self.refresh_treemap();
        self.refresh_file_types();
        self.refresh_info();
    }

    // ---------------------------------------------------------------- history/settings

    fn refresh_history(&self) {
        let Some(window) = self.window() else { return };
        let entries = self.db.history(200).unwrap_or_default();
        let rows: Vec<HistoryRow> = entries
            .iter()
            .map(|entry| HistoryRow {
                id: entry.id as i32,
                path: ss(entry.path.clone()),
                when: ss(format_timestamp(entry.started_at)),
                size: ss(format_size(entry.total_size)),
                files: ss(format_count(entry.files)),
                status: ss(crate::storage::database::status_label(&entry.status)),
            })
            .collect();
        window.set_history_empty(rows.is_empty());
        window.set_history_rows(model(rows));
        *self.history.borrow_mut() = entries;
    }

    fn history_rescan(&self, id: i32) {
        let path = self
            .history
            .borrow()
            .iter()
            .find(|entry| entry.id == id as i64)
            .map(|entry| entry.path.clone());
        if let Some(path) = path {
            if let Some(window) = self.window() {
                window.set_path_text(ss(path.clone()));
            }
            self.start_scan_from_text(&path);
        }
    }

    fn history_clear(&self) {
        match self.db.clear_history() {
            Ok(()) => {
                self.refresh_history();
                self.toast("Scan history cleared. Settings and recent locations were kept.", false);
            }
            Err(error) => self.toast(&error.to_string(), true),
        }
    }

    fn refresh_settings_props(&self) {
        let Some(window) = self.window() else { return };
        window.set_settings_dark(self.settings.borrow().is_dark());
        window.set_settings_low_resource(self.settings.borrow().low_resource);
        window.set_settings_follow_reparse(self.settings.borrow().follow_reparse_points);
        window.set_settings_use_recycle(self.settings.borrow().use_recycle_bin);
        window.set_settings_confirm_destructive(self.settings.borrow().confirm_destructive);
        window.set_settings_remember_location(self.settings.borrow().remember_last_location);
        window.set_settings_workers_text(ss(self.settings.borrow().workers.to_string()));
        window.set_settings_exclusions_text(ss(self.settings.borrow().exclusions_text()));
        window.set_settings_min_dup_text(ss(
            (self.settings.borrow().min_duplicate_size / (1024 * 1024)).max(1).to_string(),
        ));
        window.set_db_path(ss(self.db_path_text.clone()));
        window.set_db_size(ss(format_size(self.db.size_bytes())));
    }

    fn settings_save(&self) {
        let Some(window) = self.window() else { return };
        let workers = window
            .get_settings_workers_text()
            .trim()
            .parse::<usize>()
            .unwrap_or(0)
            .min(32);
        let min_dup_mb = window
            .get_settings_min_dup_text()
            .trim()
            .parse::<u64>()
            .unwrap_or(1)
            .clamp(1, 1024);
        let saved = {
            let mut settings = self.settings.borrow_mut();
            settings.theme = if window.get_settings_dark() {
                "dark".into()
            } else {
                "light".into()
            };
            settings.low_resource = window.get_settings_low_resource();
            settings.follow_reparse_points = window.get_settings_follow_reparse();
            settings.use_recycle_bin = window.get_settings_use_recycle();
            settings.confirm_destructive = window.get_settings_confirm_destructive();
            settings.show_info_panel = window.get_show_info();
            settings.remember_last_location = window.get_settings_remember_location();
            settings.workers = workers;
            settings.min_duplicate_size = min_dup_mb * 1024 * 1024;
            settings.set_exclusions_text(&window.get_settings_exclusions_text());
            self.db.save_settings(&settings)
        };
        match saved {
            Ok(()) => {
                window.set_dark(self.settings.borrow().is_dark());
                self.refresh_settings_props();
                window.set_plan_recycle(self.settings.borrow().use_recycle_bin);
                self.toast("Settings saved locally.", false);
            }
            Err(error) => self.toast(&error.to_string(), true),
        }
    }

    fn open_data_folder(&self) {
        match default_data_dir() {
            Ok(dir) => {
                if let Err(error) = shell::open_folder(&dir) {
                    self.toast(&error, true);
                }
            }
            Err(error) => self.toast(&error.to_string(), true),
        }
    }

    fn clear_scan_data(&self) {
        self.show_dialog(
            "Clear local scan data",
            "This removes scan history and recent locations from the local database. It does not delete any files on disk. This action cannot be undone.",
            "Clear data",
            "Cancel",
            true,
            false,
            "",
            "",
            Pending::ClearData,
        );
    }

    // ---------------------------------------------------------------- toasts

    fn toast(&self, text: &str, error: bool) {
        let Some(window) = self.window() else { return };
        window.set_toast_text(ss(text));
        window.set_toast_error(error);
        window.set_toast_visible(true);
        self.toast_until
            .set(Some(Instant::now() + Duration::from_secs(if error { 7 } else { 5 })));
    }

    fn tick_toast(&self) {
        if let Some(deadline) = self.toast_until.get() {
            if Instant::now() >= deadline {
                self.toast_until.set(None);
                if let Some(window) = self.window() {
                    window.set_toast_visible(false);
                }
            }
        }
    }

    // ---------------------------------------------------------------- nav / sort

    fn nav(&self, page: i32) {
        let Some(window) = self.window() else { return };
        if !(0..=9).contains(&page) {
            return;
        }
        window.set_context_visible(false);
        window.set_page(page);
        match page {
            1 => self.refresh_explorer(),
            2 => self.refresh_treemap(),
            3 => self.refresh_largest(),
            4 => self.refresh_file_types(),
            5 => self.refresh_cleanup(),
            6 => self.refresh_duplicates(),
            8 => self.refresh_history(),
            9 => self.refresh_settings_props(),
            _ => {}
        }
    }

    fn sort_items(&self, column: i32) -> bool {
        let (current, asc) = self.item_sort.get();
        let next = if current == column { !asc } else { !matches!(column, 2 | 3 | 4) };
        self.item_sort.set((column, next));
        next
    }

    fn item_sort(&self, column: i32) {
        let asc = self.sort_items(column);
        if let Some(window) = self.window() {
            window.set_item_sort_column(column);
            window.set_item_sort_asc(asc);
        }
        self.refresh_explorer();
    }

    fn largest_sort(&self, column: i32) {
        let (current, asc) = self.largest_sort.get();
        let next = if current == column { !asc } else { !matches!(column, 2 | 3 | 4) };
        self.largest_sort.set((column, next));
        self.refresh_largest();
    }

    fn search_sort(&self, column: i32) {
        let (current, asc) = self.search_sort.get();
        let next = if current == column { !asc } else { !matches!(column, 2 | 3 | 4) };
        self.search_sort.set((column, next));
        self.refresh_search();
    }

    fn escape(&self) {
        if let Some(window) = self.window() {
            window.set_context_visible(false);
            window.set_info_visible(false);
        }
        self.selected.set(-1);
        self.refresh_explorer();
    }

    fn tick_file_op(&self) {
        let done = {
            let guard = self.file_op.borrow();
            let Some(receiver) = guard.as_ref() else {
                return;
            };
            match receiver.try_recv() {
                Ok(done) => Some(done),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some((
                    false,
                    Err("The file operation ended unexpectedly.".to_string()),
                )),
            }
        };
        let Some((cut, outcome)) = done else { return };
        *self.file_op.borrow_mut() = None;
        match outcome {
            Ok(outcome) => {
                if cut && outcome.ok() {
                    let mut clipboard = self.clipboard.borrow_mut();
                    clipboard.paths.clear();
                    clipboard.cut = false;
                }
                let message = format!(
                    "{} Press F5 to refresh the scan.",
                    outcome.summary()
                );
                self.toast(&message, !outcome.failed.is_empty() || outcome.cancelled);
            }
            Err(message) => self.toast(&message, true),
        }
    }

    fn tick(&self) {
        self.tick_toast();
        if self.scan.borrow().is_some() {
            self.tick_scan();
        }
        if self.dup_service.borrow().is_some() {
            self.tick_duplicates();
        }
        if self.file_op.borrow().is_some() {
            self.tick_file_op();
        }
    }
}

fn sort_ids(result: &ScanResult, ids: &mut [NodeId], column: i32, asc: bool) {
    ids.sort_by(|a, b| {
        let na = &result.nodes[*a as usize];
        let nb = &result.nodes[*b as usize];
        let ordering = match column {
            3 => na.allocated.cmp(&nb.allocated),
            4 => na.modified.cmp(&nb.modified),
            6 => na.risk.cmp(&nb.risk),
            _ => na.size.cmp(&nb.size),
        };
        let ordering = ordering.then_with(|| {
            na.name
                .to_ascii_lowercase()
                .cmp(&nb.name.to_ascii_lowercase())
        });
        if asc {
            ordering
        } else {
            ordering.reverse()
        }
    });
}

pub fn run() -> Result<(), slint::PlatformError> {
    let window = AppWindow::new()?;

    let (db, db_path_text) = match Database::open_default() {
        Ok(db) => {
            let path = db.path().to_string_lossy().into_owned();
            (db, path)
        }
        Err(error) => {
            log::error!("could not open the local database: {error}");
            let db = Database::in_memory().expect("in-memory database must open");
            let note = format!("Local database unavailable ({error}). Using a temporary database.");
            (db, note)
        }
    };
    let settings = db.load_settings();
    let drives = list_drives();
    let backend = Rc::new(Backend {
        window: window.as_weak(),
        db,
        settings: RefCell::new(settings),
        drives,
        scan: RefCell::new(None),
        result: RefCell::new(None),
        selected: Cell::new(-1),
        current_folder: Cell::new(INVALID_NODE),
        treemap_path: RefCell::new(Vec::new()),
        expanded: RefCell::new(HashSet::new()),
        deleted: RefCell::new(HashSet::new()),
        largest_ids: RefCell::new(Vec::new()),
        item_sort: Cell::new((2, false)),
        largest_sort: Cell::new((2, false)),
        search_sort: Cell::new((2, false)),
        search_ids: RefCell::new(Vec::new()),
        cleanup_groups: RefCell::new(Vec::new()),
        cleanup_selected: RefCell::new(HashSet::new()),
        cleanup_initialized: Cell::new(false),
        plan: RefCell::new(None),
        dup_service: RefCell::new(None),
        dup_groups: RefCell::new(Vec::new()),
        clipboard: RefCell::new(FileClipboard::default()),
        pending: RefCell::new(None),
        toast_until: Cell::new(None),
        history: RefCell::new(Vec::new()),
        suppress_drive_callback: Cell::new(false),
        startup_guard_until: Cell::new(None),
        db_path_text,
        explorer_model: RefCell::new(None),
        largest_model: RefCell::new(None),
        search_model: RefCell::new(None),
        treemap_model: RefCell::new(None),
        file_op: RefCell::new(None),
    });

    wire(&window, &backend);
    backend.initialize();

    let weak = Rc::downgrade(&backend);
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(140),
        move || {
            if let Some(backend) = weak.upgrade() {
                backend.tick();
            }
        },
    );

    let result = window.run();
    drop(timer);
    result
}

fn wire(window: &AppWindow, backend: &Rc<Backend>) {
    let b = backend.clone();
    window.on_drive_selected(move || b.drive_selected());

    let b = backend.clone();
    window.on_choose_folder(move || b.choose_folder());

    let b = backend.clone();
    window.on_scan_requested(move || b.scan_requested());

    let b = backend.clone();
    window.on_pause_requested(move || b.pause_requested());

    let b = backend.clone();
    window.on_cancel_requested(move || b.cancel_requested());

    let b = backend.clone();
    window.on_nav(move |page| b.nav(page));

    let b = backend.clone();
    window.on_rescan_requested(move || {
        if let Some(result) = b.current_result() {
            b.start_scan(PathBuf::from(result.root_path.clone()));
        } else {
            b.scan_requested();
        }
    });

    let b = backend.clone();
    window.on_go_up(move || b.go_up());

    let b = backend.clone();
    window.on_crumb_clicked(move |id| b.crumb_clicked(id));

    let b = backend.clone();
    window.on_tree_select(move |id, _additive| b.select_node(id, false));

    let b = backend.clone();
    window.on_tree_activate(move |id| b.activate_node(id));

    let b = backend.clone();
    window.on_tree_toggle(move |id| b.tree_toggle(id));

    let b = backend.clone();
    window.on_tree_context(move |id, x, y| b.show_context(id, x, y));

    let b = backend.clone();
    window.on_item_select(move |id, _additive| b.select_node(id, false));

    let b = backend.clone();
    window.on_item_activate(move |id| b.activate_node(id));

    let b = backend.clone();
    window.on_item_context(move |id, x, y| b.show_context(id, x, y));

    let b = backend.clone();
    window.on_treemap_select(move |id, _additive| b.select_node(id, false));

    let b = backend.clone();
    window.on_treemap_activate(move |id| b.treemap_zoom(id));

    let b = backend.clone();
    window.on_treemap_context(move |id, x, y| b.show_context(id, x, y));

    let b = backend.clone();
    window.on_filter_by_category(move |key| b.filter_by_category(key.to_string()));

    let b = backend.clone();
    window.on_top_level_clicked(move |id| b.top_level_clicked(id));

    let b = backend.clone();
    window.on_overview_open_largest(move |index, is_dir| b.overview_open_largest(index, is_dir));

    let b = backend.clone();
    window.on_item_sort(move |column| b.item_sort(column));

    let b = backend.clone();
    window.on_largest_sort(move |column| b.largest_sort(column));

    let b = backend.clone();
    window.on_search_sort(move |column| b.search_sort(column));

    let b = backend.clone();
    window.on_search_run(move || b.run_search());

    let b = backend.clone();
    window.on_search_reset(move || b.search_reset());

    let b = backend.clone();
    window.on_cleanup_toggle(move |key, checked| b.cleanup_toggle(key.to_string(), checked));

    let b = backend.clone();
    window.on_cleanup_select_all(move || b.cleanup_select_all());

    let b = backend.clone();
    window.on_cleanup_clear(move || b.cleanup_clear());

    let b = backend.clone();
    window.on_cleanup_preview(move || b.cleanup_preview());

    let b = backend.clone();
    window.on_cleanup_close_plan(move || b.cleanup_close_plan());

    let b = backend.clone();
    window.on_cleanup_confirm_plan(move || b.cleanup_confirm_plan());

    let b = backend.clone();
    window.on_plan_recycle_toggled(move |recycle| b.plan_recycle_toggled(recycle));

    let b = backend.clone();
    window.on_duplicates_scan(move || b.duplicates_scan());

    let b = backend.clone();
    window.on_duplicates_cancel(move || b.duplicates_cancel());

    let b = backend.clone();
    window.on_dup_recycle(move |group, file| b.dup_recycle(group, file));

    let b = backend.clone();
    window.on_dup_recycle_extras(move |group| b.dup_recycle_extras(group));

    let b = backend.clone();
    window.on_dup_reveal(move |group, file| b.dup_reveal(group, file));

    let b = backend.clone();
    window.on_history_rescan(move |id| b.history_rescan(id));

    let b = backend.clone();
    window.on_history_clear(move || b.history_clear());

    let b = backend.clone();
    window.on_settings_save(move || b.settings_save());

    let b = backend.clone();
    window.on_open_data_folder(move || b.open_data_folder());

    let b = backend.clone();
    window.on_clear_scan_data(move || b.clear_scan_data());

    let b = backend.clone();
    window.on_open_selected(move || b.open_selected());

    let b = backend.clone();
    window.on_reveal_selected(move || b.reveal_selected());

    let b = backend.clone();
    window.on_properties_selected(move || b.properties_selected());

    let b = backend.clone();
    window.on_copy_selected(move || b.copy_selected(false));

    let b = backend.clone();
    window.on_cut_selected(move || b.copy_selected(true));

    let b = backend.clone();
    window.on_paste_into_current(move || b.paste_into_current());

    let b = backend.clone();
    window.on_rename_selected(move || b.rename_selected());

    let b = backend.clone();
    window.on_recycle_selected(move || b.recycle_selected());

    let b = backend.clone();
    window.on_delete_selected(move || b.delete_selected());

    let b = backend.clone();
    window.on_info_close(move || {
        if let Some(window) = b.window() {
            window.set_info_visible(false);
            window.set_info_panel_open(false);
        }
    });

    let b = backend.clone();
    window.on_context_close(move || {
        if let Some(window) = b.window() {
            window.set_context_visible(false);
        }
    });

    let b = backend.clone();
    window.on_context_action(move |id| b.context_action(id));

    let b = backend.clone();
    window.on_dialog_cancelled(move || b.dialog_cancelled());

    let b = backend.clone();
    window.on_dialog_accepted(move |text| b.dialog_accepted(text.to_string()));

    let b = backend.clone();
    window.on_toast_dismiss(move || {
        b.toast_until.set(None);
        if let Some(window) = b.window() {
            window.set_toast_visible(false);
        }
    });

    let b = backend.clone();
    window.on_escape_pressed(move || b.escape());
}
