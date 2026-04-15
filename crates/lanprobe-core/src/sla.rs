use serde::Serialize;

#[derive(Debug, Serialize, Default)]
pub struct SlaStats {
    pub ip: String,
    pub uptime_pct: f64,
    pub avg_latency_ms: Option<f64>,
    pub min_latency_ms: Option<u64>,
    pub max_latency_ms: Option<u64>,
    pub p95_latency_ms: Option<u64>,
    pub total_samples: usize,
    pub failed_samples: usize,
}

#[derive(Debug)]
pub struct PingSample {
    pub alive: bool,
    pub latency_ms: Option<u64>,
}

pub fn compute_sla(ip: &str, samples: &[PingSample]) -> SlaStats {
    if samples.is_empty() {
        return SlaStats {
            ip: ip.to_string(),
            uptime_pct: 0.0,
            avg_latency_ms: None,
            min_latency_ms: None,
            max_latency_ms: None,
            p95_latency_ms: None,
            total_samples: 0,
            failed_samples: 0,
        };
    }
    let total = samples.len();
    let failed = samples.iter().filter(|s| !s.alive).count();
    let uptime_pct = ((total - failed) as f64 / total as f64) * 100.0;

    let latencies: Vec<u64> = samples.iter().filter_map(|s| s.latency_ms).collect();
    let (avg, min, max, p95) = if latencies.is_empty() {
        (None, None, None, None)
    } else {
        let avg = latencies.iter().sum::<u64>() as f64 / latencies.len() as f64;
        let min = *latencies.iter().min().unwrap();
        let max = *latencies.iter().max().unwrap();
        let mut sorted = latencies.clone();
        sorted.sort();
        let p95_idx = (sorted.len() as f64 * 0.95) as usize;
        let p95 = sorted[p95_idx.min(sorted.len() - 1)];
        (Some(avg), Some(min), Some(max), Some(p95))
    };

    SlaStats { ip: ip.to_string(), uptime_pct, avg_latency_ms: avg, min_latency_ms: min, max_latency_ms: max, p95_latency_ms: p95, total_samples: total, failed_samples: failed }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sla_perfect_uptime() {
        let samples = vec![
            PingSample { alive: true, latency_ms: Some(10) },
            PingSample { alive: true, latency_ms: Some(20) },
            PingSample { alive: true, latency_ms: Some(15) },
        ];
        let stats = compute_sla("8.8.8.8", &samples);
        assert_eq!(stats.uptime_pct, 100.0);
        assert_eq!(stats.min_latency_ms, Some(10));
        assert_eq!(stats.max_latency_ms, Some(20));
    }

    #[test]
    fn test_sla_partial_uptime() {
        let samples = vec![
            PingSample { alive: true, latency_ms: Some(10) },
            PingSample { alive: false, latency_ms: None },
        ];
        let stats = compute_sla("8.8.8.8", &samples);
        assert_eq!(stats.uptime_pct, 50.0);
        assert_eq!(stats.failed_samples, 1);
    }

    #[test]
    fn test_sla_empty() {
        let stats = compute_sla("8.8.8.8", &[]);
        assert_eq!(stats.total_samples, 0);
        assert_eq!(stats.uptime_pct, 0.0);
    }
}
