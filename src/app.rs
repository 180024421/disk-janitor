//! 主界面 v0.7.0：排除列表 / 极速扫描 / 深度卸载 / 计划任务 / 安全粉碎

use crate::about::{self, AboutAssets, AboutPanel};
use crate::admin::{is_elevated, relaunch_as_admin};
use crate::checkpoint::ScanCheckpoint;
use crate::deep_uninstall::{
    install_paths_of, list_appx_packages, list_locking_processes, list_related_services,
    list_related_tasks, tokens_from_name, uninstall_appx, AppxPackage, LockingProcess,
    RelatedService, RelatedTask,
};
use crate::drives::{list_drive_infos, DriveInfo};
use crate::duplicates::{apply_keep_strategy, find_duplicates, DupGroup, KeepStrategy};
use crate::export::{
    compare_with_snapshot, default_export_dir, export_csv, export_json, load_snapshot, save_snapshot,
    DiffItem,
};
use crate::fast_scan;
use crate::file_types::{filter_files, FileKind};
use crate::junk::{
    apply_safe_selection, junk_selected_paths, reclaimable_estimate, safe_junk_hits,
    safe_selected_size, scan_junk, JunkHit,
};
use crate::leftovers::{
    scan_leftovers, scan_leftovers_for_app, AppLeftoverHint, Confidence, LeftoverHit,
};
use crate::paths_ui;
use crate::whitelist::{filter_whitelisted, LeftoverWhitelist};
use crate::model::{
    format_bytes, format_delta, format_mtime, list_drives, sort_entries, FsEntry, ScanIndex, SortDir,
    SortKey,
};
use crate::orphans::{delete_orphan_keys, scan_orphan_uninstall_keys, OrphanReg};
use crate::scan::{expand_count_only_dir, resume_scan, scan_path_ex, ScanEvent, ScanOptions, ScanProgress};
use crate::schedule;
use crate::treemap;
use crate::shortcuts::{scan_broken_shortcuts, BrokenShortcut};
use crate::software::{launch_uninstall, list_installed_apps, InstalledApp};
use crate::startup::{
    delete_startup, disable_startup, enable_startup, list_startup_items, StartupItem,
};
use crate::theme::{self, ACCENT, DANGER, MUTED, OK, WARN};
use crate::trash_ops::{
    any_sensitive, clean_junk_paths, empty_recycle_bin, format_trash_errors, move_to_trash,
    move_to_trash_with, recycle_bin_size, DeleteOptions, TrashResult,
};
use crate::updater::{
    check_update, download_and_apply, open_url, AppConfig, UpdateCheck, APP_VERSION_CODE,
    APP_VERSION_NAME, DEFAULT_API_BASE,
};
use eframe::egui;
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Overview,
    Browse,
    TopN,
    Types,
    Junk,
    Software,
    Startup,
    Shortcuts,
    Registry,
    Duplicates,
    Tools,
    Settings,
    About,
}

enum WorkerMsg {
    Progress(ScanProgress),
    Partial(ScanIndex),
    Done(ScanIndex),
    JunkDone(Vec<JunkHit>),
    JunkCleanDone(TrashResult),
    DeleteDone(TrashResult),
    SoftwareDone(Vec<InstalledApp>),
    StartupDone(Vec<StartupItem>),
    ShortcutsDone(Vec<BrokenShortcut>),
    OrphansDone(Vec<OrphanReg>),
    UpdateDone(UpdateCheck),
    UpdateApplied(Result<String, String>),
    DuplicatesDone(Vec<DupGroup>),
    LeftoversDone(Vec<LeftoverHit>),
    DeepLockDone(Vec<LockingProcess>),
    DeepSvcDone(Vec<RelatedService>, Vec<RelatedTask>),
    AppxDone(Vec<AppxPackage>),
}

pub struct JanitorApp {
    drives: Vec<PathBuf>,
    drive_infos: Vec<DriveInfo>,
    root_input: String,
    current_dir: PathBuf,
    index: Option<ScanIndex>,
    sort_key: SortKey,
    sort_dir: SortDir,
    filter: String,
    selected: HashSet<String>,
    status: String,
    scanning: bool,
    progress: Option<ScanProgress>,
    cancel: Option<Arc<AtomicBool>>,
    rx: Option<Receiver<WorkerMsg>>,
    _worker: Option<JoinHandle<()>>,
    update_rx: Option<Receiver<WorkerMsg>>,
    update_worker: Option<JoinHandle<()>>,
    confirm_delete: bool,
    confirm_sensitive: bool,
    last_error: String,
    tab: Tab,
    tree_expanded: HashSet<String>,
    junk_hits: Vec<JunkHit>,
    junk_scanning: bool,
    junk_cleaning: bool,
    confirm_junk: bool,
    apps: Vec<InstalledApp>,
    app_filter: String,
    apps_loading: bool,
    startup_items: Vec<StartupItem>,
    startup_loading: bool,
    startup_filter: String,
    broken_shortcuts: Vec<BrokenShortcut>,
    shortcuts_scanning: bool,
    shortcut_scan_started: Option<Instant>,
    confirm_shortcuts: bool,
    orphans: Vec<OrphanReg>,
    orphans_scanning: bool,
    confirm_orphans: bool,
    prefer_quiet_uninstall: bool,
    list_limit: usize,
    list_total: usize,
    op_log: VecDeque<String>,
    show_log: bool,
    deleting: bool,
    config: AppConfig,
    update_status: String,
    update_checking: bool,
    pending_update: Option<crate::updater::RemoteManifest>,
    /// 回收站失败时是否允许直接删除（确认框勾选）
    allow_permanent_delete: bool,
    file_kind: FileKind,
    type_min_mb: u64,
    dup_groups: Vec<DupGroup>,
    dup_scanning: bool,
    confirm_dup_delete: bool,
    leftovers: Vec<LeftoverHit>,
    leftovers_scanning: bool,
    confirm_leftovers: bool,
    /// 卸载后跟扫线索
    pending_followup: Option<AppLeftoverHint>,
    leftovers_is_followup: bool,
    recycle_bin_label: String,
    confirm_empty_recycle: bool,
    diff_items: Vec<DiffItem>,
    is_admin: bool,
    whitelist: LeftoverWhitelist,
    confirm_safe_clean: bool,
    show_recycle_hint: bool,
    awaiting_uninstall_done: Option<AppLeftoverHint>,
    scan_paused_root: Option<String>,
    about_panel: AboutPanel,
    about_assets: Option<AboutAssets>,
    about_tip: String,
    expand_scanning: Option<PathBuf>,
    quiet_clean_started: bool,
    /// 多盘勾选（根路径字符串）
    selected_drives: HashSet<String>,
    /// 排队扫描的盘根
    drive_scan_queue: VecDeque<PathBuf>,
    /// 当前扫描是否极速
    scan_use_turbo: bool,
    exclude_edit: String,
    shred_delete: bool,
    locking_procs: Vec<LockingProcess>,
    related_services: Vec<RelatedService>,
    related_tasks: Vec<RelatedTask>,
    appx_packages: Vec<AppxPackage>,
    deep_loading: bool,
    appx_loading: bool,
    appx_filter: String,
    schedule_status: String,
}

impl Default for JanitorApp {
    fn default() -> Self {
        let drives = list_drives();
        let drive_infos = list_drive_infos();
        let start = dirs_fallback();
        Self {
            drives,
            drive_infos,
            root_input: start.display().to_string(),
            current_dir: start,
            index: None,
            sort_key: SortKey::Size,
            sort_dir: SortDir::Desc,
            filter: String::new(),
            selected: HashSet::new(),
            status: "欢迎使用大帅清理器。可从「总览」选盘扫描，或用工具清理残留。删除默认进回收站。"
                .into(),
            scanning: false,
            progress: None,
            cancel: None,
            rx: None,
            _worker: None,
            update_rx: None,
            update_worker: None,
            confirm_delete: false,
            confirm_sensitive: false,
            last_error: String::new(),
            tab: Tab::Overview,
            tree_expanded: HashSet::new(),
            junk_hits: Vec::new(),
            junk_scanning: false,
            junk_cleaning: false,
            confirm_junk: false,
            apps: Vec::new(),
            app_filter: String::new(),
            apps_loading: false,
            startup_items: Vec::new(),
            startup_loading: false,
            startup_filter: String::new(),
            broken_shortcuts: Vec::new(),
            shortcuts_scanning: false,
            shortcut_scan_started: None,
            confirm_shortcuts: false,
            orphans: Vec::new(),
            orphans_scanning: false,
            confirm_orphans: false,
            prefer_quiet_uninstall: false,
            list_limit: 100,
            list_total: 0,
            op_log: VecDeque::new(),
            show_log: true,
            deleting: false,
            config: AppConfig::load(),
            update_status: String::new(),
            update_checking: false,
            pending_update: None,
            allow_permanent_delete: false,
            file_kind: FileKind::All,
            type_min_mb: 10,
            dup_groups: Vec::new(),
            dup_scanning: false,
            confirm_dup_delete: false,
            leftovers: Vec::new(),
            leftovers_scanning: false,
            confirm_leftovers: false,
            pending_followup: None,
            leftovers_is_followup: false,
            recycle_bin_label: "点击刷新查看回收站占用".into(),
            confirm_empty_recycle: false,
            diff_items: Vec::new(),
            is_admin: is_elevated(),
            whitelist: LeftoverWhitelist::load(),
            confirm_safe_clean: false,
            show_recycle_hint: false,
            awaiting_uninstall_done: None,
            scan_paused_root: None,
            about_panel: AboutPanel::About,
            about_assets: None,
            about_tip: String::new(),
            expand_scanning: None,
            quiet_clean_started: false,
            selected_drives: HashSet::new(),
            drive_scan_queue: VecDeque::new(),
            scan_use_turbo: false,
            exclude_edit: String::new(),
            shred_delete: false,
            locking_procs: Vec::new(),
            related_services: Vec::new(),
            related_tasks: Vec::new(),
            appx_packages: Vec::new(),
            deep_loading: false,
            appx_loading: false,
            appx_filter: String::new(),
            schedule_status: String::new(),
        }
    }
}

fn dirs_fallback() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\"))
}

impl JanitorApp {
    pub fn new(cc: &eframe::CreationContext<'_>, open_path: Option<PathBuf>) -> Self {
        theme::apply_theme(&cc.egui_ctx);
        install_cjk_fonts(&cc.egui_ctx);
        let mut app = Self::default();
        app.exclude_edit = app.config.exclude_paths.join("\n");
        app.scan_use_turbo = app.config.scan_mode.eq_ignore_ascii_case("turbo");
        app.shred_delete = app.config.shred_default;
        if schedule::task_installed() {
            app.schedule_status = "计划任务已安装（DiskJanitorQuietClean）".into();
            app.config.schedule_quiet_clean = true;
        }
        cc.egui_ctx
            .set_pixels_per_point(app.config.ui_scale.clamp(0.85, 2.0));
        let default_root = dirs_fallback().display().to_string();
        if let Some(p) = open_path {
            app.root_input = p.display().to_string();
            app.current_dir = p;
            app.status = format!("已从命令行打开路径：{}", app.root_input);
        } else if !app.config.last_scan_root.trim().is_empty() && app.root_input == default_root {
            app.root_input = app.config.last_scan_root.clone();
            app.scan_paused_root = Some(app.config.last_scan_root.clone());
        }
        if app.config.check_on_start && !app.config.update_api_base.trim().is_empty() {
            app.start_update_check();
        }
        app
    }

    /// 扫描/清理等互斥；更新检查独立，不计入 busy
    fn busy(&self) -> bool {
        self.scanning
            || self.expand_scanning.is_some()
            || self.junk_scanning
            || self.junk_cleaning
            || self.deleting
            || self.apps_loading
            || self.startup_loading
            || self.shortcuts_scanning
            || self.orphans_scanning
            || self.dup_scanning
            || self.leftovers_scanning
            || self.deep_loading
            || self.appx_loading
    }

    fn scan_excludes(&self) -> Vec<PathBuf> {
        self.config
            .exclude_paths
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .collect()
    }

    fn start_scan(&mut self) {
        self.start_scan_with_opts(self.config.scan_mode == "turbo");
    }

    fn start_scan_with_opts(&mut self, turbo: bool) {
        if self.busy() {
            return;
        }
        let root = PathBuf::from(self.root_input.trim());
        if root.as_os_str().is_empty() {
            self.status = "请输入有效路径".into();
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        self.cancel = Some(cancel.clone());
        self.rx = Some(rx);
        self.scanning = true;
        self.scan_use_turbo = turbo;
        self.selected.clear();
        self.last_error.clear();
        self.tree_expanded.insert(ScanIndex::key(&root));
        let mode = if turbo { "极速" } else { "普通" };
        self.status = format!("正在{mode}扫描 {} …（可边扫边浏览）", root.display());
        self.current_dir = root.clone();
        self.tab = Tab::Browse;
        let excludes = self.scan_excludes();
        let opts = ScanOptions {
            excludes: excludes.clone(),
            turbo,
        };

        let handle = std::thread::spawn(move || {
            let send = |ev: ScanEvent| match ev {
                ScanEvent::Progress(p) => {
                    let _ = tx.send(WorkerMsg::Progress(p));
                }
                ScanEvent::Partial(i) => {
                    let _ = tx.send(WorkerMsg::Partial(i));
                }
                ScanEvent::Done(i) => {
                    let _ = tx.send(WorkerMsg::Done(i));
                }
            };
            if turbo {
                let _ = fast_scan::try_fast_scan(root, cancel, &excludes, send);
            } else {
                let _ = scan_path_ex(root, cancel, send, opts);
            }
        });
        self._worker = Some(handle);
    }

    fn start_queued_drive_scans(&mut self, roots: Vec<PathBuf>, turbo: bool) {
        if roots.is_empty() || self.busy() {
            return;
        }
        let mut iter = roots.into_iter();
        let Some(first) = iter.next() else {
            return;
        };
        self.drive_scan_queue = iter.collect();
        self.scan_use_turbo = turbo;
        self.root_input = first.display().to_string();
        let remaining = self.drive_scan_queue.len();
        self.status = if remaining > 0 {
            format!(
                "多盘排队：先扫 {}，其后还有 {} 个",
                first.display(),
                remaining
            )
        } else {
            format!("开始扫描 {}", first.display())
        };
        self.start_scan_with_opts(turbo);
    }

    fn maybe_continue_drive_queue(&mut self) {
        if self.busy() {
            return;
        }
        if let Some(next) = self.drive_scan_queue.pop_front() {
            self.root_input = next.display().to_string();
            let left = self.drive_scan_queue.len();
            self.status = format!(
                "多盘排队继续：{}（剩余 {}）",
                next.display(),
                left
            );
            let turbo = self.scan_use_turbo;
            self.start_scan_with_opts(turbo);
        }
    }

    fn start_junk_scan(&mut self) {
        if self.busy() {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        self.cancel = Some(cancel.clone());
        self.rx = Some(rx);
        self.junk_scanning = true;
        self.status = "正在扫描常见垃圾位置…".into();
        self.tab = Tab::Junk;
        let handle = std::thread::spawn(move || {
            let hits = scan_junk(&cancel);
            let _ = tx.send(WorkerMsg::JunkDone(hits));
        });
        self._worker = Some(handle);
    }

    fn start_software_scan(&mut self) {
        if self.busy() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.apps_loading = true;
        self.status = "正在读取已安装软件列表…".into();
        self.tab = Tab::Software;
        let handle = std::thread::spawn(move || {
            let apps = list_installed_apps();
            let _ = tx.send(WorkerMsg::SoftwareDone(apps));
        });
        self._worker = Some(handle);
    }

    fn start_appx_list(&mut self) {
        if self.busy() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.appx_loading = true;
        self.status = "正在读取 Store/AppX…".into();
        let handle = std::thread::spawn(move || {
            let pkgs = list_appx_packages();
            let _ = tx.send(WorkerMsg::AppxDone(pkgs));
        });
        self._worker = Some(handle);
    }

    fn start_deep_lock_check(&mut self, app: &InstalledApp) {
        if self.busy() {
            return;
        }
        let paths = install_paths_of(&app.install_location);
        if paths.is_empty() {
            self.status = "该软件无安装路径，无法查占用进程".into();
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.deep_loading = true;
        self.status = format!("正在查占用：{} …", app.display_name);
        let handle = std::thread::spawn(move || {
            let procs = list_locking_processes(&paths);
            let _ = tx.send(WorkerMsg::DeepLockDone(procs));
        });
        self._worker = Some(handle);
    }

    fn start_deep_svc_check(&mut self, app: &InstalledApp) {
        if self.busy() {
            return;
        }
        let tokens = tokens_from_name(&app.display_name);
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.deep_loading = true;
        self.status = format!("正在查服务/任务：{} …", app.display_name);
        let handle = std::thread::spawn(move || {
            let svcs = list_related_services(&tokens);
            let tasks = list_related_tasks(&tokens);
            let _ = tx.send(WorkerMsg::DeepSvcDone(svcs, tasks));
        });
        self._worker = Some(handle);
    }

    fn start_startup_scan(&mut self) {
        if self.busy() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.startup_loading = true;
        self.status = "正在读取开机启动项…".into();
        self.tab = Tab::Startup;
        let handle = std::thread::spawn(move || {
            let items = list_startup_items();
            let _ = tx.send(WorkerMsg::StartupDone(items));
        });
        self._worker = Some(handle);
    }

    fn start_shortcuts_scan(&mut self) {
        if self.busy() {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        self.cancel = Some(cancel.clone());
        self.rx = Some(rx);
        self.shortcuts_scanning = true;
        self.shortcut_scan_started = Some(Instant::now());
        self.status = "正在扫描失效快捷方式（桌面 + 开始菜单，最多约 6 秒）…".into();
        self.tab = Tab::Shortcuts;
        let handle = std::thread::spawn(move || {
            let hits = std::panic::catch_unwind(|| scan_broken_shortcuts(&cancel)).unwrap_or_default();
            let _ = tx.send(WorkerMsg::ShortcutsDone(hits));
        });
        self._worker = Some(handle);
    }

    fn start_orphans_scan(&mut self) {
        if self.busy() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.orphans_scanning = true;
        self.status = "正在检查无效卸载注册表项…".into();
        self.tab = Tab::Registry;
        let handle = std::thread::spawn(move || {
            let hits = scan_orphan_uninstall_keys();
            let _ = tx.send(WorkerMsg::OrphansDone(hits));
        });
        self._worker = Some(handle);
    }

    fn start_duplicates_scan(&mut self) {
        if self.busy() {
            return;
        }
        let Some(idx) = self.index.clone() else {
            self.status = "请先完成一次扫描，再查重复文件".into();
            return;
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        self.cancel = Some(cancel.clone());
        self.rx = Some(rx);
        self.dup_scanning = true;
        let min_mb = self.config.dup_min_mb.max(1);
        let max_groups = self.config.dup_max_groups.max(1);
        self.status = format!("正在查找重复文件（≥{min_mb}MB，最多 {max_groups} 组）…");
        self.tab = Tab::Duplicates;
        let strategy = self.config.dup_keep_strategy;
        let min_bytes = min_mb.saturating_mul(1024 * 1024);
        let handle = std::thread::spawn(move || {
            let groups = find_duplicates(&idx.entries, min_bytes, &cancel, max_groups, strategy);
            let _ = tx.send(WorkerMsg::DuplicatesDone(groups));
        });
        self._worker = Some(handle);
    }

    fn start_leftovers_scan(&mut self) {
        if self.busy() {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        self.cancel = Some(cancel.clone());
        self.rx = Some(rx);
        self.leftovers_scanning = true;
        self.leftovers_is_followup = false;
        self.status = "正在粗扫残留（仅 Program Files，易误报，请谨慎）…".into();
        self.tab = Tab::Tools;
        let handle = std::thread::spawn(move || {
            let hits = scan_leftovers(&cancel);
            let _ = tx.send(WorkerMsg::LeftoversDone(hits));
        });
        self._worker = Some(handle);
    }

    fn start_leftovers_followup(&mut self, hint: AppLeftoverHint) {
        if self.busy() {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        self.cancel = Some(cancel.clone());
        self.rx = Some(rx);
        self.leftovers_scanning = true;
        self.leftovers_is_followup = true;
        self.pending_followup = Some(hint.clone());
        self.status = format!("正在跟扫「{}」可能残留…", hint.display_name);
        self.tab = Tab::Software;
        let handle = std::thread::spawn(move || {
            let hits = scan_leftovers_for_app(&hint, &cancel);
            let _ = tx.send(WorkerMsg::LeftoversDone(hits));
        });
        self._worker = Some(handle);
    }

    fn start_update_check(&mut self) {
        let base = self.config.update_api_base.clone();
        if base.trim().is_empty() {
            self.update_status = "请先在「设置」填写更新服务器地址".into();
            self.tab = Tab::Settings;
            return;
        }
        if self.update_checking {
            self.update_status = "正在检查更新，请稍候…".into();
            return;
        }
        self.update_checking = true;
        self.update_status = "正在检查更新（jiaoben 三源）…".into();
        let (tx, rx) = mpsc::channel();
        self.update_rx = Some(rx);
        let handle = std::thread::spawn(move || {
            let r = check_update(&base);
            let _ = tx.send(WorkerMsg::UpdateDone(r));
        });
        self.update_worker = Some(handle);
    }

    fn start_update_apply(&mut self) {
        let Some(m) = self.pending_update.clone() else {
            return;
        };
        if self.update_checking {
            return;
        }
        self.update_checking = true;
        self.update_status = "正在下载更新…".into();
        let (tx, rx) = mpsc::channel();
        self.update_rx = Some(rx);
        let handle = std::thread::spawn(move || {
            let r = download_and_apply(&m);
            let _ = tx.send(WorkerMsg::UpdateApplied(r));
        });
        self.update_worker = Some(handle);
    }

    fn cancel_scan(&mut self) {
        if let Some(c) = &self.cancel {
            c.store(true, Ordering::Relaxed);
        }
        let was_scanning = self.scanning;
        if self.shortcuts_scanning {
            self.shortcuts_scanning = false;
            self.shortcut_scan_started = None;
            self.status = "已取消快捷方式扫描".into();
        }
        let root = self.root_input.trim();
        if was_scanning && !root.is_empty() {
            self.config.last_scan_root = root.to_string();
            self.scan_paused_root = Some(root.to_string());
            let _ = self.config.save();
            self.status = format!("已取消扫描并记住路径：{root}（可续扫）");
        }
    }

    fn resume_last_scan(&mut self) {
        let root = self
            .scan_paused_root
            .clone()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                let s = self.config.last_scan_root.trim();
                if s.is_empty() {
                    None
                } else {
                    Some(s.to_string())
                }
            });
        let Some(root) = root else {
            self.status = "没有可续扫的路径".into();
            return;
        };
        self.root_input = root.clone();

        if let Some(cp) = ScanCheckpoint::load() {
            let same_root = cp.root.eq_ignore_ascii_case(root.trim());
            if same_root && !cp.remaining.is_empty() {
                let base = self
                    .index
                    .clone()
                    .or_else(ScanCheckpoint::load_index);
                if let Some(base) = base {
                    let remaining: Vec<PathBuf> =
                        cp.remaining.iter().map(PathBuf::from).collect();
                    self.index = Some(base.clone());
                    self.start_resume_scan(PathBuf::from(root), base, remaining);
                    return;
                }
            }
        }
        self.start_scan();
    }

    fn start_resume_scan(&mut self, root: PathBuf, base: ScanIndex, remaining: Vec<PathBuf>) {
        if self.busy() {
            return;
        }
        if remaining.is_empty() {
            self.start_scan();
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        self.cancel = Some(cancel.clone());
        self.rx = Some(rx);
        self.scanning = true;
        self.selected.clear();
        self.last_error.clear();
        self.tree_expanded.insert(ScanIndex::key(&root));
        self.status = format!(
            "正在续扫 {}（剩余 {} 个目录）…",
            root.display(),
            remaining.len()
        );
        self.current_dir = root.clone();
        self.tab = Tab::Browse;

        let handle = std::thread::spawn(move || {
            let _idx = resume_scan(root, base, remaining, cancel, |ev| match ev {
                ScanEvent::Progress(p) => {
                    let _ = tx.send(WorkerMsg::Progress(p));
                }
                ScanEvent::Partial(i) => {
                    let _ = tx.send(WorkerMsg::Partial(i));
                }
                ScanEvent::Done(i) => {
                    let _ = tx.send(WorkerMsg::Done(i));
                }
            });
        });
        self._worker = Some(handle);
    }

    fn start_expand_count_only(&mut self, dir: PathBuf) {
        if self.busy() {
            return;
        }
        let Some(base) = self.index.clone() else {
            self.status = "请先扫描".into();
            return;
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        self.cancel = Some(cancel.clone());
        self.rx = Some(rx);
        self.scanning = true;
        self.expand_scanning = Some(dir.clone());
        self.status = format!("正在深入展开 {} …", dir.display());
        self.tab = Tab::Browse;
        let handle = std::thread::spawn(move || {
            let _idx = expand_count_only_dir(base, dir, cancel, |ev| match ev {
                ScanEvent::Progress(p) => {
                    let _ = tx.send(WorkerMsg::Progress(p));
                }
                ScanEvent::Partial(i) => {
                    let _ = tx.send(WorkerMsg::Partial(i));
                }
                ScanEvent::Done(i) => {
                    let _ = tx.send(WorkerMsg::Done(i));
                }
            });
        });
        self._worker = Some(handle);
    }

    fn maybe_quiet_clean_on_start(&mut self) {
        if self.quiet_clean_started || !self.config.quiet_clean_on_start {
            return;
        }
        if self.busy() {
            // 等空闲后再清，不要把「已尝试」锁死
            return;
        }
        self.quiet_clean_started = true;
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.junk_cleaning = true;
        self.status = "启动时安静清理安全垃圾…".into();
        let handle = std::thread::spawn(move || {
            let cancel = AtomicBool::new(false);
            let hits = scan_junk(&cancel);
            let safe = safe_junk_hits(hits);
            let paths = junk_selected_paths(&safe);
            let res = if paths.is_empty() {
                TrashResult::default()
            } else {
                clean_junk_paths(&paths)
            };
            let _ = tx.send(WorkerMsg::JunkCleanDone(res));
        });
        self._worker = Some(handle);
    }

    fn refresh_drives(&mut self) {
        self.drives = list_drives();
        self.drive_infos = list_drive_infos();
        self.status = format!("已刷新盘符：{} 个", self.drive_infos.len());
    }

    fn refresh_recycle_bin(&mut self) {
        match recycle_bin_size() {
            Ok((sz, n)) => {
                self.recycle_bin_label = format!("回收站约 {} · {} 项", format_bytes(sz), n);
                self.status = self.recycle_bin_label.clone();
            }
            Err(e) => {
                self.recycle_bin_label = format!("无法估算回收站：{e}");
                self.last_error = e;
            }
        }
    }

    fn poll_one_rx(rx: &Receiver<WorkerMsg>) -> (Option<ScanProgress>, Option<ScanIndex>, Vec<WorkerMsg>) {
        let mut latest_progress = None;
        let mut latest_partial = None;
        let mut other = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            match msg {
                WorkerMsg::Progress(p) => latest_progress = Some(p),
                WorkerMsg::Partial(idx) => latest_partial = Some(idx),
                other_msg => other.push(other_msg),
            }
        }
        (latest_progress, latest_partial, other)
    }

    fn poll_worker(&mut self) {
        let mut msgs = Vec::new();
        if let Some(rx) = &self.rx {
            let (p, partial, other) = Self::poll_one_rx(rx);
            if let Some(p) = p {
                self.progress = Some(p);
            }
            if let Some(idx) = partial {
                let keep = self.current_dir.clone();
                self.index = Some(idx);
                if let Some(i) = &self.index {
                    if i.get(&keep).is_some() {
                        self.current_dir = keep;
                    }
                }
            }
            msgs.extend(other);
        }
        if let Some(rx) = &self.update_rx {
            let (_p, _partial, other) = Self::poll_one_rx(rx);
            msgs.extend(other);
        }

        for msg in msgs {
            match msg {
                WorkerMsg::Progress(_) | WorkerMsg::Partial(_) => {}
                WorkerMsg::Done(idx) => {
                    self.scanning = false;
                    let was_expand = self.expand_scanning.take();
                    let cancelled = idx.partial
                        || self
                            .progress
                            .as_ref()
                            .map(|p| p.cancelled)
                            .unwrap_or(false);
                    let root = idx.root.clone();
                    let n = idx.entries.len();
                    let skipped = idx.skipped;
                    let skipped_bytes = idx.skipped_bytes;
                    let total = idx.get(&root).map(|e| e.size).unwrap_or(0);
                    let secs = self
                        .progress
                        .as_ref()
                        .map(|p| p.elapsed.as_secs_f32())
                        .unwrap_or(0.0);
                    let visited = self
                        .progress
                        .as_ref()
                        .map(|p| p.visited)
                        .unwrap_or(n as u64);
                    let bytes_seen = self
                        .progress
                        .as_ref()
                        .map(|p| p.bytes_seen)
                        .unwrap_or(total);
                    let mut status = if was_expand.is_some() {
                        if cancelled {
                            format!(
                                "展开已取消（部分结果）：{} 项 · {}",
                                n,
                                format_bytes(total)
                            )
                        } else {
                            format!(
                                "已展开目录：{} 项，合计 {}（{:.1}s）",
                                n,
                                format_bytes(total),
                                secs
                            )
                        }
                    } else if cancelled {
                        format!(
                            "已取消扫描（部分结果）：{} 项，合计 {}（{:.1}s）· 可续扫",
                            n,
                            format_bytes(total),
                            secs
                        )
                    } else {
                        format!(
                            "扫描完成：{} 项，跳过 {}，合计 {}（{:.1}s）",
                            n,
                            skipped,
                            format_bytes(total),
                            secs
                        )
                    };
                    if skipped_bytes > 0 {
                        status.push_str(&format!(
                            " · 跳过目录估算计入 {}",
                            format_bytes(skipped_bytes)
                        ));
                    }
                    self.status = status;
                    if !idx.errors.is_empty() {
                        self.last_error = idx.errors.join("\n");
                    }
                    if !idx.skipped_notes.is_empty() {
                        self.push_log(format!(
                            "跳过提示：{}",
                            idx.skipped_notes.iter().take(5).cloned().collect::<Vec<_>>().join("；")
                        ));
                    }
                    if was_expand.is_none() {
                        if cancelled && !idx.resume_stack.is_empty() {
                            let cp = ScanCheckpoint::from_cancel(
                                &root,
                                &idx.resume_stack,
                                &idx,
                                visited,
                                skipped,
                                bytes_seen,
                            );
                            if let Err(e) = cp.save_with_index(&idx) {
                                self.push_log(format!("断点保存失败：{e}"));
                            } else {
                                self.push_log(format!(
                                    "已保存断点：剩余 {} 个目录（重启后仍可续扫）",
                                    cp.remaining.len()
                                ));
                            }
                        } else if !cancelled {
                            ScanCheckpoint::clear();
                        }
                    }
                    self.config.last_scan_root = idx.root.display().to_string();
                    self.scan_paused_root = Some(self.config.last_scan_root.clone());
                    self.config.last_scan_summary = if cancelled {
                        format!(
                            "未完成 · {} 文件 · {} · {}",
                            idx.file_count,
                            format_bytes(total),
                            idx.root.display()
                        )
                    } else {
                        format!(
                            "{} 文件 · {} · {}",
                            idx.file_count,
                            format_bytes(total),
                            idx.root.display()
                        )
                    };
                    let _ = self.config.save();
                    if let Some(ref exp) = was_expand {
                        if idx.get(exp).is_some() {
                            self.current_dir = exp.clone();
                        } else {
                            self.current_dir = root;
                        }
                    } else {
                        self.current_dir = root;
                    }
                    self.index = Some(idx);
                    self.progress = None;
                    self.rx = None;
                    if was_expand.is_none() && !cancelled {
                        self.maybe_continue_drive_queue();
                    }
                }
                WorkerMsg::JunkDone(hits) => {
                    self.junk_scanning = false;
                    let total: u64 = hits.iter().map(|h| h.size).sum();
                    self.status = format!(
                        "垃圾建议：{} 类，合计约 {}",
                        hits.len(),
                        format_bytes(total)
                    );
                    self.junk_hits = hits;
                    self.rx = None;
                }
                WorkerMsg::JunkCleanDone(res) => {
                    self.apply_junk_clean_result(res);
                    self.rx = None;
                }
                WorkerMsg::DeleteDone(res) => {
                    self.apply_delete_result(res);
                    self.rx = None;
                }
                WorkerMsg::SoftwareDone(apps) => {
                    self.apps_loading = false;
                    self.status = format!("已安装软件：{} 项（可搜索后点卸载）", apps.len());
                    self.apps = apps;
                    self.rx = None;
                }
                WorkerMsg::StartupDone(items) => {
                    self.startup_loading = false;
                    let on = items.iter().filter(|i| i.enabled).count();
                    self.status = format!(
                        "开机启动项：{} 个（启用 {} / 已禁用 {}）",
                        items.len(),
                        on,
                        items.len() - on
                    );
                    self.startup_items = items;
                    self.rx = None;
                }
                WorkerMsg::ShortcutsDone(hits) => {
                    self.shortcuts_scanning = false;
                    self.shortcut_scan_started = None;
                    self.status = format!("失效快捷方式：{} 个（桌面 + 开始菜单）", hits.len());
                    self.broken_shortcuts = hits;
                    self.rx = None;
                }
                WorkerMsg::OrphansDone(hits) => {
                    self.orphans_scanning = false;
                    self.status = format!(
                        "无效卸载注册表项：{} 个（默认只勾选卸载程序也缺失的）",
                        hits.len()
                    );
                    self.orphans = hits;
                    self.rx = None;
                }
                WorkerMsg::DuplicatesDone(groups) => {
                    self.dup_scanning = false;
                    let waste: u64 = groups.iter().map(|g| g.waste()).sum();
                    self.status = format!(
                        "重复文件：{} 组，可节省约 {}",
                        groups.len(),
                        format_bytes(waste)
                    );
                    self.dup_groups = groups;
                    self.rx = None;
                }
                WorkerMsg::LeftoversDone(hits) => {
                    self.leftovers_scanning = false;
                    let hits = filter_whitelisted(hits, &self.whitelist);
                    let high = hits
                        .iter()
                        .filter(|h| h.confidence == Confidence::High)
                        .count();
                    let total: u64 = hits.iter().map(|h| h.size_hint).sum();
                    self.status = if self.leftovers_is_followup {
                        format!(
                            "跟扫完成：{} 项（高置信 {}，约 {}）。默认全不勾选，请确认后再删。",
                            hits.len(),
                            high,
                            format_bytes(total)
                        )
                    } else {
                        format!(
                            "粗扫完成：{} 项（约 {}，置信偏低）。更推荐「软件卸载 → 跟扫残留」。",
                            hits.len(),
                            format_bytes(total)
                        )
                    };
                    self.leftovers = hits;
                    if self.leftovers_is_followup {
                        // 跟扫已完成，清掉「待跟扫」以免重复提示
                        self.pending_followup = None;
                        self.awaiting_uninstall_done = None;
                        self.tab = Tab::Software;
                    } else {
                        self.tab = Tab::Tools;
                    }
                    self.rx = None;
                }
                WorkerMsg::DeepLockDone(procs) => {
                    self.deep_loading = false;
                    self.locking_procs = procs;
                    self.status = format!("占用进程：{} 个", self.locking_procs.len());
                    self.rx = None;
                }
                WorkerMsg::DeepSvcDone(svcs, tasks) => {
                    self.deep_loading = false;
                    self.related_services = svcs;
                    self.related_tasks = tasks;
                    self.status = format!(
                        "相关服务 {} · 计划任务 {}",
                        self.related_services.len(),
                        self.related_tasks.len()
                    );
                    self.rx = None;
                }
                WorkerMsg::AppxDone(pkgs) => {
                    self.appx_loading = false;
                    self.appx_packages = pkgs;
                    self.status = format!("Store/AppX 应用：{} 个", self.appx_packages.len());
                    self.rx = None;
                }
                WorkerMsg::UpdateDone(r) => {
                    self.update_checking = false;
                    self.update_rx = None;
                    match r {
                        UpdateCheck::UpToDate => {
                            self.pending_update = None;
                            self.update_status = format!(
                                "已是最新版 v{APP_VERSION_NAME} (#{APP_VERSION_CODE})"
                            );
                            self.push_log(self.update_status.clone());
                        }
                        UpdateCheck::Available(m) => {
                            self.update_status = format!(
                                "发现新版本 {}（当前 v{APP_VERSION_NAME} #{APP_VERSION_CODE}）",
                                m.label()
                            );
                            self.push_log(format!(
                                "可更新 → {}：{}",
                                m.label(),
                                if m.changelog.is_empty() {
                                    "无说明"
                                } else {
                                    &m.changelog
                                }
                            ));
                            self.pending_update = Some(m);
                            self.tab = Tab::Settings;
                        }
                        UpdateCheck::Disabled => {
                            self.update_status = "未配置更新服务器".into();
                        }
                        UpdateCheck::Failed(e) => {
                            self.update_status = format!("检查失败：{e}");
                            self.push_log(self.update_status.clone());
                        }
                    }
                }
                WorkerMsg::UpdateApplied(r) => {
                    self.update_checking = false;
                    self.update_rx = None;
                    match r {
                        Ok(msg) => {
                            self.update_status = msg.clone();
                            self.push_log(msg);
                            self.status = "更新已下载，请关闭窗口以完成替换".into();
                        }
                        Err(e) => {
                            self.update_status = format!("更新失败：{e}");
                            self.push_log(self.update_status.clone());
                            self.last_error = e;
                        }
                    }
                }
            }
        }
    }

    fn visible_entries(&self) -> Vec<&FsEntry> {
        let Some(idx) = &self.index else {
            return Vec::new();
        };
        let mut list = idx.children_of(&self.current_dir);
        if !self.filter.is_empty() {
            let f = self.filter.to_lowercase();
            list.retain(|e| e.name.to_lowercase().contains(&f));
        }
        sort_entries(&mut list, self.sort_key, self.sort_dir);
        list
    }

    fn take_visible_entries(&mut self) -> Vec<FsEntry> {
        let owned: Vec<FsEntry> = self.visible_entries().into_iter().cloned().collect();
        self.list_total = owned.len();
        let limit = self.list_limit.max(20);
        owned.into_iter().take(limit).collect()
    }

    fn selected_paths(&self) -> Vec<PathBuf> {
        let Some(idx) = &self.index else {
            return Vec::new();
        };
        self.selected
            .iter()
            .filter_map(|k| {
                idx.entries
                    .get(k)
                    .or_else(|| {
                        idx.entries
                            .iter()
                            .find(|(ek, _)| ek.eq_ignore_ascii_case(k))
                            .map(|(_, e)| e)
                    })
                    .map(|e| e.path.clone())
            })
            .collect()
    }

    fn selected_total_size(&self) -> u64 {
        let Some(idx) = &self.index else {
            return 0;
        };
        self.selected
            .iter()
            .filter_map(|k| {
                idx.entries
                    .get(k)
                    .or_else(|| {
                        idx.entries
                            .iter()
                            .find(|(ek, _)| ek.eq_ignore_ascii_case(k))
                            .map(|(_, e)| e)
                    })
                    .map(|e| e.size)
            })
            .sum()
    }

    fn push_log(&mut self, line: impl Into<String>) {
        let ts = chrono::Local::now().format("%H:%M:%S");
        self.op_log.push_front(format!("[{ts}] {}", line.into()));
        while self.op_log.len() > 80 {
            self.op_log.pop_back();
        }
        self.show_log = true;
    }

    fn do_delete(&mut self) {
        let paths = self.selected_paths();
        if paths.is_empty() || self.busy() {
            return;
        }
        self.confirm_delete = false;
        self.confirm_sensitive = false;
        let allow = self.allow_permanent_delete;
        let shred = self.shred_delete;
        self.deleting = true;
        self.last_error.clear();
        self.status = format!("正在删除 {} 项…（大文件可能需一点时间）", paths.len());
        self.push_log(format!(
            "开始删除 {} 项{}",
            paths.len(),
            if shred {
                "（安全粉碎）"
            } else if allow {
                "（允许回收站失败时直接删除）"
            } else {
                ""
            }
        ));
        for p in paths.iter().take(5) {
            self.push_log(format!("  · {}", p.display()));
        }
        if paths.len() > 5 {
            self.push_log(format!("  · …另有 {} 项", paths.len() - 5));
        }
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let handle = std::thread::spawn(move || {
            let res = move_to_trash_with(
                &paths,
                DeleteOptions {
                    allow_permanent: allow || shred,
                    shred,
                },
            );
            let _ = tx.send(WorkerMsg::DeleteDone(res));
        });
        self._worker = Some(handle);
    }

    fn apply_delete_result(&mut self, res: TrashResult) {
        self.deleting = false;
        for p in &res.ok {
            let key = ScanIndex::key(p);
            self.selected.remove(&key);
            if let Some(idx) = &mut self.index {
                idx.remove_cascade(p);
            }
            // 同步从重复组 / 残留列表移除（保持 selected 与 paths 对齐；忽略大小写）
            let del_key = ScanIndex::key_norm(p);
            for g in &mut self.dup_groups {
                let mut new_paths = Vec::new();
                let mut new_sel = Vec::new();
                for (i, x) in g.paths.iter().enumerate() {
                    if ScanIndex::key_norm(x) != del_key {
                        new_paths.push(x.clone());
                        new_sel.push(g.selected.get(i).copied().unwrap_or(false));
                    }
                }
                // 若只剩一项，取消勾选（避免误删唯一保留项）
                if new_paths.len() == 1 {
                    new_sel = vec![false];
                }
                g.paths = new_paths;
                g.selected = new_sel;
            }
            self.dup_groups.retain(|g| g.paths.len() >= 2);
            self.leftovers
                .retain(|h| ScanIndex::key_norm(&h.path) != del_key);
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| p.display().to_string());
            self.push_log(format!("✓ 已删除 {name}"));
        }
        for (p, e) in &res.failed {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| p.display().to_string());
            self.push_log(format!("✗ 失败 {name}: {e}"));
        }
        if res.permanent > 0 {
            self.push_log(format!("ℹ {} 项无法进回收站，已直接删除", res.permanent));
        }
        if res.skipped_locked > 0 {
            self.push_log(format!(
                "ℹ {} 项占用中已跳过（如 WSL 的 vhdx，请先 wsl --shutdown）",
                res.skipped_locked
            ));
        }

        let err = format_trash_errors(&res, 40);
        if !err.is_empty() {
            self.last_error = err;
        }

        self.status = if res.failed.is_empty() && !res.ok.is_empty() {
            format!(
                "删除完成：成功 {} 项{}（列表已刷新）",
                res.ok.len(),
                if res.permanent > 0 {
                    format!("，其中直接删除 {}", res.permanent)
                } else {
                    String::new()
                }
            )
        } else if res.ok.is_empty() {
            format!(
                "删除失败：0 成功 / {} 失败（见操作日志）",
                res.failed.len()
            )
        } else {
            format!(
                "部分完成：成功 {}，失败 {}（列表已刷新成功项，见操作日志）",
                res.ok.len(),
                res.failed.len()
            )
        };
        // permanent 项也计入 ok；仅当至少有一项进了回收站时提示
        if !res.ok.is_empty() && (res.ok.len() as u64) > res.permanent {
            self.show_recycle_hint = true;
        }
    }

    fn do_junk_clean(&mut self) {
        let paths = junk_selected_paths(&self.junk_hits);
        if paths.is_empty() || self.busy() {
            return;
        }
        self.confirm_junk = false;
        self.confirm_safe_clean = false;
        self.junk_cleaning = true;
        self.last_error.clear();
        self.status = format!(
            "正在清理 {} 个位置（后台进行，占用文件会跳过）…",
            paths.len()
        );
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let handle = std::thread::spawn(move || {
            let res = clean_junk_paths(&paths);
            let _ = tx.send(WorkerMsg::JunkCleanDone(res));
        });
        self._worker = Some(handle);
    }

    fn apply_junk_clean_result(&mut self, res: TrashResult) {
        self.junk_cleaning = false;
        let cleaned = res.ok.len() as u64;
        if cleaned > 0 || res.permanent > 0 {
            for h in &mut self.junk_hits {
                if h.selected {
                    h.selected = false;
                    h.size = 0;
                    h.note = "已清理（可再扫描确认）".into();
                    h.paths.clear();
                }
            }
            self.junk_hits
                .retain(|h| h.size > 0 || !h.note.contains("已清理"));
        }
        self.push_log(format!(
            "垃圾清理：成功 {}，直接删 {}，跳过占用 {}，失败 {}",
            cleaned,
            res.permanent,
            res.skipped_locked,
            res.failed.len()
        ));
        self.status = format!(
            "垃圾清理完成：成功 {} 项，直接删除 {}，跳过占用 {}，失败 {}（见操作日志）",
            cleaned,
            res.permanent,
            res.skipped_locked,
            res.failed.len()
        );
        let err = format_trash_errors(&res, 30);
        if !err.is_empty() {
            self.last_error = err;
        }
        if cleaned > 0 && cleaned > res.permanent {
            self.show_recycle_hint = true;
        }
    }

    fn do_shortcuts_clean(&mut self) {
        let paths: Vec<PathBuf> = self
            .broken_shortcuts
            .iter()
            .filter(|s| s.selected)
            .map(|s| s.path.clone())
            .collect();
        if paths.is_empty() {
            return;
        }
        let res = move_to_trash(&paths);
        self.broken_shortcuts
            .retain(|s| !res.ok.iter().any(|p| p == &s.path));
        self.status = if res.failed.is_empty() {
            format!("已删除失效快捷方式：{} 个", res.ok.len())
        } else {
            format!(
                "快捷方式：成功 {}，失败 {}",
                res.ok.len(),
                res.failed.len()
            )
        };
        if !res.ok.is_empty() && res.permanent == 0 {
            self.show_recycle_hint = true;
        }
        self.confirm_shortcuts = false;
    }

    fn do_orphans_clean(&mut self) {
        let items: Vec<OrphanReg> = self
            .orphans
            .iter()
            .filter(|o| o.selected)
            .cloned()
            .collect();
        if items.is_empty() {
            return;
        }
        let (ok, errs) = delete_orphan_keys(&items);
        let attempted: HashSet<String> = items.iter().map(|o| o.full_path.clone()).collect();
        let failed: HashSet<String> = errs
            .iter()
            .filter_map(|e| {
                attempted
                    .iter()
                    .find(|p| e.starts_with(p.as_str()))
                    .cloned()
            })
            .collect();
        self.orphans
            .retain(|o| !attempted.contains(&o.full_path) || failed.contains(&o.full_path));

        self.status = if errs.is_empty() {
            format!("已删除无效注册表项：{ok} 个")
        } else {
            format!("注册表：成功 {ok}，失败 {}（HKLM 常需管理员）", errs.len())
        };
        if !errs.is_empty() {
            self.last_error = errs.join("\n");
        }
        self.confirm_orphans = false;
    }

    fn do_dup_delete(&mut self) {
        let mut paths = Vec::new();
        for g in &self.dup_groups {
            for (i, p) in g.paths.iter().enumerate() {
                if g.selected.get(i).copied().unwrap_or(false) {
                    paths.push(p.clone());
                }
            }
        }
        if paths.is_empty() || self.busy() {
            return;
        }
        self.confirm_dup_delete = false;
        let allow = self.allow_permanent_delete;
        self.deleting = true;
        self.status = format!("正在删除 {} 个重复文件…", paths.len());
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let handle = std::thread::spawn(move || {
            let res = move_to_trash_with(&paths, DeleteOptions {
                    allow_permanent: allow,
                    shred: false,
                });
            let _ = tx.send(WorkerMsg::DeleteDone(res));
        });
        self._worker = Some(handle);
    }

    fn do_leftovers_delete(&mut self) {
        let paths: Vec<PathBuf> = self
            .leftovers
            .iter()
            .filter(|h| h.selected)
            .map(|h| h.path.clone())
            .collect();
        if paths.is_empty() || self.busy() {
            return;
        }
        self.confirm_leftovers = false;
        let allow = self.allow_permanent_delete;
        self.deleting = true;
        self.status = format!("正在删除 {} 个残留目录…", paths.len());
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let handle = std::thread::spawn(move || {
            let res = move_to_trash_with(&paths, DeleteOptions {
                    allow_permanent: allow,
                    shred: false,
                });
            let _ = tx.send(WorkerMsg::DeleteDone(res));
        });
        self._worker = Some(handle);
    }

    fn do_empty_recycle(&mut self) {
        self.confirm_empty_recycle = false;
        match empty_recycle_bin() {
            Ok(msg) => {
                self.status = msg.clone();
                self.push_log(msg);
                self.refresh_recycle_bin();
            }
            Err(e) => {
                self.status = format!("清空回收站失败：{e}");
                self.last_error = e;
            }
        }
    }

    fn export_scan(&mut self, as_json: bool) {
        let Some(idx) = &self.index else {
            self.status = "请先扫描再导出".into();
            return;
        };
        let dir = default_export_dir();
        let _ = std::fs::create_dir_all(&dir);
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let path = if as_json {
            dir.join(format!("disk-janitor-{stamp}.json"))
        } else {
            dir.join(format!("disk-janitor-{stamp}.csv"))
        };
        let r = if as_json {
            export_json(idx, &path)
        } else {
            export_csv(idx, &path)
        };
        match r {
            Ok(()) => {
                self.status = format!("已导出到 {}", path.display());
                self.push_log(self.status.clone());
            }
            Err(e) => {
                self.status = format!("导出失败：{e}");
                self.last_error = e;
            }
        }
    }

    fn save_baseline(&mut self) {
        let Some(idx) = &self.index else {
            self.status = "请先扫描再保存对比基线".into();
            return;
        };
        match save_snapshot(idx) {
            Ok(p) => {
                self.status = format!("已保存对比基线：{}", p.display());
                self.push_log(self.status.clone());
            }
            Err(e) => {
                self.status = format!("保存快照失败：{e}");
                self.last_error = e;
            }
        }
    }

    fn compare_baseline(&mut self) {
        let Some(idx) = &self.index else {
            self.status = "请先扫描再对比".into();
            return;
        };
        match load_snapshot() {
            Ok(old) => {
                self.diff_items = compare_with_snapshot(idx, &old);
                self.status = format!(
                    "与基线对比：{} 项显著变化（基线 {} · {}）",
                    self.diff_items.len(),
                    old.saved_at,
                    old.root
                );
                self.push_log(self.status.clone());
            }
            Err(e) => {
                self.status = format!("加载基线失败：{e}（请先「保存对比基线」）");
                self.last_error = e;
            }
        }
    }
}

fn install_cjk_fonts(ctx: &egui::Context) {
    let candidates = [
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\msyh.ttf",
        "C:\\Windows\\Fonts\\simhei.ttf",
        "C:\\Windows\\Fonts\\simsun.ttc",
    ];
    let mut data = None;
    for p in candidates {
        if let Ok(bytes) = std::fs::read(p) {
            data = Some(bytes);
            break;
        }
    }
    let Some(bytes) = data else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "chinese".to_owned(),
        std::sync::Arc::new(egui::FontData::from_owned(bytes)),
    );
    if let Some(fam) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        fam.insert(0, "chinese".to_owned());
    }
    if let Some(fam) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        fam.insert(0, "chinese".to_owned());
    }
    ctx.set_fonts(fonts);
}

fn draw_tree(ui: &mut egui::Ui, app: &mut JanitorApp, dir: PathBuf, depth: u32) {
    let Some(idx) = &app.index else {
        return;
    };
    let key = ScanIndex::key(&dir);
    let size = idx.get(&dir).map(|e| e.size).unwrap_or(0);
    let name = idx
        .get(&dir)
        .map(|e| e.name.clone())
        .unwrap_or_else(|| dir.display().to_string());
    let label = if depth == 0 {
        format!("{} ({})", name, format_bytes(size))
    } else {
        format!("{}  {}", name, format_bytes(size))
    };
    let children: Vec<PathBuf> = idx
        .child_dirs_top(&dir, app.list_limit.min(80).max(20))
        .into_iter()
        .map(|e| e.path.clone())
        .collect();

    if children.is_empty() {
        let selected = app.current_dir == dir;
        if ui
            .selectable_label(selected, format!("📁 {label}"))
            .clicked()
        {
            app.current_dir = dir;
            app.tab = Tab::Browse;
        }
        return;
    }

    let default_open = depth < 1 || app.tree_expanded.contains(&key);
    let id = ui.make_persistent_id(("dj_tree", &key));
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, default_open)
        .show_header(ui, |ui| {
            let selected = app.current_dir == dir;
            if ui
                .selectable_label(selected, format!("📁 {label}"))
                .clicked()
            {
                app.current_dir = dir.clone();
                app.tab = Tab::Browse;
                app.tree_expanded.insert(key.clone());
            }
        })
        .body(|ui| {
            app.tree_expanded.insert(key.clone());
            for c in children {
                draw_tree(ui, app, c, depth + 1);
            }
        });
}

impl eframe::App for JanitorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.about_assets.is_none() {
            self.about_assets = Some(AboutAssets::load(ctx));
        }
        self.maybe_quiet_clean_on_start();
        self.poll_worker();
        if self.shortcuts_scanning {
            let timed_out = self
                .shortcut_scan_started
                .map(|t| t.elapsed().as_secs() >= 4)
                .unwrap_or(false);
            if timed_out {
                self.shortcuts_scanning = false;
                self.shortcut_scan_started = None;
                self.status = "快捷方式扫描超时已结束（可再点一次）".into();
            }
        }
        if let Some(h) = &self._worker {
            if h.is_finished() {
                self.poll_worker();
                if self.shortcuts_scanning {
                    self.shortcuts_scanning = false;
                    self.shortcut_scan_started = None;
                    self.status = "快捷方式扫描结束".into();
                }
                if self.apps_loading {
                    self.apps_loading = false;
                }
                if self.startup_loading {
                    self.startup_loading = false;
                }
                if self.orphans_scanning {
                    self.orphans_scanning = false;
                }
                if self.junk_scanning {
                    self.junk_scanning = false;
                }
                if self.junk_cleaning {
                    self.junk_cleaning = false;
                    self.status = "垃圾清理已结束".into();
                }
                if self.deleting {
                    self.deleting = false;
                    self.status = "删除已结束".into();
                }
                if self.dup_scanning {
                    self.dup_scanning = false;
                }
                if self.leftovers_scanning {
                    self.leftovers_scanning = false;
                }
                if self.deep_loading {
                    self.deep_loading = false;
                }
                if self.appx_loading {
                    self.appx_loading = false;
                }
                if self.expand_scanning.is_some() {
                    self.expand_scanning = None;
                    self.scanning = false;
                }
            }
        }
        if let Some(h) = &self.update_worker {
            if h.is_finished() {
                self.poll_worker();
                if self.update_checking {
                    self.update_checking = false;
                }
            }
        }
        if self.busy() || self.update_checking {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }

        egui::TopBottomPanel::top("top")
            .frame(theme::top_bar_frame())
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new("DISK JANITOR")
                                .color(MUTED)
                                .size(10.0)
                                .strong(),
                        );
                        ui.label(
                            egui::RichText::new("大帅清理器")
                                .color(ACCENT)
                                .strong()
                                .size(22.0),
                        );
                    });
                    ui.add_space(6.0);
                    theme::version_pill(
                        ui,
                        &format!("v{APP_VERSION_NAME}  ·  #{APP_VERSION_CODE}"),
                    );
                    if self.pending_update.is_some() {
                        ui.colored_label(theme::DANGER, "● 有新版");
                    }
                    if self.is_admin {
                        ui.colored_label(OK, "管理员");
                    }
                    ui.add_space(8.0);
                    for (tab, label) in [
                        (Tab::Overview, "总览"),
                        (Tab::Browse, "浏览"),
                        (Tab::TopN, "最大占用"),
                        (Tab::Types, "按类型"),
                        (Tab::Junk, "垃圾建议"),
                        (Tab::Software, "软件卸载"),
                        (Tab::Startup, "开机自启"),
                        (Tab::Shortcuts, "快捷方式"),
                        (Tab::Registry, "注册表"),
                        (Tab::Duplicates, "重复文件"),
                        (Tab::Tools, "工具"),
                        (Tab::Settings, "设置"),
                        (Tab::About, "关于"),
                    ] {
                        let selected = self.tab == tab;
                        if theme::tab_label(ui, selected, label).clicked() {
                            self.tab = tab;
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !self.update_status.is_empty() {
                            ui.label(
                                egui::RichText::new(&self.update_status)
                                    .color(WARN)
                                    .size(12.0),
                            );
                        }
                    });
                });
                ui.add_space(8.0);
                theme::hairline(ui);
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("盘符").color(MUTED).size(12.0));
                    for d in self.drives.clone() {
                        if ui
                            .add(theme::ghost_button(&d.display().to_string()))
                            .clicked()
                        {
                            self.root_input = d.display().to_string();
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("路径").color(MUTED));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.root_input)
                            .desired_width(300.0)
                            .hint_text(r"例如 C:\ 或 D:\Downloads"),
                    );
                    let busy = self.busy();
                    if ui
                        .add_enabled(!busy, theme::accent_button("扫描"))
                        .clicked()
                    {
                        self.start_scan();
                    }
                    if busy && ui.add(theme::ghost_button("取消")).clicked() {
                        self.cancel_scan();
                    }
                    if !busy
                        && !self.config.last_scan_root.trim().is_empty()
                        && ui.add(theme::ghost_button("续扫")).clicked()
                    {
                        self.resume_last_scan();
                    }
                    if ui
                        .add_enabled(!busy, theme::ghost_button("垃圾建议"))
                        .clicked()
                    {
                        self.start_junk_scan();
                    }
                    if ui
                        .add_enabled(!busy, theme::ghost_button("刷新软件"))
                        .clicked()
                    {
                        self.start_software_scan();
                    }
                    if ui
                        .add_enabled(!busy, theme::ghost_button("开机自启"))
                        .clicked()
                    {
                        self.start_startup_scan();
                    }
                    if ui
                        .add_enabled(!busy, theme::ghost_button("扫快捷方式"))
                        .clicked()
                    {
                        self.start_shortcuts_scan();
                    }
                    if ui
                        .add_enabled(!busy, theme::ghost_button("扫注册表"))
                        .clicked()
                    {
                        self.start_orphans_scan();
                    }
                });
                if let Some(p) = &self.progress {
                    let partial = self.index.as_ref().map(|i| i.partial).unwrap_or(false);
                    ui.label(format!(
                        "{}已访问 {} · 跳过 {} · 约 {} · {}",
                        if partial { "（增量中）" } else { "" },
                        p.visited,
                        p.skipped,
                        format_bytes(p.bytes_seen),
                        p.current
                    ));
                    ui.add(egui::ProgressBar::new(0.4).animate(true));
                }
                if let Some(idx) = &self.index {
                    if idx.skipped_bytes > 0 && !idx.skipped_notes.is_empty() {
                        ui.colored_label(
                            WARN,
                            format!(
                                "提示：部分目录仅估算占用（{}），如 {}",
                                format_bytes(idx.skipped_bytes),
                                idx.skipped_notes.first().cloned().unwrap_or_default()
                            ),
                        );
                    }
                }
                if self.tab == Tab::Browse {
                    ui.horizontal(|ui| {
                        ui.label("排序");
                        for (label, key) in [
                            ("大小", SortKey::Size),
                            ("时间", SortKey::Mtime),
                            ("名称", SortKey::Name),
                        ] {
                            if ui.selectable_label(self.sort_key == key, label).clicked() {
                                if self.sort_key == key {
                                    self.sort_dir = match self.sort_dir {
                                        SortDir::Asc => SortDir::Desc,
                                        SortDir::Desc => SortDir::Asc,
                                    };
                                } else {
                                    self.sort_key = key;
                                    self.sort_dir = if key == SortKey::Name {
                                        SortDir::Asc
                                    } else {
                                        SortDir::Desc
                                    };
                                }
                            }
                        }
                        ui.separator();
                        ui.label("显示前");
                        for n in [50usize, 100, 200, 500] {
                            if ui
                                .selectable_label(self.list_limit == n, format!("{n}"))
                                .clicked()
                            {
                                self.list_limit = n;
                            }
                        }
                        ui.separator();
                        ui.label("过滤");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.filter)
                                .desired_width(120.0)
                                .hint_text("名称包含…"),
                        );
                    });
                }
            });

        egui::TopBottomPanel::bottom("bottom").show(ctx, |ui| {
            ui.label(&self.status);
            ui.horizontal(|ui| {
                if matches!(self.tab, Tab::Browse | Tab::TopN | Tab::Types) {
                    let n = self.selected.len();
                    let sz = self.selected_total_size();
                    ui.label(format!("已选 {} · {}", n, format_bytes(sz)));
                    if self.deleting {
                        ui.spinner();
                        ui.label("删除中…");
                    }
                    if ui
                        .add_enabled(
                            n > 0 && !self.scanning && !self.deleting,
                            egui::Button::new("移到回收站"),
                        )
                        .clicked()
                    {
                        let paths = self.selected_paths();
                        if any_sensitive(&paths) {
                            self.confirm_sensitive = true;
                        } else {
                            self.confirm_delete = true;
                        }
                    }
                    if ui
                        .add_enabled(n > 0, egui::Button::new("打开位置"))
                        .clicked()
                    {
                        if let Some(p) = self.selected_paths().first() {
                            if let Err(e) = paths_ui::open_in_explorer(p) {
                                self.status = e;
                            }
                        }
                    }
                    if ui
                        .add_enabled(n > 0, egui::Button::new("复制路径"))
                        .clicked()
                    {
                        if let Some(p) = self.selected_paths().first() {
                            paths_ui::copy_path_to_clipboard(ctx, p);
                            self.status = "已复制路径".into();
                        }
                    }
                    if ui.button("清除选择").clicked() {
                        self.selected.clear();
                    }
                }
                if self.tab == Tab::Junk {
                    let n = self.junk_hits.iter().filter(|h| h.selected).count();
                    let sz: u64 = self
                        .junk_hits
                        .iter()
                        .filter(|h| h.selected)
                        .map(|h| h.size)
                        .sum();
                    ui.label(format!("勾选 {} 类 · {}", n, format_bytes(sz)));
                    if ui
                        .add_enabled(
                            n > 0 && !self.junk_scanning && !self.junk_cleaning,
                            egui::Button::new("清理勾选项"),
                        )
                        .clicked()
                    {
                        self.confirm_junk = true;
                    }
                    if self.junk_cleaning {
                        ui.spinner();
                        ui.label("清理中…");
                    }
                }
                if self.tab == Tab::Shortcuts {
                    let n = self.broken_shortcuts.iter().filter(|s| s.selected).count();
                    ui.label(format!("勾选 {n} 个失效快捷方式"));
                    if ui
                        .add_enabled(
                            n > 0 && !self.shortcuts_scanning,
                            egui::Button::new("删除勾选"),
                        )
                        .clicked()
                    {
                        self.confirm_shortcuts = true;
                    }
                }
                if self.tab == Tab::Registry {
                    let n = self.orphans.iter().filter(|o| o.selected).count();
                    ui.label(format!("勾选 {n} 个无效注册表项"));
                    if ui
                        .add_enabled(n > 0 && !self.orphans_scanning, egui::Button::new("删除勾选"))
                        .clicked()
                    {
                        self.confirm_orphans = true;
                    }
                }
                if self.tab == Tab::Duplicates {
                    let n: usize = self
                        .dup_groups
                        .iter()
                        .map(|g| g.selected.iter().filter(|&&s| s).count())
                        .sum();
                    ui.label(format!("勾选 {n} 个重复文件"));
                    if ui
                        .add_enabled(n > 0 && !self.dup_scanning && !self.deleting, egui::Button::new("删除勾选"))
                        .clicked()
                    {
                        self.confirm_dup_delete = true;
                    }
                }
                if self.tab == Tab::Tools || (self.tab == Tab::Software && self.leftovers_is_followup)
                {
                    let n = self.leftovers.iter().filter(|h| h.selected).count();
                    if n > 0 {
                        ui.label(format!("勾选 {n} 个残留"));
                        if ui
                            .add_enabled(
                                !self.deleting && !self.leftovers_scanning,
                                egui::Button::new("删除勾选残留"),
                            )
                            .clicked()
                        {
                            self.confirm_leftovers = true;
                        }
                    }
                }
            });
            if !self.op_log.is_empty() {
                egui::CollapsingHeader::new(format!("操作日志 ({})", self.op_log.len()))
                    .default_open(self.show_log)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(120.0)
                            .show(ui, |ui| {
                                for line in self.op_log.iter().take(40) {
                                    ui.monospace(line);
                                }
                            });
                        if ui.button("清空日志").clicked() {
                            self.op_log.clear();
                        }
                    });
            }
            if !self.last_error.is_empty() {
                ui.collapsing("详细错误", |ui| {
                    ui.monospace(&self.last_error);
                });
            }
        });

        match self.tab {
            Tab::Overview => self.ui_overview(ctx),
            Tab::Browse => self.ui_browse(ctx),
            Tab::TopN => self.ui_topn(ctx),
            Tab::Types => self.ui_types(ctx),
            Tab::Junk => self.ui_junk(ctx),
            Tab::Software => self.ui_software(ctx),
            Tab::Startup => self.ui_startup(ctx),
            Tab::Shortcuts => self.ui_shortcuts(ctx),
            Tab::Registry => self.ui_registry(ctx),
            Tab::Duplicates => self.ui_duplicates(ctx),
            Tab::Tools => self.ui_tools(ctx),
            Tab::Settings => self.ui_settings(ctx),
            Tab::About => self.ui_about(ctx),
        }

        self.ui_dialogs(ctx);
    }
}

impl JanitorApp {
    fn ui_overview(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(egui::Margin::same(16)),
            )
            .show(ctx, |ui| {
                theme::section_title(
                    ui,
                    "磁盘总览",
                    "无需全盘扫描即可查看各盘占用；点击盘符可设为扫描根路径。",
                );
                theme::card_frame().show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("快捷续扫与安全清理")
                            .color(ACCENT)
                            .strong()
                            .size(15.0),
                    );
                    if self.config.last_scan_summary.trim().is_empty() {
                        ui.label(
                            egui::RichText::new("尚无上次扫描记录。完成一次扫描后可在此续扫。")
                                .color(MUTED)
                                .size(13.0),
                        );
                    } else {
                        ui.label(format!("上次：{}", self.config.last_scan_summary));
                        if !self.config.last_scan_root.is_empty() {
                            ui.weak(format!("路径：{}", self.config.last_scan_root));
                        }
                    }
                    let dup_waste: u64 = self.dup_groups.iter().map(|g| g.waste()).sum();
                    let has_junk = !self.junk_hits.is_empty();
                    let has_dup = !self.dup_groups.is_empty();
                    if has_junk || has_dup {
                        let est = reclaimable_estimate(&self.junk_hits, dup_waste);
                        let junk_sel: u64 = self
                            .junk_hits
                            .iter()
                            .filter(|h| h.selected)
                            .map(|h| h.size)
                            .sum();
                        let safe = safe_selected_size(&self.junk_hits);
                        ui.label(format!(
                            "可回收估算约 {}（已勾选垃圾 {}，其中安全项 {}，重复可省约 {}）",
                            format_bytes(est),
                            format_bytes(junk_sel),
                            format_bytes(safe),
                            format_bytes(dup_waste)
                        ));
                    } else {
                        ui.label(
                            egui::RichText::new("可回收估算：请先在「垃圾建议」或「重复文件」扫描。")
                                .color(MUTED)
                                .size(13.0),
                        );
                    }
                    ui.horizontal_wrapped(|ui| {
                        let can_resume = !self.config.last_scan_root.trim().is_empty() && !self.busy();
                        if ui
                            .add_enabled(can_resume, theme::accent_button("续扫上次路径"))
                            .clicked()
                        {
                            self.resume_last_scan();
                        }
                        if ui
                            .add_enabled(!self.busy(), theme::ghost_button("一键安全清理"))
                            .clicked()
                        {
                            if self.junk_hits.is_empty() {
                                self.status = "请先到「垃圾建议」扫描后再一键安全清理".into();
                                self.tab = Tab::Junk;
                            } else {
                                apply_safe_selection(&mut self.junk_hits);
                                if junk_selected_paths(&self.junk_hits).is_empty() {
                                    self.status = "当前没有可安全清理的项".into();
                                } else {
                                    self.confirm_safe_clean = true;
                                }
                            }
                        }
                        if ui.add(theme::ghost_button("打开回收站")).clicked() {
                            if let Err(e) = paths_ui::open_recycle_bin() {
                                self.status = e;
                            }
                        }
                    });
                });
                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.add(theme::accent_button("刷新盘符")).clicked() {
                        self.refresh_drives();
                    }
                    ui.label(
                        egui::RichText::new(format!("共 {} 个可用卷", self.drive_infos.len()))
                            .color(MUTED),
                    );
                    let n_sel = self.selected_drives.len();
                    let queue_label = format!("多盘排队扫描（{n_sel}）");
                    if ui
                        .add_enabled(
                            n_sel > 0 && !self.busy(),
                            theme::ghost_button(&queue_label),
                        )
                        .clicked()
                    {
                        let roots: Vec<PathBuf> = self
                            .selected_drives
                            .iter()
                            .map(PathBuf::from)
                            .collect();
                        self.start_queued_drive_scans(roots, false);
                    }
                    if !self.drive_scan_queue.is_empty() {
                        ui.colored_label(
                            WARN,
                            format!("排队剩余 {} 盘", self.drive_scan_queue.len()),
                        );
                    }
                });
                ui.add_space(8.0);
                if self.drive_infos.is_empty() {
                    ui.label("未检测到可用磁盘。");
                    return;
                }
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for d in self.drive_infos.clone() {
                        let key = d.root.display().to_string();
                        theme::card_frame().show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let mut checked = self.selected_drives.contains(&key);
                                if ui.checkbox(&mut checked, "").changed() {
                                    if checked {
                                        self.selected_drives.insert(key.clone());
                                    } else {
                                        self.selected_drives.remove(&key);
                                    }
                                }
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new(&d.label)
                                            .color(ACCENT)
                                            .strong()
                                            .size(16.0),
                                    );
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{} · {}",
                                            d.kind,
                                            d.root.display()
                                        ))
                                        .color(MUTED)
                                        .size(12.0),
                                    );
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .add_enabled(
                                                !self.busy(),
                                                theme::accent_button("极速扫描此盘"),
                                            )
                                            .clicked()
                                        {
                                            self.root_input = d.root.display().to_string();
                                            self.start_scan_with_opts(true);
                                        }
                                        if ui.add(theme::ghost_button("扫描此盘")).clicked() {
                                            self.root_input = d.root.display().to_string();
                                            self.start_scan_with_opts(false);
                                        }
                                        if ui.add(theme::ghost_button("设为路径")).clicked() {
                                            self.root_input = d.root.display().to_string();
                                            self.status =
                                                format!("已选择 {}", d.root.display());
                                        }
                                    },
                                );
                            });
                            ui.add_space(6.0);
                            ui.add(
                                egui::ProgressBar::new(d.used_ratio())
                                    .text(d.summary())
                                    .desired_width(ui.available_width()),
                            );
                        });
                        ui.add_space(10.0);
                    }
                });
            });
    }

    fn ui_browse(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("tree")
            .default_width(260.0)
            .show(ctx, |ui| {
                ui.heading("目录树");
                if let Some(idx) = &self.index {
                    let root = idx.root.clone();
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        draw_tree(ui, self, root, 0);
                    });
                } else {
                    ui.label("扫描后显示");
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("⬆ 上级").clicked() {
                    if let Some(p) = self.current_dir.parent() {
                        if let Some(idx) = &self.index {
                            if p.starts_with(&idx.root) || *p == idx.root {
                                self.current_dir = p.to_path_buf();
                            }
                        }
                    }
                }
                ui.monospace(self.current_dir.display().to_string());
                let mut expand_here: Option<PathBuf> = None;
                if let Some(idx) = &self.index {
                    if let Some(e) = idx.get(&self.current_dir) {
                        ui.label(format!(
                            "占用 {}{}",
                            format_bytes(e.size),
                            if idx.partial { "（扫描中估算）" } else { "" }
                        ));
                        if e.count_only {
                            ui.colored_label(WARN, "未展开");
                            if ui
                                .add_enabled(!self.busy(), theme::accent_button("深入展开"))
                                .clicked()
                            {
                                expand_here = Some(self.current_dir.clone());
                            }
                        }
                    }
                    if idx.skipped_bytes > 0 {
                        ui.colored_label(
                            WARN,
                            format!("跳过估算 {}", format_bytes(idx.skipped_bytes)),
                        );
                    }
                }
                if let Some(p) = expand_here {
                    self.start_expand_count_only(p);
                }
            });
            ui.separator();
            if self.config.show_treemap {
                if let Some(idx) = &self.index {
                    ui.weak("占用占比（点击文件夹进入）");
                    if let Some(p) =
                        treemap::show_treemap(ui, idx, &self.current_dir, 200.0)
                    {
                        self.current_dir = p;
                    }
                    ui.separator();
                }
            }
            let entries = self.take_visible_entries();
            if self.list_total > self.list_limit {
                ui.colored_label(
                    egui::Color32::from_rgb(160, 120, 40),
                    format!(
                        "仅显示排序最前 {} / 共 {} 项（提高「显示前」可看更多）",
                        self.list_limit, self.list_total
                    ),
                );
            } else if self.list_total > 0 {
                ui.weak(format!("共 {} 项", self.list_total));
            }
            self.ui_entry_list(ui, entries);
        });
    }

    fn ui_topn(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(egui::Margin::same(16)),
            )
            .show(ctx, |ui| {
                let Some(idx) = self.index.clone() else {
                    theme::section_title(ui, "最大占用", "请先扫描一个路径");
                    return;
                };
                theme::section_title(ui, "最大占用", "按体积排序的热点，便于快速下手清理。");
                ui.heading(
                    egui::RichText::new("最大的文件夹（Top 40）")
                        .color(ACCENT)
                        .size(15.0),
                );
                let dirs: Vec<FsEntry> = idx.top_by_size(true, 40).into_iter().cloned().collect();
                self.ui_entry_list(ui, dirs);
                ui.add_space(12.0);
                ui.heading(
                    egui::RichText::new("最大的文件（Top 40）")
                        .color(ACCENT)
                        .size(15.0),
                );
                let files: Vec<FsEntry> =
                    idx.top_by_size(false, 40).into_iter().cloned().collect();
                self.ui_entry_list(ui, files);
                ui.add_space(12.0);
                ui.heading(
                    egui::RichText::new("空文件夹（最多 80）")
                        .color(ACCENT)
                        .size(15.0),
                );
                ui.label(
                    egui::RichText::new("占用为 0 的目录，可勾选后移到回收站。").color(MUTED),
                );
                let empty: Vec<FsEntry> = idx.empty_dirs(80).into_iter().cloned().collect();
                if empty.is_empty() {
                    ui.label("未发现空文件夹");
                } else {
                    self.ui_entry_list(ui, empty);
                }
            });
    }

    fn ui_types(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(egui::Margin::same(16)),
            )
            .show(ctx, |ui| {
                theme::section_title(
                    ui,
                    "按类型筛选",
                    "基于已扫描索引，按扩展名粗分大文件（视频 / 安装包等）。",
                );
                if self.index.is_none() {
                    ui.label("请先扫描一个路径。");
                    return;
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label("类型");
                    for k in FileKind::all() {
                        if ui
                            .selectable_label(self.file_kind == *k, k.label())
                            .clicked()
                        {
                            self.file_kind = *k;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("最小体积");
                    for mb in [1u64, 10, 50, 100] {
                        if ui
                            .selectable_label(self.type_min_mb == mb, format!("{mb} MB"))
                            .clicked()
                        {
                            self.type_min_mb = mb;
                        }
                    }
                });
                ui.separator();
                let min = self.type_min_mb * 1024 * 1024;
                let limit = self.list_limit.max(50);
                let entries: Vec<FsEntry> = if let Some(idx) = &self.index {
                    filter_files(idx.entries.values(), self.file_kind, min, limit)
                        .into_iter()
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                };
                ui.label(format!(
                    "显示 {} 项（{} · ≥ {}）",
                    entries.len(),
                    self.file_kind.label(),
                    format_bytes(min)
                ));
                self.ui_entry_list(ui, entries);
            });
    }

    fn ui_junk(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("垃圾建议");
            ui.label(
                "清理时删除目录内文件（保留 Temp 根目录）。优先进回收站；正在使用的文件会跳过。",
            );
            ui.colored_label(
                theme::WARN,
                "标「敏感」的项默认不勾选（如 Prefetch、系统 Temp），乱清可能影响开机或系统行为。",
            );
            if self.junk_scanning || self.junk_cleaning {
                ui.spinner();
                ui.label(if self.junk_cleaning {
                    "清理中，请稍候…"
                } else {
                    "扫描中…"
                });
                return;
            }
            if self.junk_hits.is_empty() {
                ui.label("点击上方「垃圾建议」开始。");
                return;
            }
            ui.horizontal(|ui| {
                if ui.button("仅勾选安全项").clicked() {
                    apply_safe_selection(&mut self.junk_hits);
                    let sz = safe_selected_size(&self.junk_hits);
                    self.status = format!("已勾选安全项，合计约 {}", format_bytes(sz));
                }
                if ui
                    .add_enabled(!self.busy(), theme::accent_button("一键安全清理"))
                    .clicked()
                {
                    apply_safe_selection(&mut self.junk_hits);
                    if junk_selected_paths(&self.junk_hits).is_empty() {
                        self.status = "当前没有可安全清理的项".into();
                    } else {
                        self.confirm_safe_clean = true;
                    }
                }
            });
            let mut open_path: Option<PathBuf> = None;
            let mut copy_path: Option<PathBuf> = None;
            egui::ScrollArea::vertical().show(ui, |ui| {
                for h in &mut self.junk_hits {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut h.selected, "");
                            ui.strong(&h.title);
                            ui.label(format_bytes(h.size));
                            if h.sensitive {
                                ui.colored_label(theme::DANGER, "敏感·勿盲目勾选");
                            }
                            if h.rule_id.contains("cookie") || h.rule_id.contains("history") {
                                ui.colored_label(theme::WARN, "隐私·默认不勾选");
                            }
                        });
                        ui.label(&h.detail);
                        if !h.note.is_empty() {
                            ui.weak(&h.note);
                        }
                        for p in h.paths.iter().take(3) {
                            ui.horizontal(|ui| {
                                ui.monospace(p.display().to_string());
                                if ui.small_button("打开位置").clicked() {
                                    open_path = Some(p.clone());
                                }
                                if ui.small_button("复制").clicked() {
                                    copy_path = Some(p.clone());
                                }
                            });
                        }
                        if h.paths.len() > 3 {
                            ui.weak(format!("…另有 {} 项", h.paths.len() - 3));
                        }
                    });
                }
            });
            if let Some(p) = open_path {
                if let Err(e) = paths_ui::open_in_explorer(&p) {
                    self.status = e;
                }
            }
            if let Some(p) = copy_path {
                paths_ui::copy_path_to_clipboard(ctx, &p);
                self.status = "已复制路径".into();
            }
        });
    }

    fn ui_software(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("软件卸载");
            ui.label(
                "调用官方卸载程序。建议：完成卸载向导后再点「我已卸完再跟扫」，比全盘粗扫准得多。",
            );
            if let Some(hint) = self.awaiting_uninstall_done.clone() {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        theme::WARN,
                        format!("请完成「{}」卸载向导后点击：", hint.display_name),
                    );
                    if ui
                        .add_enabled(!self.busy(), theme::accent_button("我已卸完再跟扫"))
                        .clicked()
                    {
                        self.awaiting_uninstall_done = None;
                        self.start_leftovers_followup(hint);
                    }
                    if ui.small_button("取消等待").clicked() {
                        self.awaiting_uninstall_done = None;
                    }
                });
            } else if let Some(hint) = self.pending_followup.clone() {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        theme::WARN,
                        format!("待跟扫：{}", hint.display_name),
                    );
                    if ui
                        .add_enabled(!self.busy(), theme::accent_button("跟扫残留"))
                        .clicked()
                    {
                        self.start_leftovers_followup(hint);
                    }
                    if ui.small_button("清除提醒").clicked() {
                        self.pending_followup = None;
                    }
                });
            }
            if self.leftovers_scanning && self.leftovers_is_followup {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("正在跟扫残留…");
                });
            }
            if self.leftovers_is_followup && !self.leftovers.is_empty() {
                ui.separator();
                ui.heading("跟扫残留结果");
                ui.colored_label(
                    WARN,
                    "默认全不勾选。确认后再清理；误报可点「忽略」加入白名单。",
                );
                self.ui_leftovers_hits(ui, ctx);
                ui.separator();
            }
            ui.horizontal(|ui| {
                ui.label("搜索");
                ui.add(
                    egui::TextEdit::singleline(&mut self.app_filter)
                        .desired_width(220.0)
                        .hint_text("名称 / 发布者"),
                );
                ui.checkbox(&mut self.prefer_quiet_uninstall, "优先静默卸载（若有）");
                if ui
                    .add_enabled(!self.busy(), egui::Button::new("刷新列表"))
                    .clicked()
                {
                    self.start_software_scan();
                }
                if ui
                    .add_enabled(!self.busy(), egui::Button::new("刷新 Store 应用"))
                    .clicked()
                {
                    self.start_appx_list();
                }
            });
            if self.appx_loading {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("正在读取 AppX…");
                });
            }
            if !self.appx_packages.is_empty() {
                ui.collapsing("Store / AppX 应用", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("筛选");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.appx_filter)
                                .desired_width(200.0)
                                .hint_text("包名"),
                        );
                    });
                    let filter = self.appx_filter.to_lowercase();
                    let mut to_remove: Option<String> = None;
                    egui::ScrollArea::vertical()
                        .max_height(160.0)
                        .show(ui, |ui| {
                            for pkg in &self.appx_packages {
                                if !filter.is_empty()
                                    && !pkg.name.to_lowercase().contains(&filter)
                                    && !pkg.package_full_name.to_lowercase().contains(&filter)
                                {
                                    continue;
                                }
                                ui.horizontal(|ui| {
                                    ui.label(&pkg.name);
                                    ui.weak(&pkg.package_full_name);
                                    if ui.small_button("卸载 AppX").clicked() {
                                        to_remove = Some(pkg.package_full_name.clone());
                                    }
                                });
                            }
                        });
                    if let Some(full) = to_remove {
                        match uninstall_appx(&full) {
                            Ok(()) => {
                                self.status = format!("已请求卸载 AppX：{full}");
                                self.appx_packages
                                    .retain(|p| p.package_full_name != full);
                            }
                            Err(e) => self.status = format!("卸载 AppX 失败：{e}"),
                        }
                    }
                });
            }
            if self.deep_loading {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("深度查询中…");
                });
            }
            if !self.locking_procs.is_empty() {
                ui.collapsing(
                    format!("占用进程（{}）", self.locking_procs.len()),
                    |ui| {
                        for p in &self.locking_procs {
                            ui.label(format!("[{}] {} — {}", p.pid, p.name, p.path));
                        }
                    },
                );
            }
            if !self.related_services.is_empty() || !self.related_tasks.is_empty() {
                ui.collapsing("相关服务 / 计划任务", |ui| {
                    for s in &self.related_services {
                        ui.label(format!(
                            "服务 {} ({}) — {}",
                            s.name, s.status, s.display_name
                        ));
                    }
                    for t in &self.related_tasks {
                        ui.label(format!("任务 {}{} — {}", t.path, t.name, t.state));
                    }
                });
            }
            if self.apps_loading {
                ui.spinner();
                ui.label("读取中…");
                return;
            }
            if self.apps.is_empty() {
                ui.label("点击「刷新软件」或「刷新列表」加载。");
                return;
            }
            let filter = self.app_filter.to_lowercase();
            let quiet = self.prefer_quiet_uninstall;
            let mut to_uninstall: Option<usize> = None;
            let mut to_follow: Option<usize> = None;
            let mut to_lock: Option<usize> = None;
            let mut to_svc: Option<usize> = None;
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (i, app) in self.apps.iter().enumerate() {
                    if !filter.is_empty() {
                        let hay = format!(
                            "{} {} {}",
                            app.display_name, app.publisher, app.version
                        )
                        .to_lowercase();
                        if !hay.contains(&filter) {
                            continue;
                        }
                    }
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.strong(&app.display_name);
                            ui.label(app.hive);
                            if !app.version.is_empty() {
                                ui.label(&app.version);
                            }
                            if app.estimated_size > 0 {
                                ui.label(format_bytes(app.estimated_size));
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button("卸载").clicked() {
                                    to_uninstall = Some(i);
                                }
                                if ui
                                    .add_enabled(!self.busy(), egui::Button::new("查残留"))
                                    .clicked()
                                {
                                    to_follow = Some(i);
                                }
                                if ui
                                    .add_enabled(!self.busy(), egui::Button::new("查占用进程"))
                                    .clicked()
                                {
                                    to_lock = Some(i);
                                }
                                if ui
                                    .add_enabled(!self.busy(), egui::Button::new("查服务/任务"))
                                    .clicked()
                                {
                                    to_svc = Some(i);
                                }
                            });
                        });
                        if !app.publisher.is_empty() {
                            ui.weak(&app.publisher);
                        }
                        if !app.install_location.is_empty() {
                            ui.monospace(&app.install_location);
                        }
                        ui.weak(&app.reg_path);
                    });
                }
            });
            if let Some(i) = to_lock {
                if let Some(app) = self.apps.get(i).cloned() {
                    self.start_deep_lock_check(&app);
                }
            }
            if let Some(i) = to_svc {
                if let Some(app) = self.apps.get(i).cloned() {
                    self.start_deep_svc_check(&app);
                }
            }
            if let Some(i) = to_follow {
                if let Some(app) = self.apps.get(i).cloned() {
                    self.start_leftovers_followup(AppLeftoverHint::from_app(&app));
                }
            }
            if let Some(i) = to_uninstall {
                if let Some(app) = self.apps.get(i).cloned() {
                    let hint = AppLeftoverHint::from_app(&app);
                    match launch_uninstall(&app, quiet) {
                        Ok(()) => {
                            self.pending_followup = Some(hint.clone());
                            self.awaiting_uninstall_done = Some(hint);
                            self.status = format!(
                                "已启动卸载：{}。完成后点「我已卸完再跟扫」。",
                                app.display_name
                            );
                            self.push_log(format!("启动卸载 {}", app.display_name));
                        }
                        Err(e) => {
                            self.status = format!("启动卸载失败：{e}");
                            self.last_error = e;
                        }
                    }
                }
            }
        });
    }

    /// 残留列表：选择 / 清理确认 / 白名单（软件页与工具页共用）
    fn ui_leftovers_hits(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            if ui.button("全不选").clicked() {
                for h in &mut self.leftovers {
                    h.selected = false;
                }
            }
            if ui.button("仅勾选高置信").clicked() {
                for h in &mut self.leftovers {
                    h.selected = h.confidence == Confidence::High;
                }
            }
            let n = self.leftovers.iter().filter(|h| h.selected).count();
            if ui
                .add_enabled(n > 0 && !self.busy(), theme::accent_button("清理勾选项"))
                .clicked()
            {
                self.confirm_leftovers = true;
            }
            ui.label(format!("白名单 {} 条", self.whitelist.paths.len()));
        });
        let mut ignore_idx: Option<usize> = None;
        let mut open_path: Option<PathBuf> = None;
        let mut copy_path: Option<PathBuf> = None;
        for (i, h) in self.leftovers.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.checkbox(&mut h.selected, "");
                let (label, color) = match h.confidence {
                    Confidence::High => ("高", OK),
                    Confidence::Medium => ("中", WARN),
                    Confidence::Low => ("低", DANGER),
                };
                ui.colored_label(color, label);
                ui.label(format_bytes(h.size_hint));
                ui.monospace(h.path.display().to_string());
                if ui.small_button("忽略").clicked() {
                    ignore_idx = Some(i);
                }
                if ui.small_button("打开").clicked() {
                    open_path = Some(h.path.clone());
                }
                if ui.small_button("复制").clicked() {
                    copy_path = Some(h.path.clone());
                }
            });
            ui.weak(&h.reason);
        }
        if let Some(i) = ignore_idx {
            if let Some(h) = self.leftovers.get(i).cloned() {
                if self.whitelist.add(&h.path) {
                    let _ = self.whitelist.save();
                    self.push_log(format!("已加入白名单：{}", h.path.display()));
                }
                self.leftovers.remove(i);
                self.status = "已忽略并加入白名单".into();
            }
        }
        if let Some(p) = open_path {
            if let Err(e) = paths_ui::open_in_explorer(&p) {
                self.status = e;
            }
        }
        if let Some(p) = copy_path {
            paths_ui::copy_path_to_clipboard(ctx, &p);
            self.status = "已复制路径".into();
        }
    }

    fn ui_startup(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("开机自启");
            ui.label(
                "管理注册表 Run 与启动文件夹项。禁用会移出启动位置并可恢复；删除不可恢复（快捷方式进回收站）。",
            );
            ui.horizontal(|ui| {
                ui.label("搜索");
                ui.add(
                    egui::TextEdit::singleline(&mut self.startup_filter)
                        .desired_width(200.0)
                        .hint_text("名称 / 命令"),
                );
                if ui
                    .add_enabled(!self.busy(), egui::Button::new("刷新"))
                    .clicked()
                {
                    self.start_startup_scan();
                }
            });
            if self.startup_loading {
                ui.spinner();
                ui.label("读取中…");
                return;
            }
            if self.startup_items.is_empty() {
                ui.label("点击上方「开机自启」或「刷新」加载。");
                return;
            }

            let filter = self.startup_filter.to_lowercase();
            let mut action: Option<(usize, &'static str)> = None;

            egui::ScrollArea::vertical().show(ui, |ui| {
                for (i, item) in self.startup_items.iter().enumerate() {
                    if !filter.is_empty() {
                        let hay = format!("{} {} {}", item.name, item.command, item.location)
                            .to_lowercase();
                        if !hay.contains(&filter) {
                            continue;
                        }
                    }
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            if item.enabled {
                                ui.colored_label(egui::Color32::from_rgb(40, 140, 70), "启用");
                            } else {
                                ui.colored_label(egui::Color32::from_rgb(140, 100, 40), "已禁用");
                            }
                            ui.strong(&item.name);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("删除").clicked() {
                                    action = Some((i, "delete"));
                                }
                                if item.enabled {
                                    if ui.small_button("禁用").clicked() {
                                        action = Some((i, "disable"));
                                    }
                                } else if ui.small_button("启用").clicked() {
                                    action = Some((i, "enable"));
                                }
                            });
                        });
                        ui.weak(&item.location);
                        ui.monospace(&item.command);
                    });
                }
            });

            if let Some((i, act)) = action {
                if let Some(item) = self.startup_items.get(i).cloned() {
                    let result = match act {
                        "disable" => disable_startup(&item).map(|_| "已禁用"),
                        "enable" => enable_startup(&item).map(|_| "已启用"),
                        "delete" => delete_startup(&item).map(|_| "已删除"),
                        _ => Err("未知操作".into()),
                    };
                    match result {
                        Ok(msg) => {
                            self.push_log(format!("{msg} 启动项：{}", item.name));
                            self.status = format!("{msg}：{}", item.name);
                            self.start_startup_scan();
                        }
                        Err(e) => {
                            self.push_log(format!("启动项操作失败 {}: {e}", item.name));
                            self.status = format!("操作失败：{e}");
                            self.last_error = e;
                        }
                    }
                }
            }
        });
    }

    fn ui_shortcuts(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("失效快捷方式");
            ui.label("扫桌面 + 开始菜单一层 .lnk（开始菜单会再进一层文件夹），约数秒内结束；与上方路径无关。");
            if self.shortcuts_scanning {
                ui.spinner();
                ui.label("扫描中…");
                return;
            }
            if self.broken_shortcuts.is_empty() {
                ui.label("点击上方「扫快捷方式」开始。");
                return;
            }
            ui.horizontal(|ui| {
                if ui.button("全选").clicked() {
                    for s in &mut self.broken_shortcuts {
                        s.selected = true;
                    }
                }
                if ui.button("全不选").clicked() {
                    for s in &mut self.broken_shortcuts {
                        s.selected = false;
                    }
                }
            });
            egui::ScrollArea::vertical().show(ui, |ui| {
                for s in &mut self.broken_shortcuts {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut s.selected, "");
                            ui.strong(
                                s.path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| s.path.display().to_string()),
                            );
                        });
                        ui.weak(format!("位置: {}", s.location));
                        ui.monospace(s.path.display().to_string());
                        ui.colored_label(
                            egui::Color32::from_rgb(180, 90, 60),
                            format!("目标不存在: {}", s.target),
                        );
                    });
                }
            });
        });
    }

    fn ui_registry(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("无效卸载注册表");
            ui.label(
                "仅列出「安装目录或卸载程序已不存在」的 Uninstall 项。删除 HKLM 项通常需要管理员权限。",
            );
            if self.orphans_scanning {
                ui.spinner();
                ui.label("扫描中…");
                return;
            }
            if self.orphans.is_empty() {
                ui.label("点击上方「扫注册表」开始。");
                return;
            }
            ui.horizontal(|ui| {
                if ui.button("全选").clicked() {
                    for o in &mut self.orphans {
                        o.selected = true;
                    }
                }
                if ui.button("全不选").clicked() {
                    for o in &mut self.orphans {
                        o.selected = false;
                    }
                }
            });
            egui::ScrollArea::vertical().show(ui, |ui| {
                for o in &mut self.orphans {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut o.selected, "");
                            ui.strong(&o.display_name);
                            ui.label(o.hive);
                        });
                        ui.colored_label(egui::Color32::from_rgb(180, 90, 60), &o.reason);
                        ui.monospace(&o.full_path);
                    });
                }
            });
        });
    }

    fn ui_duplicates(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(egui::Margin::same(16)),
            )
            .show(ctx, |ui| {
                theme::section_title(
                    ui,
                    "重复文件",
                    "在已扫描索引中按大小+哈希查找（≥1MB，最多 80 组）。可按策略自动勾选待删项。",
                );
                ui.horizontal(|ui| {
                    ui.label("保留策略");
                    let mut changed = false;
                    egui::ComboBox::from_id_salt("dup_keep_strategy")
                        .selected_text(self.config.dup_keep_strategy.label())
                        .show_ui(ui, |ui| {
                            for s in KeepStrategy::all() {
                                if ui
                                    .selectable_value(
                                        &mut self.config.dup_keep_strategy,
                                        *s,
                                        s.label(),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                            }
                        });
                    if changed {
                        let _ = self.config.save();
                    }
                    if ui.button("按策略重算勾选").clicked() {
                        let strategy = self.config.dup_keep_strategy;
                        for g in &mut self.dup_groups {
                            apply_keep_strategy(g, strategy);
                        }
                        self.status = format!("已按「{}」重算勾选", strategy.label());
                    }
                    if ui
                        .add_enabled(!self.busy(), theme::accent_button("开始查重"))
                        .clicked()
                    {
                        self.start_duplicates_scan();
                    }
                    if self.dup_scanning {
                        ui.spinner();
                        ui.label("查重中…");
                    }
                });
                if self.dup_groups.is_empty() && !self.dup_scanning {
                    ui.label("点击「开始查重」。需先完成扫描。");
                    return;
                }
                let mut open_path: Option<PathBuf> = None;
                let mut copy_path: Option<PathBuf> = None;
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for g in &mut self.dup_groups {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.strong(format!(
                                    "{} × {} · 可省 {}",
                                    format_bytes(g.size),
                                    g.paths.len(),
                                    format_bytes(g.waste())
                                ));
                                ui.weak(format!("sha256:{}…", &g.hash[..g.hash.len().min(12)]));
                            });
                            let n = g.paths.len();
                            for i in 0..n {
                                ui.horizontal(|ui| {
                                    if let Some(sel) = g.selected.get_mut(i) {
                                        ui.checkbox(sel, "");
                                    }
                                    if let Some(p) = g.paths.get(i) {
                                        ui.monospace(p.display().to_string());
                                        if !g.selected.get(i).copied().unwrap_or(false) {
                                            ui.weak("（保留）");
                                        }
                                        if ui.small_button("打开").clicked() {
                                            open_path = Some(p.clone());
                                        }
                                        if ui.small_button("复制").clicked() {
                                            copy_path = Some(p.clone());
                                        }
                                    }
                                });
                            }
                        });
                    }
                });
                if let Some(p) = open_path {
                    if let Err(e) = paths_ui::open_in_explorer(&p) {
                        self.status = e;
                    }
                }
                if let Some(p) = copy_path {
                    paths_ui::copy_path_to_clipboard(ctx, &p);
                    self.status = "已复制路径".into();
                }
            });
    }

    fn ui_tools(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(egui::Margin::same(16)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    theme::section_title(ui, "工具箱", "导出、对比、残留、回收站与提权。");

                    theme::card_frame().show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("导出扫描结果")
                                .color(ACCENT)
                                .strong()
                                .size(15.0),
                        );
                        ui.label(
                            egui::RichText::new("导出到桌面（最多约 5 万行）。")
                                .color(MUTED)
                                .size(13.0),
                        );
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(self.index.is_some(), theme::ghost_button("导出 CSV"))
                                .clicked()
                            {
                                self.export_scan(false);
                            }
                            if ui
                                .add_enabled(self.index.is_some(), theme::ghost_button("导出 JSON"))
                                .clicked()
                            {
                                self.export_scan(true);
                            }
                        });
                    });

                    ui.add_space(12.0);
                    theme::card_frame().show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("对比基线")
                                .color(ACCENT)
                                .strong()
                                .size(15.0),
                        );
                        ui.label(
                            egui::RichText::new(
                                "保存当前扫描为基线，之后再次扫描可对比体积变化（≥1MB）。",
                            )
                            .color(MUTED)
                            .size(13.0),
                        );
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(
                                    self.index.is_some(),
                                    theme::accent_button("保存对比基线"),
                                )
                                .clicked()
                            {
                                self.save_baseline();
                            }
                            if ui
                                .add_enabled(
                                    self.index.is_some(),
                                    theme::ghost_button("与上次基线对比"),
                                )
                                .clicked()
                            {
                                self.compare_baseline();
                            }
                        });
                        if !self.diff_items.is_empty() {
                            ui.add_space(6.0);
                            ui.label(format!("显著变化（前 {}）", self.diff_items.len()));
                            for d in self.diff_items.iter().take(40) {
                                ui.horizontal(|ui| {
                                    let color = if d.delta >= 0 {
                                        theme::DANGER
                                    } else {
                                        OK
                                    };
                                    ui.colored_label(color, format_delta(d.delta));
                                    ui.weak(format!(
                                        "{} → {}",
                                        format_bytes(d.old_size),
                                        format_bytes(d.new_size)
                                    ));
                                    ui.monospace(&d.path);
                                });
                            }
                        }
                    });

                    ui.add_space(12.0);
                    theme::card_frame().show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("卸载残留")
                                .color(ACCENT)
                                .strong()
                                .size(15.0),
                        );
                        ui.label(
                            egui::RichText::new(
                                "推荐：在「软件卸载」点卸载后「跟扫残留」，或对单个软件点「查残留」。",
                            )
                            .color(MUTED)
                            .size(13.0),
                        );
                        ui.colored_label(
                            WARN,
                            "下方「粗扫」只扫 Program Files，仍可能误报；全部默认不勾选。",
                        );
                        if let Some(hint) = self.pending_followup.clone() {
                            ui.horizontal(|ui| {
                                ui.label(format!("待跟扫：{}", hint.display_name));
                                if ui
                                    .add_enabled(!self.busy(), theme::accent_button("跟扫此软件"))
                                    .clicked()
                                {
                                    self.awaiting_uninstall_done = None;
                                    self.start_leftovers_followup(hint);
                                }
                            });
                        }
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(!self.busy(), theme::ghost_button("粗扫 Program Files"))
                                .clicked()
                            {
                                self.start_leftovers_scan();
                            }
                            if self.leftovers_scanning {
                                ui.spinner();
                                ui.label("扫描中…");
                            }
                            ui.label(format!("白名单 {} 条", self.whitelist.paths.len()));
                            if ui
                                .add_enabled(
                                    !self.whitelist.paths.is_empty(),
                                    theme::ghost_button("清空白名单"),
                                )
                                .clicked()
                            {
                                self.whitelist.paths.clear();
                                let _ = self.whitelist.save();
                                self.status = "已清空白名单".into();
                            }
                        });
                        if !self.leftovers.is_empty() {
                            self.ui_leftovers_hits(ui, ctx);
                        }
                    });

                    ui.add_space(12.0);
                    theme::card_frame().show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("回收站")
                                .color(ACCENT)
                                .strong()
                                .size(15.0),
                        );
                        ui.label(&self.recycle_bin_label);
                        ui.horizontal(|ui| {
                            if ui.add(theme::ghost_button("估算占用")).clicked() {
                                self.refresh_recycle_bin();
                            }
                            if ui.add(theme::ghost_button("清空回收站")).clicked() {
                                self.confirm_empty_recycle = true;
                            }
                        });
                    });

                    ui.add_space(12.0);
                    theme::card_frame().show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("管理员权限")
                                .color(ACCENT)
                                .strong()
                                .size(15.0),
                        );
                        if self.is_admin {
                            ui.colored_label(OK, "当前已以管理员身份运行");
                        } else {
                            ui.label(
                                egui::RichText::new("部分系统路径清理 / HKLM 注册表需要管理员。")
                                    .color(MUTED)
                                    .size(13.0),
                            );
                            if ui.add(theme::accent_button("以管理员身份重启")).clicked() {
                                match relaunch_as_admin() {
                                    Ok(()) => {
                                        self.push_log("已请求提权重启，即将退出");
                                        std::process::exit(0);
                                    }
                                    Err(e) => {
                                        self.status = format!("提权失败：{e}");
                                        self.last_error = e;
                                    }
                                }
                            }
                        }
                    });
                });
            });
    }

    fn ui_settings(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(egui::Margin::same(18)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    theme::section_title(
                        ui,
                        "设置与更新",
                        &format!(
                            "当前 v{APP_VERSION_NAME}  ·  build #{APP_VERSION_CODE}  ·  配置 %LOCALAPPDATA%\\disk-janitor\\config.json"
                        ),
                    );

                    theme::card_frame().show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("界面与扫描记忆")
                                .color(ACCENT)
                                .strong()
                                .size(15.0),
                        );
                        ui.horizontal(|ui| {
                            ui.label("UI 缩放");
                            for (label, scale) in [
                                ("0.9", 0.9_f32),
                                ("1.0", 1.0),
                                ("1.15", 1.15),
                                ("1.3", 1.3),
                            ] {
                                if ui
                                    .selectable_label(
                                        (self.config.ui_scale - scale).abs() < 0.01,
                                        label,
                                    )
                                    .clicked()
                                {
                                    self.config.ui_scale = scale;
                                    ctx.set_pixels_per_point(scale.clamp(0.85, 2.0));
                                    let _ = self.config.save();
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("重复文件保留策略");
                            let mut changed = false;
                            egui::ComboBox::from_id_salt("settings_dup_keep")
                                .selected_text(self.config.dup_keep_strategy.label())
                                .show_ui(ui, |ui| {
                                    for s in KeepStrategy::all() {
                                        if ui
                                            .selectable_value(
                                                &mut self.config.dup_keep_strategy,
                                                *s,
                                                s.label(),
                                            )
                                            .changed()
                                        {
                                            changed = true;
                                        }
                                    }
                                });
                            if changed {
                                let _ = self.config.save();
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("查重最小体积 (MB)");
                            let mut mb = self.config.dup_min_mb as i64;
                            if ui
                                .add(egui::DragValue::new(&mut mb).range(1..=1024))
                                .changed()
                            {
                                self.config.dup_min_mb = mb as u64;
                                let _ = self.config.save();
                            }
                            ui.label("最多组数");
                            let mut n = self.config.dup_max_groups;
                            if ui
                                .add(egui::DragValue::new(&mut n).range(1..=500))
                                .changed()
                            {
                                self.config.dup_max_groups = n;
                                let _ = self.config.save();
                            }
                        });
                        if ui
                            .checkbox(
                                &mut self.config.quiet_clean_on_start,
                                "启动时安静清理安全垃圾（仅勾选项，非敏感）",
                            )
                            .changed()
                        {
                            let _ = self.config.save();
                        }
                        if ui
                            .checkbox(
                                &mut self.config.show_treemap,
                                "浏览页显示 Treemap 占用图",
                            )
                            .changed()
                        {
                            let _ = self.config.save();
                        }
                        ui.horizontal(|ui| {
                            ui.label("默认扫描模式");
                            let turbo = self.config.scan_mode == "turbo";
                            if ui
                                .selectable_label(!turbo, "普通")
                                .clicked()
                            {
                                self.config.scan_mode = "normal".into();
                                let _ = self.config.save();
                            }
                            if ui.selectable_label(turbo, "极速").clicked() {
                                self.config.scan_mode = "turbo".into();
                                let _ = self.config.save();
                            }
                        });
                        if ui
                            .checkbox(
                                &mut self.config.shred_default,
                                "删除确认默认勾选「安全粉碎」",
                            )
                            .changed()
                        {
                            self.shred_delete = self.config.shred_default;
                            let _ = self.config.save();
                        }
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new("排除路径（每行一条，扫描时跳过此前缀）")
                                .color(MUTED)
                                .size(13.0),
                        );
                        ui.add(
                            egui::TextEdit::multiline(&mut self.exclude_edit)
                                .desired_width(f32::INFINITY)
                                .desired_rows(4)
                                .hint_text(r"例如 D:\HugeRepo\node_modules"),
                        );
                        if ui.add(theme::ghost_button("保存排除列表")).clicked() {
                            self.config.exclude_paths = self
                                .exclude_edit
                                .lines()
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                            match self.config.save() {
                                Ok(()) => {
                                    self.status = format!(
                                        "已保存 {} 条排除路径",
                                        self.config.exclude_paths.len()
                                    )
                                }
                                Err(e) => self.status = format!("保存失败：{e}"),
                            }
                        }
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new("每日安静清理（计划任务）")
                                .color(ACCENT)
                                .strong()
                                .size(14.0),
                        );
                        ui.horizontal(|ui| {
                            ui.label("时间 HH:MM");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.config.schedule_time)
                                    .desired_width(70.0),
                            );
                            let mut en = self.config.schedule_quiet_clean;
                            if ui.checkbox(&mut en, "启用每日安静清理").changed() {
                                self.config.schedule_quiet_clean = en;
                                let exe = std::env::current_exe().ok();
                                if en {
                                    if let Some(exe) = exe {
                                        match schedule::install_daily_task(
                                            &exe,
                                            &self.config.schedule_time,
                                        ) {
                                            Ok(()) => {
                                                self.schedule_status =
                                                    "已安装计划任务 DiskJanitorQuietClean".into();
                                                let _ = self.config.save();
                                            }
                                            Err(e) => {
                                                self.config.schedule_quiet_clean = false;
                                                self.schedule_status = format!("安装失败：{e}");
                                            }
                                        }
                                    }
                                } else {
                                    match schedule::remove_daily_task() {
                                        Ok(()) => {
                                            self.schedule_status = "已移除计划任务".into();
                                            let _ = self.config.save();
                                        }
                                        Err(e) => {
                                            self.schedule_status = format!("移除失败：{e}");
                                        }
                                    }
                                }
                            }
                            if ui.small_button("刷新状态").clicked() {
                                let on = schedule::task_installed();
                                self.config.schedule_quiet_clean = on;
                                self.schedule_status = if on {
                                    "计划任务已安装".into()
                                } else {
                                    "未安装计划任务".into()
                                };
                            }
                        });
                        if !self.schedule_status.is_empty() {
                            ui.weak(&self.schedule_status);
                        }
                        if self.config.last_scan_root.trim().is_empty() {
                            ui.weak("上次扫描路径：无");
                        } else {
                            ui.label(format!("上次扫描路径：{}", self.config.last_scan_root));
                        }
                        if !self.config.last_scan_summary.trim().is_empty() {
                            ui.weak(&self.config.last_scan_summary);
                        }
                    });

                    ui.add_space(14.0);
                    theme::card_frame().show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("远程更新")
                                .color(ACCENT)
                                .strong()
                                .size(15.0),
                        );
                        ui.label(
                            egui::RichText::new(
                                "与 DeskReader 相同：填写 jiaoben 服务器根地址。更新检查可与扫盘并行。",
                            )
                            .color(MUTED)
                            .size(13.0),
                        );
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("服务器").color(MUTED));
                            ui.add(
                                egui::TextEdit::singleline(&mut self.config.update_api_base)
                                    .desired_width(420.0)
                                    .hint_text(DEFAULT_API_BASE),
                            );
                            if ui.add(theme::ghost_button("恢复默认")).clicked() {
                                self.config.update_api_base = DEFAULT_API_BASE.to_string();
                            }
                        });
                        ui.checkbox(&mut self.config.check_on_start, "启动时自动检查更新");
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui.add(theme::ghost_button("保存配置")).clicked() {
                                self.config.normalize();
                                match self.config.save() {
                                    Ok(()) => {
                                        self.update_status = "配置已保存".to_string();
                                        self.push_log("已保存更新配置");
                                    }
                                    Err(e) => {
                                        self.update_status = format!("保存失败：{e}");
                                    }
                                }
                            }
                            if ui
                                .add_enabled(!self.update_checking, theme::accent_button("检查更新"))
                                .clicked()
                            {
                                self.config.normalize();
                                let _ = self.config.save();
                                self.start_update_check();
                            }
                            if self.pending_update.is_some()
                                && ui
                                    .add_enabled(
                                        !self.update_checking,
                                        theme::accent_button("下载并安装"),
                                    )
                                    .clicked()
                            {
                                self.start_update_apply();
                            }
                            if let Some(m) = &self.pending_update {
                                let url = m.url.clone();
                                if !url.is_empty()
                                    && ui.add(theme::ghost_button("打开下载链接")).clicked()
                                {
                                    if let Err(e) = open_url(&url) {
                                        self.update_status = format!("打开失败：{e}");
                                    }
                                }
                            }
                        });
                        if !self.update_status.is_empty() {
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new(&self.update_status).color(OK));
                        }
                        if let Some(m) = &self.pending_update {
                            ui.add_space(6.0);
                            ui.colored_label(WARN, format!("待更新：{}", m.label()));
                            if !m.changelog.is_empty() {
                                ui.label(&m.changelog);
                            }
                            if !m.url.is_empty() {
                                ui.monospace(&m.url);
                            }
                        }
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(format!(
                                "探测：{{base}}/{}/app-update.json  ·  /api/{}/app-update  ·  /api/app-update/{}",
                                crate::updater::APP_KEY,
                                crate::updater::APP_KEY,
                                crate::updater::APP_KEY
                            ))
                            .color(MUTED)
                            .size(12.0),
                        );
                    });

                    ui.add_space(14.0);
                    theme::card_frame().show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("打包分发")
                                .color(ACCENT)
                                .strong()
                                .size(15.0),
                        );
                        ui.label(
                            egui::RichText::new(
                                "一键打包.cmd → release\\DiskJanitor-x.y.z.exe + deploy\\app-update.json，上传到 jiaoben 或静态目录即可。",
                            )
                            .color(MUTED)
                            .size(13.0),
                        );
                    });
                });
            });
    }

    fn ui_about(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(egui::Margin::same(18)),
            )
            .show(ctx, |ui| {
                if self.about_assets.is_none() {
                    self.about_assets = Some(AboutAssets::load(ctx));
                }
                let mut panel = self.about_panel;
                let mut tip = std::mem::take(&mut self.about_tip);
                if let Some(assets) = self.about_assets.as_ref() {
                    about::draw_about(
                        ui,
                        &mut panel,
                        assets,
                        APP_VERSION_NAME,
                        APP_VERSION_CODE,
                        &mut tip,
                    );
                }
                self.about_panel = panel;
                self.about_tip = tip;
            });
    }

    fn ui_entry_list(&mut self, ui: &mut egui::Ui, entries: Vec<FsEntry>) {
        let mut open_dir: Option<PathBuf> = None;
        let mut toggle: Option<(String, bool)> = None;
        let mut open_loc: Option<PathBuf> = None;
        let mut copy_path: Option<PathBuf> = None;
        let mut expand_path: Option<PathBuf> = None;

        let parent_size = self
            .index
            .as_ref()
            .and_then(|idx| idx.get(&self.current_dir))
            .map(|e| e.size)
            .unwrap_or(0);
        let max_in_list = entries.iter().map(|e| e.size).max().unwrap_or(0);
        let bar_den = if parent_size > 0 {
            parent_size
        } else {
            max_in_list.max(1)
        };

        egui::ScrollArea::vertical()
            .max_height(ui.available_height().max(120.0))
            .show(ui, |ui| {
                for e in &entries {
                    let key = ScanIndex::key(&e.path);
                    let checked = self.selected.contains(&key);
                    ui.horizontal(|ui| {
                        let mut c = checked;
                        if ui.checkbox(&mut c, "").changed() {
                            toggle = Some((key.clone(), c));
                        }
                        let label = if e.is_dir {
                            format!("📁 {}", e.name)
                        } else {
                            format!("📄 {}", e.name)
                        };
                        let resp = ui.add(egui::Label::new(&label).sense(egui::Sense::click()));
                        if resp.double_clicked() && e.is_dir {
                            open_dir = Some(e.path.clone());
                        } else if resp.clicked() && e.is_dir {
                            open_dir = Some(e.path.clone());
                        } else if resp.clicked() {
                            toggle = Some((key.clone(), !checked));
                        }
                        if e.count_only {
                            ui.colored_label(WARN, "未展开");
                            if ui
                                .add_enabled(!self.busy(), egui::Button::new("深入展开"))
                                .clicked()
                            {
                                expand_path = Some(e.path.clone());
                            }
                        }
                        let ratio = (e.size as f32 / bar_den as f32).clamp(0.0, 1.0);
                        ui.add(egui::ProgressBar::new(ratio).desired_width(72.0));
                        if ui.small_button("打开").clicked() {
                            open_loc = Some(e.path.clone());
                        }
                        if ui.small_button("复制").clicked() {
                            copy_path = Some(e.path.clone());
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(if e.is_dir { "文件夹" } else { "文件" });
                            ui.monospace(format_mtime(e.mtime));
                            ui.monospace(format!("{:>10}", format_bytes(e.size)));
                        });
                    });
                }
            });

        if let Some(p) = open_dir {
            self.current_dir = p;
            self.tab = Tab::Browse;
        }
        if let Some((k, on)) = toggle {
            if on {
                self.selected.insert(k);
            } else {
                self.selected.remove(&k);
            }
        }
        if let Some(p) = open_loc {
            if let Err(e) = paths_ui::open_in_explorer(&p) {
                self.status = e;
            }
        }
        if let Some(p) = copy_path {
            paths_ui::copy_path_to_clipboard(ui.ctx(), &p);
            self.status = "已复制路径".into();
        }
        if let Some(p) = expand_path {
            self.start_expand_count_only(p);
        }
    }

    fn ui_dialogs(&mut self, ctx: &egui::Context) {
        if self.confirm_sensitive {
            egui::Window::new("系统路径警告")
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("选中项包含敏感系统路径，确定仍要移到回收站？");
                    ui.horizontal(|ui| {
                        if ui.button("取消").clicked() {
                            self.confirm_sensitive = false;
                        }
                        if ui.button("仍然删除").clicked() {
                            self.confirm_sensitive = false;
                            self.confirm_delete = true;
                        }
                    });
                });
        }
        if self.confirm_delete {
            let paths = self.selected_paths();
            let n = paths.len();
            let sz = format_bytes(self.selected_total_size());
            let preview = paths_ui::preview_paths(&paths, 12);
            egui::Window::new("确认删除")
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(format!("将删除 {n} 项（合计 {sz}）。"));
                    ui.label("优先进回收站。结果会写在底部「操作日志」。");
                    ui.group(|ui| {
                        ui.weak("将删除：");
                        ui.monospace(&preview);
                    });
                    ui.checkbox(
                        &mut self.allow_permanent_delete,
                        "回收站失败时允许直接删除",
                    );
                    if ui
                        .checkbox(
                            &mut self.shred_delete,
                            "安全粉碎（仅文件，不可进回收站）",
                        )
                        .changed()
                        && self.shred_delete
                    {
                        self.allow_permanent_delete = true;
                    }
                    if self.shred_delete {
                        ui.colored_label(
                            DANGER,
                            "粉碎将覆写后永久删除，无法从回收站恢复。",
                        );
                    }
                    ui.weak("提示：WSL 的 ext4.vhdx 若占用，请先在终端执行 wsl --shutdown");
                    ui.horizontal(|ui| {
                        if ui.button("取消").clicked() {
                            self.confirm_delete = false;
                        }
                        if ui.button("开始删除").clicked() {
                            self.do_delete();
                        }
                    });
                });
        }
        if self.confirm_junk || self.confirm_safe_clean {
            let paths = junk_selected_paths(&self.junk_hits);
            let sensitive = any_sensitive(&paths);
            let sz: u64 = self
                .junk_hits
                .iter()
                .filter(|h| h.selected)
                .map(|h| h.size)
                .sum();
            let preview = paths_ui::preview_paths(&paths, 12);
            let title = if self.confirm_safe_clean {
                "确认一键安全清理"
            } else {
                "确认清理垃圾"
            };
            egui::Window::new(title)
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    if self.confirm_safe_clean {
                        ui.label("仅清理非敏感默认安全项（Temp / 浏览器缓存 / 缩略图等）。");
                    }
                    ui.label(format!(
                        "将清理 {} 个位置（约 {}）。目录只清内容，不删根文件夹。",
                        paths.len(),
                        format_bytes(sz)
                    ));
                    ui.label("优先进回收站；进不去则直接删除。正在使用的文件会跳过。");
                    ui.group(|ui| {
                        ui.weak("将清理：");
                        ui.monospace(&preview);
                    });
                    if sensitive {
                        ui.colored_label(
                            egui::Color32::from_rgb(200, 80, 60),
                            "含系统敏感路径，请确认。",
                        );
                    }
                    ui.horizontal(|ui| {
                        if ui.button("取消").clicked() {
                            self.confirm_junk = false;
                            self.confirm_safe_clean = false;
                        }
                        if ui.button("开始清理").clicked() {
                            self.do_junk_clean();
                        }
                    });
                });
        }
        if self.confirm_shortcuts {
            let paths: Vec<PathBuf> = self
                .broken_shortcuts
                .iter()
                .filter(|s| s.selected)
                .map(|s| s.path.clone())
                .collect();
            let n = paths.len();
            let preview = paths_ui::preview_paths(&paths, 12);
            egui::Window::new("确认删除失效快捷方式")
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(format!("将把 {n} 个失效 .lnk 移到回收站。"));
                    ui.group(|ui| {
                        ui.weak("将删除：");
                        ui.monospace(&preview);
                    });
                    ui.horizontal(|ui| {
                        if ui.button("取消").clicked() {
                            self.confirm_shortcuts = false;
                        }
                        if ui.button("移到回收站").clicked() {
                            self.do_shortcuts_clean();
                        }
                    });
                });
        }
        if self.confirm_orphans {
            let n = self.orphans.iter().filter(|o| o.selected).count();
            egui::Window::new("确认删除注册表项")
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.colored_label(
                        egui::Color32::from_rgb(200, 80, 60),
                        format!("将永久删除 {n} 个 Uninstall 注册表子键（不可进回收站）。"),
                    );
                    ui.label("仅限判定为无效的卸载项；HKLM 需要管理员权限。");
                    ui.horizontal(|ui| {
                        if ui.button("取消").clicked() {
                            self.confirm_orphans = false;
                        }
                        if ui.button("删除注册表项").clicked() {
                            self.do_orphans_clean();
                        }
                    });
                });
        }
        if self.confirm_dup_delete {
            let mut paths = Vec::new();
            for g in &self.dup_groups {
                for (i, p) in g.paths.iter().enumerate() {
                    if g.selected.get(i).copied().unwrap_or(false) {
                        paths.push(p.clone());
                    }
                }
            }
            let n = paths.len();
            let preview = paths_ui::preview_paths(&paths, 12);
            egui::Window::new("确认删除重复文件")
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(format!("将删除 {n} 个勾选的重复文件（优先进回收站）。"));
                    ui.group(|ui| {
                        ui.weak("将删除：");
                        ui.monospace(&preview);
                    });
                    ui.checkbox(
                        &mut self.allow_permanent_delete,
                        "回收站失败时允许直接删除",
                    );
                    ui.horizontal(|ui| {
                        if ui.button("取消").clicked() {
                            self.confirm_dup_delete = false;
                        }
                        if ui.button("开始删除").clicked() {
                            self.do_dup_delete();
                        }
                    });
                });
        }
        if self.confirm_leftovers {
            let paths: Vec<PathBuf> = self
                .leftovers
                .iter()
                .filter(|h| h.selected)
                .map(|h| h.path.clone())
                .collect();
            let n = paths.len();
            let preview = paths_ui::preview_paths(&paths, 12);
            egui::Window::new("确认删除卸载残留")
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.colored_label(
                        egui::Color32::from_rgb(200, 80, 60),
                        format!("将删除 {n} 个残留目录（请确认不是仍在用的软件）。"),
                    );
                    ui.group(|ui| {
                        ui.weak("将删除：");
                        ui.monospace(&preview);
                    });
                    ui.checkbox(
                        &mut self.allow_permanent_delete,
                        "回收站失败时允许直接删除",
                    );
                    ui.horizontal(|ui| {
                        if ui.button("取消").clicked() {
                            self.confirm_leftovers = false;
                        }
                        if ui.button("开始删除").clicked() {
                            self.do_leftovers_delete();
                        }
                    });
                });
        }
        if self.confirm_empty_recycle {
            egui::Window::new("确认清空回收站")
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.colored_label(
                        egui::Color32::from_rgb(200, 80, 60),
                        "将永久清空回收站，无法恢复。",
                    );
                    ui.horizontal(|ui| {
                        if ui.button("取消").clicked() {
                            self.confirm_empty_recycle = false;
                        }
                        if ui.button("清空回收站").clicked() {
                            self.do_empty_recycle();
                        }
                    });
                });
        }
        if self.show_recycle_hint {
            egui::Window::new("已移入回收站")
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("已移入回收站。可在回收站还原。");
                    ui.horizontal(|ui| {
                        if ui.button("打开回收站").clicked() {
                            let _ = paths_ui::open_recycle_bin();
                            self.show_recycle_hint = false;
                        }
                        if ui.button("知道了").clicked() {
                            self.show_recycle_hint = false;
                        }
                    });
                });
        }
    }
}
