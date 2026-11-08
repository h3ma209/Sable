use std::collections::{HashMap, HashSet};
use std::os::darwin::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::access::{folder_access, AccessStatus};
use crate::cleanup::finder_trash;
use crate::control::ExecutionControl;

/// `SF_DATALESS`. The scan tests this bit of `st_flags` after `lstat`.
const SF_DATALESS: u32 = 0x40000000;

/// Concurrent directory subtrees.
const MAX_CONCURRENT_SCANS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloudStatus {
    NotCloud,
    CloudLocal,
    CloudEvicted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloudScannerStrategy {
    UrlUbiquitousKeys,
    DatalessFlag,
}

#[derive(Clone, Debug)]
struct PathFilter {
    custom_ignore_paths: HashSet<PathBuf>,
    reminders_path: Option<PathBuf>,
}

struct ScanTask {
    filter: PathFilter,
    strategy: CloudScannerStrategy,
}

/// Full tree node. Sable keeps one `rootNode` on the representation view model
/// and drills down via `navigationHistory` without rescanning each folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpaceEntity {
    pub path: PathBuf,
    pub bytes: u64,
    pub is_directory: bool,
    pub is_purgeable: bool,
    pub cloud: CloudStatus,
    pub leaves: u64,
    pub modified: Option<u64>,
    pub children: Vec<SpaceEntity>,
}

/// One volume scan. Navigation reuses `root` and only remaps immediate children.
#[derive(Clone, Debug)]
pub struct SpaceVolumeScan {
    pub scan_root: PathBuf,
    root: SpaceEntity,
    /// Child-index path from `root`. Empty vec is the scan root. No tree walk on open.
    lookup: HashMap<PathBuf, Vec<usize>>,
}

/// One level in the UI. Sizes include descendants; no nested `children` in the list row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpaceNode {
    pub path: PathBuf,
    pub bytes: u64,
    pub is_directory: bool,
    pub is_purgeable: bool,
    pub cloud: CloudStatus,
    pub leaves: u64,
    pub modified: Option<u64>,
}

/// Real-time scan progress emitted during volume mapping.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpaceProgress {
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub items_done: u64,
    pub current_path: String,
}

pub fn volume_used_bytes(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::mem::MaybeUninit;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat: MaybeUninit<libc::statvfs> = MaybeUninit::uninit();
    let res = unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if res == 0 {
        let stat = unsafe { stat.assume_init() };
        let total = (stat.f_blocks as u64).saturating_mul(stat.f_frsize as u64);
        let free = (stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64);
        Some(total.saturating_sub(free))
    } else {
        None
    }
}

struct ProgressTracker {
    control: Option<Arc<ExecutionControl>>,
    tx: Option<tokio::sync::mpsc::UnboundedSender<SpaceProgress>>,
    bytes_done: AtomicU64,
    items_done: AtomicU64,
    bytes_total: AtomicU64,
    last_emit: Mutex<std::time::Instant>,
}

impl ProgressTracker {
    fn new(
        control: Option<Arc<ExecutionControl>>,
        tx: Option<tokio::sync::mpsc::UnboundedSender<SpaceProgress>>,
        initial_total: u64,
    ) -> Self {
        Self {
            control,
            tx,
            bytes_done: AtomicU64::new(0),
            items_done: AtomicU64::new(0),
            bytes_total: AtomicU64::new(initial_total),
            last_emit: Mutex::new(std::time::Instant::now()),
        }
    }

    fn is_canceled(&self) -> bool {
        self.control.as_ref().map(|c| c.is_canceled()).unwrap_or(false)
    }

    fn record_item(&self, bytes: u64, path: &Path) {
        let done = self.bytes_done.fetch_add(bytes, Ordering::Relaxed) + bytes;
        let items = self.items_done.fetch_add(1, Ordering::Relaxed) + 1;

        if let Some(ref tx) = self.tx {
            let mut should_emit = false;
            if let Ok(mut last) = self.last_emit.try_lock() {
                if last.elapsed() >= std::time::Duration::from_millis(50) {
                    *last = std::time::Instant::now();
                    should_emit = true;
                }
            }
            if should_emit {
                let mut total = self.bytes_total.load(Ordering::Relaxed);
                if done >= total {
                    total = done.saturating_mul(105) / 100;
                    self.bytes_total.store(total, Ordering::Relaxed);
                }
                let _ = tx.send(SpaceProgress {
                    bytes_done: done,
                    bytes_total: total,
                    items_done: items,
                    current_path: path.display().to_string(),
                });
            }
        }
    }

    fn finish(&self, final_bytes: u64) {
        if let Some(ref tx) = self.tx {
            let items = self.items_done.load(Ordering::Relaxed);
            let _ = tx.send(SpaceProgress {
                bytes_done: final_bytes,
                bytes_total: final_bytes,
                items_done: items,
                current_path: "Complete".into(),
            });
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpaceReport {
    pub root: PathBuf,
    pub total: u64,
    pub children: Vec<SpaceNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeletionUnit {
    pub path: PathBuf,
    pub size: u64,
    pub removable_leaves_count: u64,
}

pub const SPACE_LENS_BUBBLE_THRESHOLD: f32 = 0.02;

pub fn computer_root() -> PathBuf {
    let data = PathBuf::from("/System/Volumes/Data");
    if data.is_dir() {
        data
    } else {
        PathBuf::from("/")
    }
}

pub fn normalize_space_path(path: &str) -> String {
    let trimmed = path.trim();
    let candidate = PathBuf::from(trimmed);
    std::fs::canonicalize(&candidate)
        .map(|resolved| resolved.display().to_string())
        .unwrap_or_else(|_| trimmed.to_string())
}

/// Builds the full entity tree under `scan_root` once (up to four subtrees at a time).
pub async fn scan_space_volume(scan_root: PathBuf) -> SpaceVolumeScan {
    scan_space_volume_with_progress(scan_root, None, None).await
}

pub async fn scan_space_volume_with_progress(
    scan_root: PathBuf,
    control: Option<Arc<ExecutionControl>>,
    progress_tx: Option<tokio::sync::mpsc::UnboundedSender<SpaceProgress>>,
) -> SpaceVolumeScan {
    let scan_root = PathBuf::from(normalize_space_path(&scan_root.display().to_string()));
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let task = Arc::new(create_scan_task(&home));
    let root_dev = std::fs::symlink_metadata(&scan_root).map(|meta| meta.st_dev()).unwrap_or(0);
    let seen = Arc::new(Mutex::new(HashSet::new()));
    let scan_for_task = scan_root.clone();
    let task_bg = Arc::clone(&task);
    let seen_bg = Arc::clone(&seen);
    let home_clone = home.clone();
    let root = tokio::task::spawn_blocking(move || {
        let vol_used = volume_used_bytes(&scan_for_task).unwrap_or(250 * 1024 * 1024 * 1024);
        let initial_total = if scan_for_task == Path::new("/") || scan_for_task == computer_root() {
            vol_used
        } else if scan_for_task.starts_with(&home_clone) {
            (vol_used * 60) / 100
        } else {
            vol_used / 3
        };
        let tracker = Arc::new(ProgressTracker::new(control, progress_tx, initial_total));
        build_root_entity(scan_for_task, root_dev, &task_bg, seen_bg, &tracker)
    })
    .await
    .unwrap_or_else(|_| SpaceEntity {
        path: scan_root.clone(),
        bytes: 0,
        is_directory: true,
        is_purgeable: false,
        cloud: CloudStatus::NotCloud,
        leaves: 0,
        modified: None,
        children: Vec::new(),
    });
    new_volume(scan_root, root)
}

fn new_volume(scan_root: PathBuf, root: SpaceEntity) -> SpaceVolumeScan {
    let lookup = build_lookup(&root);
    SpaceVolumeScan {
        scan_root,
        root,
        lookup,
    }
}

/// One folder already in the scanned tree. Children stay in the tree; the UI borrows them.
pub struct SpaceFolder<'a> {
    pub path: &'a Path,
    pub total: u64,
    pub children: &'a [SpaceEntity],
}

pub fn space_folder<'a>(volume: &'a SpaceVolumeScan, folder: &Path) -> Option<SpaceFolder<'a>> {
    let node = find_entity_in_volume(volume, folder)?;
    Some(SpaceFolder {
        path: &node.path,
        total: node.bytes,
        children: &node.children,
    })
}
pub fn space_report_at(volume: &SpaceVolumeScan, folder: &Path) -> Option<SpaceReport> {
    let node = find_entity_in_volume(volume, folder)?;
    let root = node.path.clone();
    let mut children: Vec<SpaceNode> = node.children.iter().map(entity_to_node).collect();
    children.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    children.retain(|entry| entry.bytes > 0 || entry.is_directory);
    let total = sum_bytes(&children);
    Some(SpaceReport {
        root,
        total,
        children,
    })
}

pub fn volume_contains(volume: &SpaceVolumeScan, folder: &Path) -> bool {
    find_entity_in_volume(volume, folder).is_some()
}

/// Desktop, Downloads, and Documents are TCC-protected even when FDA is off.
pub fn home_folder_access(home: &Path) -> AccessStatus {
    let mut status = AccessStatus::Granted;
    for name in ["Desktop", "Downloads", "Documents"] {
        match folder_access(&home.join(name)) {
            AccessStatus::Denied => return AccessStatus::Denied,
            AccessStatus::Unknown => status = AccessStatus::Unknown,
            AccessStatus::Granted => {}
        }
    }
    status
}

pub async fn scan_space(root: PathBuf) -> SpaceReport {
    let volume = scan_space_volume(root).await;
    let at = volume.scan_root.clone();
    space_report_at(&volume, &at).unwrap_or(SpaceReport {
        root: at,
        total: 0,
        children: Vec::new(),
    })
}

fn create_scan_task(home: &Path) -> ScanTask {
    let reminders = home.join("Library/Reminders");
    let fda_granted = folder_access(&reminders) == AccessStatus::Granted
        || folder_access(&home.join("Library/Mail")) == AccessStatus::Granted;
    ScanTask {
        filter: PathFilter {
            custom_ignore_paths: HashSet::new(),
            reminders_path: if fda_granted { None } else { Some(reminders) },
        },
        strategy: CloudScannerStrategy::UrlUbiquitousKeys,
    }
}

fn build_root_entity(
    scan_root: PathBuf,
    root_dev: u64,
    task: &Arc<ScanTask>,
    seen: Arc<Mutex<HashSet<(u64, u64)>>>,
    tracker: &Arc<ProgressTracker>,
) -> SpaceEntity {
    if let Ok(meta) = std::fs::symlink_metadata(&scan_root) {
        if let Ok(mut guard) = seen.lock() {
            guard.insert((meta.st_dev(), meta.st_ino()));
        }
    }
    tracker.record_item(0, &scan_root);
    let entries = directory_entries(task, &scan_root);
    let mut children = build_children(entries, root_dev, task, &seen, tracker, true);
    children.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    children.retain(|node| node.bytes > 0 || node.is_directory);
    let bytes = children.iter().fold(0u64, |sum, child| sum.saturating_add(child.bytes));
    let leaves = children.iter().fold(0u64, |sum, child| sum.saturating_add(child.leaves));
    tracker.finish(bytes);
    SpaceEntity {
        path: scan_root.clone(),
        bytes,
        is_directory: true,
        is_purgeable: false,
        cloud: CloudStatus::NotCloud,
        leaves,
        modified: std::fs::symlink_metadata(&scan_root)
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs()),
        children,
    }
}

fn directory_entries(task: &ScanTask, directory: &Path) -> Vec<PathBuf> {
    match std::fs::read_dir(directory) {
        Ok(entries) => entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| !ignored(&task.filter, path))
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Parallel only for the scan root's immediate children. Deeper folders stay on that same thread.
fn build_children(
    entries: Vec<PathBuf>,
    root_dev: u64,
    task: &Arc<ScanTask>,
    seen: &Arc<Mutex<HashSet<(u64, u64)>>>,
    tracker: &Arc<ProgressTracker>,
    parallel: bool,
) -> Vec<SpaceEntity> {
    if entries.is_empty() || tracker.is_canceled() {
        return Vec::new();
    }
    let total_top_entries = entries.len();
    let completed_top_count = Arc::new(AtomicU64::new(0));

    let workers = if parallel {
        MAX_CONCURRENT_SCANS.min(entries.len())
    } else {
        1
    };
    if workers <= 1 {
        return entries
            .into_iter()
            .take_while(|_| !tracker.is_canceled())
            .filter_map(|path| {
                if tracker.is_canceled() {
                    return None;
                }
                let ent = build_entity(path, root_dev, task, seen, tracker);
                if parallel && total_top_entries > 0 {
                    let done_top = completed_top_count.fetch_add(1, Ordering::Relaxed) + 1;
                    let current_done = tracker.bytes_done.load(Ordering::Relaxed);
                    let est = (current_done as f64 / (done_top as f64 / total_top_entries as f64)) as u64;
                    tracker.bytes_total.store(est.max(current_done + 1), Ordering::Relaxed);
                }
                ent
            })
            .collect();
    }
    let chunk_size = entries.len().div_ceil(workers);
    let chunks: Vec<Vec<PathBuf>> = entries
        .chunks(chunk_size)
        .map(|chunk| chunk.to_vec())
        .collect();
    std::thread::scope(|scope| {
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|chunk| {
                let task = Arc::clone(task);
                let seen = Arc::clone(seen);
                let tracker = Arc::clone(tracker);
                let completed_top = Arc::clone(&completed_top_count);
                scope.spawn(move || {
                    let mut list = Vec::new();
                    for path in chunk {
                        if tracker.is_canceled() {
                            break;
                        }
                        if let Some(ent) = build_entity(path, root_dev, &task, &seen, &tracker) {
                            list.push(ent);
                        }
                        if parallel && total_top_entries > 0 {
                            let done_top = completed_top.fetch_add(1, Ordering::Relaxed) + 1;
                            let current_done = tracker.bytes_done.load(Ordering::Relaxed);
                            let est = (current_done as f64 / (done_top as f64 / total_top_entries as f64)) as u64;
                            tracker.bytes_total.store(est.max(current_done + 1), Ordering::Relaxed);
                        }
                    }
                    list
                })
            })
            .collect();
        handles
            .into_iter()
            .filter_map(|handle| handle.join().ok())
            .flatten()
            .collect()
    })
}

fn build_entity(
    path: PathBuf,
    root_dev: u64,
    task: &Arc<ScanTask>,
    seen: &Arc<Mutex<HashSet<(u64, u64)>>>,
    tracker: &Arc<ProgressTracker>,
) -> Option<SpaceEntity> {
    if tracker.is_canceled() {
        return None;
    }
    let meta = std::fs::symlink_metadata(&path).ok()?;
    let same_volume = meta.st_dev() == root_dev;
    let is_link = meta.file_type().is_symlink();
    // Do not follow symlinks as directories: avoids loops and duplicated stats!
    let is_directory = same_volume && meta.is_dir() && !is_link;
    let cloud = cloud_status(
        task.strategy,
        path_is_ubiquitous(&path),
        meta.st_flags() & SF_DATALESS != 0,
        meta.st_flags(),
    );
    let modified = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());

    if !same_volume {
        tracker.record_item(0, &path);
        return Some(SpaceEntity {
            path,
            bytes: 0,
            is_directory,
            is_purgeable: false,
            cloud,
            leaves: 0,
            modified,
            children: Vec::new(),
        });
    }

    if is_directory {
        tracker.record_item(0, &path);
        let id = (meta.st_dev(), meta.st_ino());
        {
            let mut guard = seen.lock().ok()?;
            if !guard.insert(id) {
                return Some(SpaceEntity {
                    path,
                    bytes: 0,
                    is_directory: true,
                    is_purgeable: false,
                    cloud,
                    leaves: 0,
                    modified,
                    children: Vec::new(),
                });
            }
        }
        let entries = directory_entries(task, &path);
        let mut children = build_children(entries, root_dev, task, seen, tracker, false);
        children.sort_by(|a, b| b.bytes.cmp(&a.bytes));
        let bytes = children.iter().fold(0u64, |sum, child| sum.saturating_add(child.bytes));
        let leaves = children.iter().fold(0u64, |sum, child| sum.saturating_add(child.leaves));
        return Some(SpaceEntity {
            path,
            bytes,
            is_directory: true,
            is_purgeable: false,
            cloud,
            leaves,
            modified,
            children,
        });
    }

    let id = (meta.st_dev(), meta.st_ino());
    if !seen.lock().ok()?.insert(id) {
        return Some(SpaceEntity {
            path,
            bytes: 0,
            is_directory: false,
            is_purgeable: false,
            cloud,
            leaves: 0,
            modified,
            children: Vec::new(),
        });
    }
    let (leaves, _) = leaf_count(false, cloud);
    let bytes = allocated_bytes(&meta);
    tracker.record_item(bytes, &path);
    Some(SpaceEntity {
        path,
        bytes,
        is_directory: false,
        is_purgeable: false,
        cloud,
        leaves,
        modified,
        children: Vec::new(),
    })
}

fn entity_to_node(entity: &SpaceEntity) -> SpaceNode {
    SpaceNode {
        path: entity.path.clone(),
        bytes: entity.bytes,
        is_directory: entity.is_directory,
        is_purgeable: entity.is_purgeable,
        cloud: entity.cloud,
        leaves: entity.leaves,
        modified: entity.modified,
    }
}

fn push_alias(aliases: &mut Vec<PathBuf>, path: PathBuf) {
    if !aliases.iter().any(|existing| existing == &path) {
        aliases.push(path);
    }
}

fn path_aliases(path: &Path) -> Vec<PathBuf> {
    let mut aliases = vec![path.to_path_buf()];
    append_home_data_aliases(&mut aliases);
    aliases
}

fn storage_aliases(path: &Path) -> Vec<PathBuf> {
    let mut aliases = vec![path.to_path_buf()];
    append_home_data_aliases(&mut aliases);
    aliases
}

fn append_home_data_aliases(aliases: &mut Vec<PathBuf>) {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    if home.as_os_str().is_empty() {
        return;
    }
    let data = computer_root();
    let user = home.file_name().map(|name| PathBuf::from(name)).unwrap_or_default();
    let data_home = data.join("Users").join(&user);
    let bases = aliases.clone();
    for base in bases {
        if let Ok(rest) = base.strip_prefix(&home) {
            push_alias(aliases, data_home.join(rest));
        }
        if let Ok(rest) = base.strip_prefix(&data_home) {
            push_alias(aliases, home.join(rest));
        }
    }
}

fn build_lookup(root: &SpaceEntity) -> HashMap<PathBuf, Vec<usize>> {
    let mut lookup = HashMap::new();
    fn walk(node: &SpaceEntity, index: &[usize], lookup: &mut HashMap<PathBuf, Vec<usize>>) {
        if node.is_directory {
            for alias in storage_aliases(&node.path) {
                lookup.entry(alias).or_insert_with(|| index.to_vec());
            }
        }
        for (offset, child) in node.children.iter().enumerate() {
            let mut child_index = Vec::with_capacity(index.len() + 1);
            child_index.extend_from_slice(index);
            child_index.push(offset);
            walk(child, &child_index, lookup);
        }
    }
    walk(root, &[], &mut lookup);
    lookup
}

fn node_at<'a>(root: &'a SpaceEntity, index: &[usize]) -> Option<&'a SpaceEntity> {
    let mut node = root;
    for offset in index {
        node = node.children.get(*offset)?;
    }
    Some(node)
}

fn find_entity_in_volume<'a>(volume: &'a SpaceVolumeScan, folder: &Path) -> Option<&'a SpaceEntity> {
    for alias in path_aliases(folder) {
        if let Some(index) = volume.lookup.get(&alias) {
            if let Some(node) = node_at(&volume.root, index) {
                return Some(node);
            }
        }
    }
    None
}

fn leaf_count(is_purgeable: bool, cloud: CloudStatus) -> (u64, u64) {
    if is_purgeable {
        return (0, 0);
    }
    let removable = u64::from(cloud != CloudStatus::CloudEvicted);
    (1, removable)
}

fn sum_bytes(children: &[SpaceNode]) -> u64 {
    children.iter().fold(0u64, |sum, node| sum.saturating_add(node.bytes))
}

pub fn cloud_status(
    strategy: CloudScannerStrategy,
    is_ubiquitous: bool,
    not_downloaded: bool,
    st_flags: u32,
) -> CloudStatus {
    if strategy == CloudScannerStrategy::DatalessFlag {
        return if st_flags & SF_DATALESS != 0 {
            CloudStatus::CloudEvicted
        } else {
            CloudStatus::CloudLocal
        };
    }
    if !is_ubiquitous {
        CloudStatus::NotCloud
    } else if not_downloaded || st_flags & SF_DATALESS != 0 {
        CloudStatus::CloudEvicted
    } else {
        CloudStatus::CloudLocal
    }
}

fn path_is_ubiquitous(path: &Path) -> bool {
    path.components().any(|component| component.as_os_str() == "Mobile Documents")
}

fn ignored(filter: &PathFilter, path: &Path) -> bool {
    if filter.custom_ignore_paths.contains(path) {
        return true;
    }
    if filter
        .reminders_path
        .as_ref()
        .is_some_and(|reminders| path.starts_with(reminders))
    {
        return true;
    }
    ignored_nofollow(path)
}

fn ignored_nofollow(path: &Path) -> bool {
    let text = path.to_string_lossy();
    if text == "/.nofollow" || text.starts_with("/.nofollow/") {
        return true;
    }
    // Never traverse into /Volumes mount loop (mounts back to Macintosh HD / other disks)
    if text == "/Volumes" || text.starts_with("/Volumes/") {
        return true;
    }
    // Never traverse into /System/Volumes (contains firmlink duplicate Data volume and recursive Volumes loop)
    if text == "/System/Volumes" || text.starts_with("/System/Volumes/") {
        return true;
    }
    // Pseudo filesystems and automounters
    if text == "/dev" || text.starts_with("/dev/")
        || text == "/net" || text.starts_with("/net/")
        || text == "/home" || text.starts_with("/home/")
    {
        return true;
    }
    // Internal volume mounts or firmlink loops anywhere in path
    if text.contains("/Volumes/Macintosh HD") || text.contains("/Data/Volumes") {
        return true;
    }
    // System index and metadata caches
    if text.starts_with("/.Spotlight-V100")
        || text.starts_with("/.DocumentRevisions-V100")
        || text.starts_with("/.fseventsd")
        || text.starts_with("/.PKInstallSandboxManager")
        || text.starts_with("/.vol")
        || text.starts_with("/.MobileBackups")
        || text.starts_with("/.timemachine")
    {
        return true;
    }
    false
}

fn allocated_bytes(meta: &std::fs::Metadata) -> u64 {
    meta.st_blocks().saturating_mul(512)
}

pub fn deletion_units(nodes: &[SpaceNode], should_spare: impl Fn(&SpaceNode) -> bool) -> Vec<DeletionUnit> {
    nodes
        .iter()
        .filter(|node| !should_spare(node) && !node.is_directory)
        .map(|node| DeletionUnit {
            path: node.path.clone(),
            size: node.bytes,
            removable_leaves_count: u64::from(node.cloud != CloudStatus::CloudEvicted && !node.is_purgeable),
        })
        .collect()
}

pub fn is_spared(path: &Path, home: &Path) -> bool {
    if path.is_dir() {
        return true;
    }
    let text = path.to_string_lossy();
    text.starts_with("/System")
        || text.starts_with("/usr")
        || text.starts_with("/bin")
        || text.starts_with("/sbin")
        || text.starts_with("/Library")
        || path.starts_with(home.join("Library"))
}

pub fn trash_selected(root: &Path, home: &Path, paths: &[PathBuf]) -> Result<usize, String> {
    let mut moved = 0usize;
    for path in paths {
        if is_spared(path, home) || !path.starts_with(root) || path == root || !path.is_file() {
            continue;
        }
        finder_trash(path)?;
        moved += 1;
    }
    if moved == 0 {
        Err("nothing removable was selected".into())
    } else {
        Ok(moved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_alias_finds_data_volume_path() {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        if home.as_os_str().is_empty() {
            return;
        }
        let user = home.file_name().unwrap();
        let data_home = computer_root().join("Users").join(user);
        if !data_home.is_dir() {
            return;
        }
        let entity = SpaceEntity {
            path: data_home.join("Desktop"),
            bytes: 42,
            is_directory: true,
            is_purgeable: false,
            cloud: CloudStatus::NotCloud,
            leaves: 1,
            modified: None,
            children: vec![],
        };
        let root = SpaceEntity {
            path: data_home.clone(),
            bytes: 42,
            is_directory: true,
            is_purgeable: false,
            cloud: CloudStatus::NotCloud,
            leaves: 1,
            modified: None,
            children: vec![entity],
        };
        let desktop = data_home.join("Desktop");
        let volume = new_volume(data_home, root);
        let report = space_report_at(&volume, &home.join("Desktop")).expect("alias lookup");
        assert_eq!(report.root, desktop);
    }

    #[test]
    fn cached_report_matches_drill_down() {
        let dir = std::env::temp_dir().join(format!("rust_utility_space_cache_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/b.txt"), vec![b'x'; 4096]).unwrap();

        let root_dev = std::fs::symlink_metadata(&dir).unwrap().st_dev();
        let task = Arc::new(create_scan_task(Path::new("/tmp")));
        let seen = Arc::new(Mutex::new(HashSet::new()));
        let tracker = Arc::new(ProgressTracker::new(None, None, 4096));
        let sub = build_entity(dir.join("sub"), root_dev, &task, &seen, &tracker).unwrap();
        let root = SpaceEntity {
            path: dir.clone(),
            bytes: sub.bytes,
            is_directory: true,
            is_purgeable: false,
            cloud: CloudStatus::NotCloud,
            leaves: sub.leaves,
            modified: None,
            children: vec![sub],
        };
        let volume = new_volume(dir.clone(), root);
        let top = space_report_at(&volume, &dir).unwrap();
        assert_eq!(top.children.len(), 1);
        let nested = space_report_at(&volume, &dir.join("sub")).unwrap();
        assert_eq!(nested.children.len(), 1);
        assert!(nested.children[0].bytes >= 4096);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn scan_sums_a_nested_file_and_skips_root_nofollow() {
        let dir = std::env::temp_dir().join(format!("rust_utility_space_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/b.txt"), vec![b'x'; 8192]).unwrap();
        let report = scan_space(dir.clone()).await;
        let sub = report.children.iter().find(|node| node.path.ends_with("sub")).unwrap();
        assert!(sub.is_directory);
        assert!(sub.bytes >= 8192);
        assert_eq!(sub.leaves, 1);
        assert_eq!(report.total, sub.bytes);
        assert!(ignored_nofollow(Path::new("/.nofollow/item")));
        assert!(ignored_nofollow(Path::new("/Volumes")));
        assert!(ignored_nofollow(Path::new("/System/Volumes")));
        assert!(ignored_nofollow(Path::new("/System/Volumes/Data/Volumes/Macintosh HD")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn symlink_directory_not_followed_as_directory() {
        let dir = std::env::temp_dir().join(format!("rust_utility_space_sym_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("file.txt"), b"hello").unwrap();
        // Symlink pointing back to dir
        #[cfg(unix)]
        let _ = std::os::unix::fs::symlink(&sub, dir.join("link_to_sub"));

        let root_dev = std::fs::symlink_metadata(&dir).unwrap().st_dev();
        let task = Arc::new(create_scan_task(Path::new("/tmp")));
        let seen = Arc::new(Mutex::new(HashSet::new()));
        let tracker = Arc::new(ProgressTracker::new(None, None, 4096));
        let entity = build_entity(dir.join("link_to_sub"), root_dev, &task, &seen, &tracker);
        if let Some(e) = entity {
            assert!(!e.is_directory, "Symlink to directory must not be marked as is_directory");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn scan_aborts_on_execution_control_cancellation() {
        let dir = std::env::temp_dir().join(format!("rust_utility_space_cancel_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..10 {
            std::fs::write(dir.join(format!("file_{i}.txt")), vec![b'a'; 1024]).unwrap();
        }

        let control = Arc::new(crate::ExecutionControl::new());
        control.cancel(); // Pre-cancel

        let volume = scan_space_volume_with_progress(dir.clone(), Some(control), None).await;
        // Even if some or no items got scanned, volume is returned safely without hanging
        let canonical = std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
        assert_eq!(volume.root.path, canonical);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
