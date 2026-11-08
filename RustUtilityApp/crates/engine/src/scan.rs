use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinSet;

use crate::control::ExecutionControl;
use crate::detect::{file_meta, Detector};
use crate::enumerate::{self, WalkOptions};
use crate::error::ScanError;
use crate::event::ScanEvent;

#[derive(Clone, Debug)]
pub struct ScanConfig {
    pub root: PathBuf,
    pub chunk_size: usize,
    pub follow_symlinks: bool,
    pub check_concurrency: usize,
    pub exclude_dirs: Vec<String>,
}

impl ScanConfig {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            chunk_size: 64,
            follow_symlinks: false,
            check_concurrency: 8,
            exclude_dirs: Vec::new(),
        }
    }
}

/// Runs the walk, then checks each chunk. Does not delete anything.
pub async fn run_scan(
    config: ScanConfig,
    detector: Arc<dyn Detector>,
    control: Arc<ExecutionControl>,
    events: mpsc::Sender<ScanEvent>,
) {
    let mut scanned = 0u64;
    let mut detections = 0u64;
    let mut errors = 0u64;

    let (chunk_tx, mut chunk_rx) = mpsc::channel(4);
    let walk_control = Arc::clone(&control);
    let exclude_dirs = config.exclude_dirs.clone();
    let walk_opts = WalkOptions {
        follow_symlinks: config.follow_symlinks,
        skip_dir: Arc::new(move |path: &std::path::Path| {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                let name_lower = name.to_ascii_lowercase();
                exclude_dirs.iter().any(|ex| {
                    let ex_clean = ex.trim().trim_matches('/').to_ascii_lowercase();
                    !ex_clean.is_empty() && (name_lower == ex_clean || name_lower.contains(&ex_clean))
                })
            } else {
                false
            }
        }),
        ..WalkOptions::new(config.root.clone(), config.chunk_size)
    };
    let walk = tokio::spawn(async move { enumerate::walk(walk_opts, &walk_control, chunk_tx).await });

    let limit = config.check_concurrency.max(1);
    let semaphore = Arc::new(Semaphore::new(limit));

    loop {
        let Some(chunk) = chunk_rx.recv().await else {
            break;
        };
        if chunk.paths.is_empty() {
            if chunk.finale {
                break;
            }
            continue;
        }

        let mut joins = JoinSet::new();
        for path in chunk.paths {
            if let Err(err) = control.checkpoint().await {
                let _ = events
                    .send(ScanEvent::Error {
                        path: config.root.clone(),
                        message: err.to_string(),
                    })
                    .await;
                joins.abort_all();
                let _ = walk.await;
                let _ = events
                    .send(ScanEvent::Finished {
                        scanned,
                        detections,
                        errors,
                    })
                    .await;
                return;
            }
            let permit = match semaphore.clone().acquire_owned().await {
                Ok(permit) => permit,
                Err(_) => break,
            };
            let detector = Arc::clone(&detector);
            let control = Arc::clone(&control);
            let events = events.clone();
            joins.spawn(async move {
                let _permit = permit;
                check_one(path, detector, control, events).await
            });
        }

        while let Some(joined) = joins.join_next().await {
            match joined {
                Ok(Outcome {
                    scanned: s,
                    detections: d,
                    errors: e,
                    canceled,
                }) => {
                    scanned += s;
                    detections += d;
                    errors += e;
                    if canceled {
                        joins.abort_all();
                        let _ = walk.await;
                        let _ = events
                            .send(ScanEvent::Finished {
                                scanned,
                                detections,
                                errors,
                            })
                            .await;
                        return;
                    }
                }
                Err(err) => {
                    errors += 1;
                    let _ = events
                        .send(ScanEvent::Error {
                            path: config.root.clone(),
                            message: err.to_string(),
                        })
                        .await;
                }
            }
        }
        if chunk.finale {
            break;
        }
    }

    if let Ok(Err(err)) = walk.await {
        errors += 1;
        let _ = events
            .send(ScanEvent::Error {
                path: config.root,
                message: err.to_string(),
            })
            .await;
    }

    let _ = events
        .send(ScanEvent::Finished {
            scanned,
            detections,
            errors,
        })
        .await;
}

struct Outcome {
    scanned: u64,
    detections: u64,
    errors: u64,
    canceled: bool,
}

async fn check_one(
    path: PathBuf,
    detector: Arc<dyn Detector>,
    control: Arc<ExecutionControl>,
    events: mpsc::Sender<ScanEvent>,
) -> Outcome {
    if let Err(err) = control.checkpoint().await {
        let _ = events
            .send(ScanEvent::Error {
                path: path.clone(),
                message: err.to_string(),
            })
            .await;
        return Outcome {
            scanned: 0,
            detections: 0,
            errors: 0,
            canceled: true,
        };
    }
    let _ = events
        .send(ScanEvent::Progress {
            path: path.clone(),
        })
        .await;
    match detector.check(&path).await {
        Ok(Some(hit)) => {
            let _ = events.send(ScanEvent::Detection(hit)).await;
            Outcome {
                scanned: 1,
                detections: 1,
                errors: 0,
                canceled: false,
            }
        }
        Ok(None) => {
            let (size, extension) = file_meta(&path).await.unwrap_or((0, String::new()));
            let _ = events
                .send(ScanEvent::Checked {
                    path,
                    size,
                    extension,
                })
                .await;
            Outcome {
                scanned: 1,
                detections: 0,
                errors: 0,
                canceled: false,
            }
        }
        Err(ScanError::Canceled) => Outcome {
            scanned: 0,
            detections: 0,
            errors: 0,
            canceled: true,
        },
        Err(err) => {
            let _ = events
                .send(ScanEvent::Error {
                    path: path.clone(),
                    message: err.to_string(),
                })
                .await;
            Outcome {
                scanned: 1,
                detections: 0,
                errors: 1,
                canceled: false,
            }
        }
    }
}
