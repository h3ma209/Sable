use std::collections::{HashMap, HashSet};
use std::os::darwin::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::access::{folder_access, AccessStatus};
use crate::cleanup::finder_trash;

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
    let scan_root = PathBuf::from(normalize_space_path(&scan_root.display().to_string()));
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let task = Arc::new(create_scan_task(&home));
    let root_dev = std::fs::symlink_metadata(&scan_root).map(|meta| meta.st_dev()).unwrap_or(0);
    let seen = Arc::new(Mutex::new(HashSet::new()));
    let scan_for_task = scan_root.clone();
    let task_bg = Arc::clone(&task);
    let seen_bg = Arc::clone(&seen);
    let root = tokio::task::spawn_blocking(move || {
        build_root_entity(scan_for_task, root_dev, &task_bg, seen_bg)
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
) -> SpaceEntity {
    if let Ok(meta) = std::fs::symlink_metadata(&scan_root) {
        if let Ok(mut guard) = seen.lock() {
            guard.insert((meta.st_dev(), meta.st_ino()));
        }
    }
    let entries = directory_entries(task, &scan_root);
    let mut children = build_children(entries, root_dev, task, &seen, true);
    children.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    children.retain(|node| node.bytes > 0 || node.is_directory);
    let bytes = children.iter().fold(0u64, |sum, child| sum.saturating_add(child.bytes));
    let leaves = children.iter().fold(0u64, |sum, child| sum.saturating_add(child.leaves));
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
    parallel: bool,
) -> Vec<SpaceEntity> {
    if entries.is_empty() {
        return Vec::new();
    }
    let workers = if parallel {
        MAX_CONCURRENT_SCANS.min(entries.len())
    } else {
        1
    };
    if workers <= 1 {
        return entries
            .into_iter()
            .filter_map(|path| build_entity(path, root_dev, task, seen))
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
                scope.spawn(move || {
                    chunk
                        .into_iter()
                        .filter_map(|path| build_entity(path, root_dev, &task, &seen))
                        .collect::<Vec<_>>()
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
) -> Option<SpaceEntity> {
    let meta = std::fs::symlink_metadata(&path).ok()?;
    let same_volume = meta.st_dev() == root_dev;
    let is_link = meta.file_type().is_symlink();
    let is_directory = same_volume
        && (meta.is_dir() && !is_link
            || is_link && std::fs::metadata(&path).map(|followed| followed.is_dir()).unwrap_or(false));
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
        let mut children = build_children(entries, root_dev, task, seen, false);
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
    Some(SpaceEntity {
        path,
        bytes: allocated_bytes(&meta),
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
    text == "/.nofollow" || text.starts_with("/.nofollow/")
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
        let sub = build_entity(dir.join("sub"), root_dev, &task, &seen).unwrap();
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
        let _ = std::fs::remove_dir_all(&dir);
    }
}
