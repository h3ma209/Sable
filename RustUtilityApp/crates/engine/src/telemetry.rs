//! SQLite-backed host and process telemetry for System Monitor history.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

use crate::performance::MachineSample;

const DB_NAME: &str = "monitor-history.db";
const RETENTION_SECS: i64 = 30 * 24 * 60 * 60;
const MAX_HOST_ROWS: i64 = 100_000;

#[derive(Clone, Debug, PartialEq)]
pub struct HostPoint {
    pub id: i64,
    pub recorded_at: i64,
    pub cpu_percent: f32,
    pub mem_used: u64,
    pub mem_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub process_count: u32,
    pub load_avg_1m: f32,
    pub uptime_secs: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProcessAggregate {
    pub name: String,
    pub samples: u64,
    pub avg_cpu: f32,
    pub peak_cpu: f32,
    pub peak_mem: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MonitorBucket {
    pub start_at: i64,
    pub avg_cpu: f32,
    pub peak_cpu: f32,
    pub avg_mem_pct: f32,
    pub peak_mem_pct: f32,
    pub samples: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MonitorAnalytics {
    pub samples: u64,
    pub avg_cpu: f32,
    pub peak_cpu: f32,
    pub p50_cpu: f32,
    pub p95_cpu: f32,
    pub avg_mem_pct: f32,
    pub peak_mem_pct: f32,
    pub p50_mem_pct: f32,
    pub p95_mem_pct: f32,
    pub hot_cpu_samples: u64,
    pub hot_mem_samples: u64,
    pub avg_load: f32,
    pub peak_load: f32,
    pub avg_swap_pct: f32,
    pub peak_swap_pct: f32,
    pub avg_process_count: f32,
    pub first_at: Option<i64>,
    pub last_at: Option<i64>,
    pub buckets: Vec<MonitorBucket>,
    pub insights: Vec<String>,
    pub top_cpu: Vec<ProcessAggregate>,
    pub top_mem: Vec<ProcessAggregate>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProcessPoint {
    pub recorded_at: i64,
    pub name: String,
    pub pid: u32,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TelemetrySummary {
    pub sample_count: u64,
    pub process_row_count: u64,
    pub oldest_at: Option<i64>,
    pub newest_at: Option<i64>,
    pub db_bytes: u64,
    pub logging_enabled: bool,
}

pub struct TelemetryStore {
    path: PathBuf,
}

impl TelemetryStore {
    pub fn open(home: &Path) -> Result<Self, String> {
        let dir = home.join("Library/Application Support/Sable");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(DB_NAME);
        let store = Self { path };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<(), String> {
        let conn = Connection::open(&self.path).map_err(|e| e.to_string())?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS host_samples (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                recorded_at INTEGER NOT NULL,
                cpu_percent REAL NOT NULL,
                mem_used INTEGER NOT NULL,
                mem_total INTEGER NOT NULL,
                swap_used INTEGER NOT NULL,
                swap_total INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS process_samples (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                host_sample_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                pid INTEGER NOT NULL,
                cpu_percent REAL NOT NULL,
                memory_bytes INTEGER NOT NULL,
                FOREIGN KEY(host_sample_id) REFERENCES host_samples(id) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_host_recorded ON host_samples(recorded_at);
            CREATE INDEX IF NOT EXISTS idx_process_host ON process_samples(host_sample_id);
            CREATE INDEX IF NOT EXISTS idx_process_name ON process_samples(name);
            "#,
        )
        .map_err(|e| e.to_string())?;
        for sql in [
            "ALTER TABLE host_samples ADD COLUMN process_count INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE host_samples ADD COLUMN load_avg_1m REAL NOT NULL DEFAULT 0",
            "ALTER TABLE host_samples ADD COLUMN uptime_secs INTEGER NOT NULL DEFAULT 0",
        ] {
            let _ = conn.execute(sql, []);
        }
        Ok(())
    }

    pub fn logging_enabled(&self) -> bool {
        self.get_setting("logging_enabled")
            .map(|v| v == "1")
            .unwrap_or(false)
    }

    pub fn set_logging_enabled(&self, on: bool) -> Result<(), String> {
        self.set_setting("logging_enabled", if on { "1" } else { "0" })
    }

    fn get_setting(&self, key: &str) -> Option<String> {
        let conn = Connection::open(&self.path).ok()?;
        conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .ok()
    }

    fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        let conn = Connection::open(&self.path).map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn record(&self, sample: &MachineSample) -> Result<i64, String> {
        let now = unix_now();
        let conn = Connection::open(&self.path).map_err(|e| e.to_string())?;
        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO host_samples (recorded_at, cpu_percent, mem_used, mem_total, swap_used, swap_total, process_count, load_avg_1m, uptime_secs)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                now,
                sample.cpu_percent,
                sample.used_memory,
                sample.total_memory,
                sample.swap_used,
                sample.swap_total,
                sample.process_count as i64,
                sample.load_avg_1m,
                sample.uptime_secs as i64,
            ],
        )
        .map_err(|e| e.to_string())?;
        let host_id = tx.last_insert_rowid();

        let mut seen_pids = std::collections::HashSet::new();
        for proc in sample
            .top_cpu
            .iter()
            .chain(sample.top_mem.iter())
        {
            if !seen_pids.insert(proc.pid) {
                continue;
            }
            tx.execute(
                "INSERT INTO process_samples (host_sample_id, name, pid, cpu_percent, memory_bytes)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![host_id, proc.name, proc.pid, proc.cpu_percent, proc.memory_bytes],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        self.prune_old()?;
        Ok(host_id)
    }

    fn prune_old(&self) -> Result<(), String> {
        let conn = Connection::open(&self.path).map_err(|e| e.to_string())?;
        let cutoff = unix_now() - RETENTION_SECS;
        conn.execute(
            "DELETE FROM process_samples WHERE host_sample_id IN (
                SELECT id FROM host_samples WHERE recorded_at < ?1
            )",
            params![cutoff],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM host_samples WHERE recorded_at < ?1",
            params![cutoff],
        )
        .map_err(|e| e.to_string())?;
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM host_samples", [], |r| r.get(0))
            .unwrap_or(0);
        if count > MAX_HOST_ROWS {
            let trim = count - MAX_HOST_ROWS;
            conn.execute(
                "DELETE FROM process_samples WHERE host_sample_id IN (
                    SELECT id FROM host_samples ORDER BY recorded_at ASC LIMIT ?1
                )",
                params![trim],
            )
            .map_err(|e| e.to_string())?;
            conn.execute(
                "DELETE FROM host_samples WHERE id IN (
                    SELECT id FROM host_samples ORDER BY recorded_at ASC LIMIT ?1
                )",
                params![trim],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn host_history(&self, since_unix: i64, limit: usize) -> Result<Vec<HostPoint>, String> {
        let conn = Connection::open(&self.path).map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, recorded_at, cpu_percent, mem_used, mem_total, swap_used, swap_total,
                        process_count, load_avg_1m, uptime_secs
                 FROM host_samples
                 WHERE recorded_at >= ?1
                 ORDER BY recorded_at ASC
                 LIMIT ?2",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![since_unix, limit as i64], |row| {
                Ok(HostPoint {
                    id: row.get(0)?,
                    recorded_at: row.get(1)?,
                    cpu_percent: row.get(2)?,
                    mem_used: row.get(3)?,
                    mem_total: row.get(4)?,
                    swap_used: row.get(5)?,
                    swap_total: row.get(6)?,
                    process_count: row.get::<_, i64>(7).unwrap_or(0) as u32,
                    load_avg_1m: row.get(8).unwrap_or(0.0),
                    uptime_secs: row.get::<_, i64>(9).unwrap_or(0) as u64,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn processes_at(&self, host_id: i64) -> Result<Vec<ProcessPoint>, String> {
        let conn = Connection::open(&self.path).map_err(|e| e.to_string())?;
        let recorded_at: i64 = conn
            .query_row(
                "SELECT recorded_at FROM host_samples WHERE id = ?1",
                params![host_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT name, pid, cpu_percent, memory_bytes FROM process_samples
                 WHERE host_sample_id = ?1
                 ORDER BY cpu_percent DESC, memory_bytes DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![host_id], |row| {
                Ok(ProcessPoint {
                    recorded_at,
                    name: row.get(0)?,
                    pid: row.get(1)?,
                    cpu_percent: row.get(2)?,
                    memory_bytes: row.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn process_series(
        &self,
        name: &str,
        since_unix: i64,
        limit: usize,
    ) -> Result<Vec<ProcessPoint>, String> {
        let conn = Connection::open(&self.path).map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT h.recorded_at, p.name, p.pid, p.cpu_percent, p.memory_bytes
                 FROM process_samples p
                 JOIN host_samples h ON h.id = p.host_sample_id
                 WHERE p.name = ?1 AND h.recorded_at >= ?2
                 ORDER BY h.recorded_at ASC
                 LIMIT ?3",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![name, since_unix, limit as i64], |row| {
                Ok(ProcessPoint {
                    recorded_at: row.get(0)?,
                    name: row.get(1)?,
                    pid: row.get(2)?,
                    cpu_percent: row.get(3)?,
                    memory_bytes: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn summary(&self) -> Result<TelemetrySummary, String> {
        let conn = Connection::open(&self.path).map_err(|e| e.to_string())?;
        let sample_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM host_samples", [], |r| r.get(0))
            .unwrap_or(0);
        let process_row_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM process_samples", [], |r| r.get(0))
            .unwrap_or(0);
        let oldest_at: Option<i64> = conn
            .query_row("SELECT MIN(recorded_at) FROM host_samples", [], |r| r.get(0))
            .unwrap_or(None);
        let newest_at: Option<i64> = conn
            .query_row("SELECT MAX(recorded_at) FROM host_samples", [], |r| r.get(0))
            .unwrap_or(None);
        let db_bytes = std::fs::metadata(&self.path)
            .map(|m| m.len())
            .unwrap_or(0);
        Ok(TelemetrySummary {
            sample_count: sample_count as u64,
            process_row_count: process_row_count as u64,
            oldest_at,
            newest_at,
            db_bytes,
            logging_enabled: self.logging_enabled(),
        })
    }

    pub fn analytics(&self, since_unix: i64) -> Result<MonitorAnalytics, String> {
        let points = self.host_history(since_unix, 50_000)?;
        if points.is_empty() {
            let mut empty = MonitorAnalytics::default();
            let summary = self.summary().unwrap_or_default();
            if summary.sample_count == 0 {
                empty.insights.push(
                    "No history in the database yet. Turn on history logging and leave Sable open for a few minutes.".into(),
                );
            } else {
                empty.insights.push(format!(
                    "No samples in this time window, but {} older samples exist — try a wider range (24h or 7d).",
                    summary.sample_count
                ));
            }
            return Ok(empty);
        }

        let mut cpu_vals: Vec<f32> = Vec::with_capacity(points.len());
        let mut mem_vals: Vec<f32> = Vec::with_capacity(points.len());
        let mut swap_vals: Vec<f32> = Vec::new();
        let mut load_vals: Vec<f32> = Vec::with_capacity(points.len());
        let mut proc_vals: Vec<f32> = Vec::with_capacity(points.len());
        let mut hot_cpu = 0u64;
        let mut hot_mem = 0u64;
        let mut sum_cpu = 0.0f64;
        let mut sum_mem = 0.0f64;
        let mut sum_load = 0.0f64;
        let mut sum_swap = 0.0f64;
        let mut sum_proc = 0.0f64;
        let mut peak_cpu = 0.0f32;
        let mut peak_mem = 0.0f32;
        let mut peak_load = 0.0f32;
        let mut peak_swap = 0.0f32;

        for p in &points {
            cpu_vals.push(p.cpu_percent);
            sum_cpu += p.cpu_percent as f64;
            peak_cpu = peak_cpu.max(p.cpu_percent);
            if p.cpu_percent >= 80.0 {
                hot_cpu += 1;
            }
            let mem_pct = if p.mem_total > 0 {
                p.mem_used as f32 / p.mem_total as f32 * 100.0
            } else {
                0.0
            };
            mem_vals.push(mem_pct);
            sum_mem += mem_pct as f64;
            peak_mem = peak_mem.max(mem_pct);
            if mem_pct >= 85.0 {
                hot_mem += 1;
            }
            load_vals.push(p.load_avg_1m);
            sum_load += p.load_avg_1m as f64;
            peak_load = peak_load.max(p.load_avg_1m);
            proc_vals.push(p.process_count as f32);
            sum_proc += f64::from(p.process_count);
            let swap_pct = if p.swap_total > 0 {
                p.swap_used as f32 / p.swap_total as f32 * 100.0
            } else {
                0.0
            };
            if p.swap_total > 0 {
                swap_vals.push(swap_pct);
                sum_swap += swap_pct as f64;
                peak_swap = peak_swap.max(swap_pct);
            }
        }

        let n = points.len() as f64;
        let avg_cpu = (sum_cpu / n) as f32;
        let avg_mem_pct = (sum_mem / n) as f32;
        let avg_load = (sum_load / n) as f32;
        let avg_process_count = (sum_proc / n) as f32;
        let avg_swap_pct = if swap_vals.is_empty() {
            0.0
        } else {
            (sum_swap / swap_vals.len() as f64) as f32
        };
        cpu_vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        mem_vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let p50_cpu = percentile(&cpu_vals, 0.5);
        let p95_cpu = percentile(&cpu_vals, 0.95);
        let p50_mem_pct = percentile(&mem_vals, 0.5);
        let p95_mem_pct = percentile(&mem_vals, 0.95);
        let first_at = points.first().map(|p| p.recorded_at);
        let last_at = points.last().map(|p| p.recorded_at);
        let buckets = bucket_hosts(&points, 40);
        let insights = build_insights(
            avg_cpu,
            p50_cpu,
            peak_cpu,
            avg_mem_pct,
            peak_mem,
            hot_cpu,
            hot_mem,
            &points,
            &buckets,
        );

        let conn = Connection::open(&self.path).map_err(|e| e.to_string())?;
        let mut top_cpu = Vec::new();
        let mut stmt = conn
            .prepare(
                "SELECT p.name, COUNT(*) AS n, AVG(p.cpu_percent), MAX(p.cpu_percent), MAX(p.memory_bytes)
                 FROM process_samples p
                 JOIN host_samples h ON h.id = p.host_sample_id
                 WHERE h.recorded_at >= ?1
                 GROUP BY p.name
                 ORDER BY AVG(p.cpu_percent) DESC
                 LIMIT 12",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![since_unix], |row| {
                Ok(ProcessAggregate {
                    name: row.get(0)?,
                    samples: row.get::<_, i64>(1)? as u64,
                    avg_cpu: row.get(2)?,
                    peak_cpu: row.get(3)?,
                    peak_mem: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            top_cpu.push(row.map_err(|e| e.to_string())?);
        }

        let mut top_mem = Vec::new();
        let mut stmt = conn
            .prepare(
                "SELECT p.name, COUNT(*) AS n, AVG(p.cpu_percent), MAX(p.cpu_percent), MAX(p.memory_bytes)
                 FROM process_samples p
                 JOIN host_samples h ON h.id = p.host_sample_id
                 WHERE h.recorded_at >= ?1
                 GROUP BY p.name
                 ORDER BY MAX(p.memory_bytes) DESC
                 LIMIT 12",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![since_unix], |row| {
                Ok(ProcessAggregate {
                    name: row.get(0)?,
                    samples: row.get::<_, i64>(1)? as u64,
                    avg_cpu: row.get(2)?,
                    peak_cpu: row.get(3)?,
                    peak_mem: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            top_mem.push(row.map_err(|e| e.to_string())?);
        }

        Ok(MonitorAnalytics {
            samples: points.len() as u64,
            avg_cpu,
            peak_cpu,
            p50_cpu,
            p95_cpu,
            avg_mem_pct,
            peak_mem_pct: peak_mem,
            p50_mem_pct,
            p95_mem_pct,
            hot_cpu_samples: hot_cpu,
            hot_mem_samples: hot_mem,
            avg_load,
            peak_load,
            avg_swap_pct,
            peak_swap_pct: peak_swap,
            avg_process_count,
            first_at,
            last_at,
            buckets,
            insights,
            top_cpu,
            top_mem,
        })
    }

    pub fn export_host_csv(&self, since_unix: i64) -> Result<String, String> {
        let points = self.host_history(since_unix, 50_000)?;
        let mut out = String::from(
            "recorded_at,cpu_percent,mem_used,mem_total,swap_used,swap_total,process_count,load_avg_1m,uptime_secs\n",
        );
        for p in points {
            out.push_str(&format!(
                "{},{:.2},{},{},{},{},{},{:.2},{}\n",
                p.recorded_at,
                p.cpu_percent,
                p.mem_used,
                p.mem_total,
                p.swap_used,
                p.swap_total,
                p.process_count,
                p.load_avg_1m,
                p.uptime_secs,
            ));
        }
        Ok(out)
    }

    pub fn clear_history(&self) -> Result<(), String> {
        let conn = Connection::open(&self.path).map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM process_samples", [])
            .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM host_samples", [])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn percentile(sorted: &[f32], p: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let idx = ((sorted.len() - 1) as f32 * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn bucket_hosts(points: &[HostPoint], max_buckets: usize) -> Vec<MonitorBucket> {
    if points.is_empty() {
        return Vec::new();
    }
    let start = points[0].recorded_at;
    let end = points.last().map(|p| p.recorded_at).unwrap_or(start);
    let span = (end - start).max(60);
    let bucket_secs = (span / max_buckets as i64).max(60);
    let mut out: Vec<MonitorBucket> = Vec::new();
    let mut i = 0;
    let mut bucket_start = start;
    while i < points.len() && out.len() < max_buckets {
        let bucket_end = bucket_start + bucket_secs;
        let mut cpus = Vec::new();
        let mut mems = Vec::new();
        while i < points.len() && points[i].recorded_at < bucket_end {
            cpus.push(points[i].cpu_percent);
            let mem_pct = if points[i].mem_total > 0 {
                points[i].mem_used as f32 / points[i].mem_total as f32 * 100.0
            } else {
                0.0
            };
            mems.push(mem_pct);
            i += 1;
        }
        if !cpus.is_empty() {
            let avg_cpu = cpus.iter().sum::<f32>() / cpus.len() as f32;
            let peak_cpu = cpus.iter().cloned().fold(0.0f32, f32::max);
            let avg_mem = mems.iter().sum::<f32>() / mems.len() as f32;
            let peak_mem = mems.iter().cloned().fold(0.0f32, f32::max);
            out.push(MonitorBucket {
                start_at: bucket_start,
                avg_cpu,
                peak_cpu,
                avg_mem_pct: avg_mem,
                peak_mem_pct: peak_mem,
                samples: cpus.len() as u64,
            });
        }
        bucket_start = bucket_end;
        if bucket_start > end {
            break;
        }
    }
    out
}

fn build_insights(
    avg_cpu: f32,
    p50_cpu: f32,
    peak_cpu: f32,
    avg_mem: f32,
    peak_mem: f32,
    hot_cpu: u64,
    hot_mem: u64,
    points: &[HostPoint],
    buckets: &[MonitorBucket],
) -> Vec<String> {
    let mut out = Vec::new();
    out.push(format!(
        "Average CPU {:.1}% · median {:.1}% · peak {:.0}% across {} samples.",
        avg_cpu,
        p50_cpu,
        peak_cpu,
        points.len()
    ));
    out.push(format!(
        "Memory averaged {:.0}% with peak {:.0}%.",
        avg_mem, peak_mem
    ));
    if hot_cpu > 0 {
        out.push(format!(
            "{hot_cpu} samples hit high CPU (≥80%) — worth checking top CPU processes below."
        ));
    }
    if hot_mem > 0 {
        out.push(format!(
            "{hot_mem} samples hit high memory (≥85%) — inspect memory leaders."
        ));
    }
    if buckets.len() >= 4 {
        let half = buckets.len() / 2;
        let early: f32 = buckets[..half].iter().map(|b| b.avg_cpu).sum::<f32>() / half as f32;
        let late: f32 = buckets[half..].iter().map(|b| b.avg_cpu).sum::<f32>()
            / (buckets.len() - half).max(1) as f32;
        if late > early + 8.0 {
            out.push(format!(
                "CPU trended up later in the window ({early:.0}% → {late:.0}% average per bucket)."
            ));
        } else if early > late + 8.0 {
            out.push(format!(
                "CPU cooled down later in the window ({early:.0}% → {late:.0}% average per bucket)."
            ));
        }
    }
    if let Some(proc) = points
        .iter()
        .max_by(|a, b| a.load_avg_1m.partial_cmp(&b.load_avg_1m).unwrap_or(std::cmp::Ordering::Equal))
    {
        if proc.load_avg_1m > 2.0 {
            out.push(format!(
                "Highest 1m load average was {:.2} (more queued work than cores).",
                proc.load_avg_1m
            ));
        }
    }
    out
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::performance::{MachineSample, ProcessSnap};

    #[test]
    fn record_and_query_roundtrip() {
        let home = std::env::temp_dir().join(format!("sable_telemetry_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let store = TelemetryStore::open(&home).unwrap();
        store.set_logging_enabled(true).unwrap();
        assert!(store.logging_enabled());
        let sample = MachineSample {
            used_memory: 8_000_000_000,
            total_memory: 16_000_000_000,
            available_memory: 8_000_000_000,
            cpu_percent: 42.5,
            swap_used: 0,
            swap_total: 0,
            process_count: 120,
            uptime_secs: 3_600,
            load_avg_1m: 1.2,
            load_avg_5m: 1.0,
            load_avg_15m: 0.9,
            boot_volume: crate::performance::VolumeBytes {
                total: 500_000_000_000,
                free: 100_000_000_000,
            },
            top_cpu: vec![ProcessSnap {
                name: "Finder".into(),
                pid: 101,
                cpu_percent: 12.0,
                memory_bytes: 50_000_000,
            }],
            top_mem: vec![],
        };
        let id = store.record(&sample).unwrap();
        let hist = store.host_history(0, 10).unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].id, id);
        let procs = store.processes_at(id).unwrap();
        assert_eq!(procs.len(), 1);
        assert_eq!(procs[0].name, "Finder");
        let a = store.analytics(0).unwrap();
        assert_eq!(a.samples, 1);
        assert!(a.peak_cpu >= 40.0);
        let _ = std::fs::remove_dir_all(&home);
    }
}
