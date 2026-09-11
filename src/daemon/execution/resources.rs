use std::io;
use std::path::Path;
use std::time::Instant;

use crate::domain::runtime::WorkspaceResourceEvents;

#[derive(Debug, Clone, Copy)]
pub(super) struct CpuCounters {
    process_ticks: u64,
    host_ticks: u64,
}

pub(super) struct ProcessTreeUsage {
    pub(super) process_count: Option<u32>,
    pub(super) resident_memory_bytes: Option<u64>,
    pub(super) cpu_percent: Option<f64>,
    pub(super) counters: Option<CpuCounters>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct CgroupCpuCounters {
    usage_usec: u64,
    sampled_at: Instant,
}

pub(super) struct CgroupUsage {
    pub(super) process_count: Option<u32>,
    pub(super) task_count: Option<u64>,
    pub(super) memory_current_bytes: Option<u64>,
    pub(super) cpu_percent: Option<f64>,
    pub(super) cpu_usage_usec: Option<u64>,
    pub(super) events: Option<WorkspaceResourceEvents>,
    pub(super) counters: Option<CgroupCpuCounters>,
}

#[cfg(target_os = "linux")]
pub(super) fn inspect(
    root_pid: u32,
    previous: Option<CpuCounters>,
) -> io::Result<Option<ProcessTreeUsage>> {
    linux::inspect(root_pid, previous)
}

#[cfg(target_os = "linux")]
pub(super) fn inspect_cgroup(
    path: &Path,
    previous: Option<CgroupCpuCounters>,
) -> io::Result<Option<CgroupUsage>> {
    linux::inspect_cgroup(path, previous)
}

#[cfg(not(target_os = "linux"))]
pub(super) fn inspect_cgroup(
    _path: &Path,
    _previous: Option<CgroupCpuCounters>,
) -> io::Result<Option<CgroupUsage>> {
    Ok(None)
}

#[cfg(not(target_os = "linux"))]
pub(super) fn inspect(
    _root_pid: u32,
    _previous: Option<CpuCounters>,
) -> io::Result<Option<ProcessTreeUsage>> {
    Ok(Some(ProcessTreeUsage {
        process_count: None,
        resident_memory_bytes: None,
        cpu_percent: None,
        counters: None,
    }))
}

#[cfg(target_os = "linux")]
mod linux {
    use std::collections::{HashMap, HashSet};
    use std::fs;
    use std::io;
    use std::path::Path;
    use std::time::Instant;

    use super::{CgroupCpuCounters, CgroupUsage, CpuCounters, ProcessTreeUsage};
    use crate::domain::runtime::WorkspaceResourceEvents;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct ProcessRecord {
        parent_pid: u32,
        cpu_ticks: u64,
        resident_memory_bytes: u64,
    }

    pub(super) fn inspect(
        root_pid: u32,
        previous: Option<CpuCounters>,
    ) -> io::Result<Option<ProcessTreeUsage>> {
        let processes = read_processes()?;
        if !processes.contains_key(&root_pid) {
            return Ok(None);
        }
        let members = process_tree(root_pid, &processes);
        let process_ticks = members.iter().fold(0_u64, |total, pid| {
            total.saturating_add(processes[pid].cpu_ticks)
        });
        let resident_memory_bytes = members.iter().fold(0_u64, |total, pid| {
            total.saturating_add(processes[pid].resident_memory_bytes)
        });
        let (host_ticks, cpu_count) = read_host_cpu()?;
        let counters = CpuCounters {
            process_ticks,
            host_ticks,
        };
        let cpu_percent = previous.and_then(|previous| cpu_percent(previous, counters, cpu_count));
        Ok(Some(ProcessTreeUsage {
            process_count: u32::try_from(members.len()).ok(),
            resident_memory_bytes: Some(resident_memory_bytes),
            cpu_percent,
            counters: Some(counters),
        }))
    }

    pub(super) fn inspect_cgroup(
        path: &Path,
        previous: Option<CgroupCpuCounters>,
    ) -> io::Result<Option<CgroupUsage>> {
        let populated = match read_keyed_u64(&path.join("cgroup.events")) {
            Ok(events) => events.get("populated").copied(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        if populated == Some(0) {
            return Ok(None);
        }

        let memory_current_bytes = read_u64(&path.join("memory.current"))?;
        let task_count = read_u64(&path.join("pids.current"))?;
        let process_count = u32::try_from(
            fs::read_to_string(path.join("cgroup.procs"))?
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count(),
        )
        .ok();
        let cpu = read_keyed_u64(&path.join("cpu.stat"))?;
        let usage_usec = cpu.get("usage_usec").copied();
        let sampled_at = Instant::now();
        let counters = usage_usec.map(|usage_usec| CgroupCpuCounters {
            usage_usec,
            sampled_at,
        });
        let cpu_percent = previous.and_then(|previous| {
            counters.and_then(|current| cgroup_cpu_percent(previous, current))
        });

        let memory = read_keyed_u64(&path.join("memory.events"))?;
        let pids = read_keyed_u64(&path.join("pids.events"))?;
        let events = WorkspaceResourceEvents {
            memory_high: memory.get("high").copied(),
            memory_max: memory.get("max").copied(),
            memory_oom: memory.get("oom").copied(),
            memory_oom_kill: memory.get("oom_kill").copied(),
            pids_max: pids.get("max").copied(),
            cpu_throttled_periods: cpu.get("nr_throttled").copied(),
            cpu_throttled_usec: cpu.get("throttled_usec").copied(),
        };

        Ok(Some(CgroupUsage {
            process_count,
            task_count: Some(task_count),
            memory_current_bytes: Some(memory_current_bytes),
            cpu_percent,
            cpu_usage_usec: usage_usec,
            events: Some(events),
            counters,
        }))
    }

    fn read_u64(path: &Path) -> io::Result<u64> {
        fs::read_to_string(path)?
            .trim()
            .parse()
            .map_err(|_| io::Error::other(format!("invalid integer in {}", path.display())))
    }

    fn read_keyed_u64(path: &Path) -> io::Result<HashMap<String, u64>> {
        parse_keyed_u64(&fs::read_to_string(path)?)
            .ok_or_else(|| io::Error::other(format!("invalid counters in {}", path.display())))
    }

    fn parse_keyed_u64(contents: &str) -> Option<HashMap<String, u64>> {
        contents
            .lines()
            .filter(|line| !line.trim().is_empty())
            .try_fold(HashMap::new(), |mut values, line| {
                let mut fields = line.split_whitespace();
                let key = fields.next()?;
                let value = fields.next()?.parse().ok()?;
                if fields.next().is_some() || values.insert(key.to_owned(), value).is_some() {
                    return None;
                }
                Some(values)
            })
    }

    fn cgroup_cpu_percent(previous: CgroupCpuCounters, current: CgroupCpuCounters) -> Option<f64> {
        let elapsed = current
            .sampled_at
            .checked_duration_since(previous.sampled_at)?;
        let elapsed_usec = elapsed.as_micros();
        if elapsed_usec == 0 {
            return None;
        }
        let usage_delta = current.usage_usec.saturating_sub(previous.usage_usec);
        let percent = usage_delta as f64 / elapsed_usec as f64 * 100.0;
        let cpu_count = std::thread::available_parallelism()
            .ok()
            .and_then(|value| u32::try_from(value.get()).ok())
            .unwrap_or(1);
        Some(percent.clamp(0.0, f64::from(cpu_count) * 100.0))
    }

    fn read_processes() -> io::Result<HashMap<u32, ProcessRecord>> {
        let mut processes = HashMap::new();
        for entry in fs::read_dir("/proc")? {
            let Ok(entry) = entry else {
                continue;
            };
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<u32>().ok())
            else {
                continue;
            };
            let Ok(stat) = fs::read_to_string(entry.path().join("stat")) else {
                continue;
            };
            let Some((parent_pid, cpu_ticks)) = parse_process_stat(&stat) else {
                continue;
            };
            let resident_memory_bytes = fs::read_to_string(entry.path().join("status"))
                .ok()
                .and_then(|status| parse_resident_memory(&status))
                .unwrap_or(0);
            processes.insert(
                pid,
                ProcessRecord {
                    parent_pid,
                    cpu_ticks,
                    resident_memory_bytes,
                },
            );
        }
        Ok(processes)
    }

    fn parse_process_stat(stat: &str) -> Option<(u32, u64)> {
        // The command name is parenthesized and may itself contain spaces or
        // parentheses, so fields after it must be located from the last `) `.
        let fields = stat
            .rsplit_once(") ")?
            .1
            .split_whitespace()
            .collect::<Vec<_>>();
        let parent_pid = fields.get(1)?.parse().ok()?;
        let user_ticks = fields.get(11)?.parse::<u64>().ok()?;
        let system_ticks = fields.get(12)?.parse::<u64>().ok()?;
        Some((parent_pid, user_ticks.saturating_add(system_ticks)))
    }

    fn parse_resident_memory(status: &str) -> Option<u64> {
        let kibibytes = status.lines().find_map(|line| {
            line.strip_prefix("VmRSS:")?
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()
        })?;
        kibibytes.checked_mul(1024)
    }

    fn process_tree(root_pid: u32, processes: &HashMap<u32, ProcessRecord>) -> HashSet<u32> {
        let mut children = HashMap::<u32, Vec<u32>>::new();
        for (&pid, process) in processes {
            children.entry(process.parent_pid).or_default().push(pid);
        }
        let mut members = HashSet::from([root_pid]);
        let mut pending = vec![root_pid];
        while let Some(parent) = pending.pop() {
            for child in children.get(&parent).into_iter().flatten() {
                if members.insert(*child) {
                    pending.push(*child);
                }
            }
        }
        members
    }

    fn read_host_cpu() -> io::Result<(u64, u32)> {
        let stat = fs::read_to_string("/proc/stat")?;
        parse_host_cpu(&stat).ok_or_else(|| io::Error::other("invalid /proc/stat CPU counters"))
    }

    fn parse_host_cpu(stat: &str) -> Option<(u64, u32)> {
        let mut lines = stat.lines();
        let aggregate = lines.next()?.strip_prefix("cpu ")?;
        // guest and guest_nice are already represented in user/nice.
        let host_ticks = aggregate
            .split_whitespace()
            .take(8)
            .try_fold(0_u64, |total, field| {
                field
                    .parse::<u64>()
                    .ok()
                    .map(|value| total.saturating_add(value))
            })?;
        let cpu_count = u32::try_from(
            lines
                .filter_map(|line| line.strip_prefix("cpu"))
                .filter(|suffix| {
                    suffix
                        .split_whitespace()
                        .next()
                        .is_some_and(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()))
                })
                .count(),
        )
        .ok()?
        .max(1);
        Some((host_ticks, cpu_count))
    }

    fn cpu_percent(previous: CpuCounters, current: CpuCounters, cpu_count: u32) -> Option<f64> {
        let host_delta = current.host_ticks.checked_sub(previous.host_ticks)?;
        if host_delta == 0 {
            return None;
        }
        let process_delta = current.process_ticks.saturating_sub(previous.process_ticks);
        let percent = process_delta as f64 / host_delta as f64 * f64::from(cpu_count) * 100.0;
        Some(percent.clamp(0.0, f64::from(cpu_count) * 100.0))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parses_process_names_with_spaces_and_parentheses() {
            let stat = "42 (odd (worker) name) S 7 1 1 0 -1 4194304 10 0 0 0 30 12 0 0 20 0";
            assert_eq!(parse_process_stat(stat), Some((7, 42)));
        }

        #[test]
        fn parses_rss_in_bytes() {
            assert_eq!(
                parse_resident_memory("Name:\ttest\nVmRSS:\t  123 kB\n"),
                Some(125_952)
            );
        }

        #[test]
        fn walks_only_the_selected_process_tree() {
            let processes = HashMap::from([
                (10, record(1)),
                (11, record(10)),
                (12, record(11)),
                (20, record(1)),
            ]);
            assert_eq!(process_tree(10, &processes), HashSet::from([10, 11, 12]));
        }

        #[test]
        fn calculates_per_core_cpu_percentage_between_samples() {
            let previous = CpuCounters {
                process_ticks: 100,
                host_ticks: 1_000,
            };
            let current = CpuCounters {
                process_ticks: 110,
                host_ticks: 1_080,
            };
            assert_eq!(cpu_percent(previous, current, 8), Some(100.0));
        }

        #[test]
        fn parses_aggregate_and_logical_cpu_counters() {
            let stat = "cpu  1 2 3 4 5 6 7 8 900 1000\ncpu0 1 2 3 4\ncpu1 1 2 3 4\nintr 1\n";
            assert_eq!(parse_host_cpu(stat), Some((36, 2)));
        }

        #[test]
        fn parses_cgroup_key_value_counters_strictly() {
            assert_eq!(
                parse_keyed_u64("usage_usec 42\nnr_throttled 3\n")
                    .unwrap()
                    .get("usage_usec"),
                Some(&42)
            );
            assert!(parse_keyed_u64("usage_usec nope\n").is_none());
            assert!(parse_keyed_u64("usage_usec 1 extra\n").is_none());
            assert!(parse_keyed_u64("usage_usec 1\nusage_usec 2\n").is_none());
        }

        #[test]
        fn calculates_cgroup_cpu_as_whole_core_percentage() {
            let sampled_at = Instant::now();
            let previous = CgroupCpuCounters {
                usage_usec: 1_000,
                sampled_at,
            };
            let current = CgroupCpuCounters {
                usage_usec: 501_000,
                sampled_at: sampled_at + std::time::Duration::from_secs(1),
            };
            assert_eq!(cgroup_cpu_percent(previous, current), Some(50.0));
        }

        #[test]
        fn reads_a_complete_cgroup_snapshot_without_calling_memory_rss() {
            let directory = tempfile::tempdir().unwrap();
            fs::write(directory.path().join("cgroup.events"), "populated 1\n").unwrap();
            fs::write(directory.path().join("cgroup.procs"), "10\n11\n").unwrap();
            fs::write(
                directory.path().join("cpu.stat"),
                "usage_usec 500000\nnr_throttled 2\nthrottled_usec 3000\n",
            )
            .unwrap();
            fs::write(directory.path().join("memory.current"), "25165824\n").unwrap();
            fs::write(
                directory.path().join("memory.events"),
                "low 0\nhigh 3\nmax 1\noom 1\noom_kill 0\n",
            )
            .unwrap();
            fs::write(directory.path().join("pids.current"), "17\n").unwrap();
            fs::write(directory.path().join("pids.events"), "max 4\n").unwrap();

            let usage = inspect_cgroup(directory.path(), None).unwrap().unwrap();
            assert_eq!(usage.process_count, Some(2));
            assert_eq!(usage.task_count, Some(17));
            assert_eq!(usage.memory_current_bytes, Some(25_165_824));
            assert_eq!(usage.cpu_usage_usec, Some(500_000));
            assert_eq!(usage.cpu_percent, None);
            let events = usage.events.unwrap();
            assert_eq!(events.memory_high, Some(3));
            assert_eq!(events.memory_max, Some(1));
            assert_eq!(events.memory_oom, Some(1));
            assert_eq!(events.memory_oom_kill, Some(0));
            assert_eq!(events.pids_max, Some(4));
            assert_eq!(events.cpu_throttled_periods, Some(2));
            assert_eq!(events.cpu_throttled_usec, Some(3_000));
        }

        const fn record(parent_pid: u32) -> ProcessRecord {
            ProcessRecord {
                parent_pid,
                cpu_ticks: 0,
                resident_memory_bytes: 0,
            }
        }
    }
}
