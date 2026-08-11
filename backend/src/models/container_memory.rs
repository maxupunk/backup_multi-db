//! Leitura de memória consciente de cgroup para a coleta da Fase 11.
//!
//! Dentro de um container, a memória do host não é o limite que o OOM killer
//! enxerga. Este probe prioriza cgroup v2 e v1 e só recorre ao host quando não
//! há limite aplicável.

use std::path::Path;

use serde::Serialize;

const UNLIMITED_THRESHOLD: u64 = 1 << 53;
const V2_LIMIT: &str = "/sys/fs/cgroup/memory.max";
const V2_USAGE: &str = "/sys/fs/cgroup/memory.current";
const V2_STAT: &str = "/sys/fs/cgroup/memory.stat";
const V1_LIMIT: &str = "/sys/fs/cgroup/memory/memory.limit_in_bytes";
const V1_USAGE: &str = "/sys/fs/cgroup/memory/memory.usage_in_bytes";
const V1_STAT: &str = "/sys/fs/cgroup/memory/memory.stat";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    pub source: String,
    pub container_limited: bool,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
}

/// Converte `memory.max`/`memory.limit_in_bytes`; `max` e sentinelas enormes
/// significam que não há teto efetivo.
pub fn parse_limit(raw: &str) -> Option<u64> {
    let value = raw.trim();
    let parsed = value.parse::<u64>().ok()?;
    (parsed > 0 && parsed < UNLIMITED_THRESHOLD).then_some(parsed)
}
pub fn parse_usage(raw: &str) -> Option<u64> {
    raw.trim().parse::<u64>().ok()
}
pub fn inactive_file(raw: &str) -> u64 {
    raw.lines()
        .find_map(|line| {
            let mut values = line.split_whitespace();
            let key = values.next()?;
            (key == "inactive_file" || key == "total_inactive_file")
                .then(|| values.next()?.parse().ok())
                .flatten()
        })
        .unwrap_or(0)
}

/// Obtém a memória disponível para o processo sem usar arquivos inexistentes
/// como erro: Windows e hosts sem cgroup usam a leitura do próprio SO.
pub fn read() -> Reading {
    read_cgroup("cgroup-v2", V2_LIMIT, V2_USAGE, V2_STAT)
        .or_else(|| read_cgroup("cgroup-v1", V1_LIMIT, V1_USAGE, V1_STAT))
        .unwrap_or_else(host_reading)
}
fn read_cgroup(
    source: &str,
    limit_path: &str,
    usage_path: &str,
    stat_path: &str,
) -> Option<Reading> {
    let limit = parse_limit(&std::fs::read_to_string(Path::new(limit_path)).ok()?)?;
    let usage = parse_usage(&std::fs::read_to_string(Path::new(usage_path)).ok()?)?;
    let reclaimable = std::fs::read_to_string(Path::new(stat_path))
        .ok()
        .map(|value| inactive_file(&value))
        .unwrap_or(0);
    let used = usage.saturating_sub(reclaimable).min(limit);
    Some(Reading {
        source: source.into(),
        container_limited: true,
        total_bytes: limit,
        used_bytes: used,
        free_bytes: limit.saturating_sub(used),
    })
}
fn host_reading() -> Reading {
    let memory = sysinfo::System::new_all();
    let total = memory.total_memory();
    let available = memory.available_memory();
    Reading {
        source: "os".into(),
        container_limited: false,
        total_bytes: total,
        used_bytes: total.saturating_sub(available),
        free_bytes: available,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_cgroup_limits_and_reclaimable_cache() {
        assert_eq!(parse_limit("max"), None);
        assert_eq!(parse_limit("9223372036854771712"), None);
        assert_eq!(parse_limit("1048576"), Some(1048576));
        assert_eq!(inactive_file("anon 42\ninactive_file 128\n"), 128);
        assert_eq!(inactive_file("total_inactive_file 256"), 256);
    }
}
