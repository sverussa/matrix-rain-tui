//! SysMonitor: lê CPU/memória numa thread própria e manda um `SysSnapshot` por segundo.
//! O loop de render só faz `try_recv`, então uma leitura lenta nunca derruba o FPS.

use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use sysinfo::System;

/// Amostras de CPU guardadas (1 por segundo).
pub const CPU_HISTORY: usize = 30;

#[derive(Clone, Debug, Default)]
pub struct SysSnapshot {
    /// CPU total em %.
    pub cpu: f32,
    /// Últimas amostras de `cpu`, a mais recente no fim.
    pub cpu_hist: Vec<f32>,
    /// % por núcleo.
    pub cores: Vec<f32>,
    pub mem_used: u64,
    pub mem_total: u64,
    /// Load average de 1, 5 e 15 min (0 no Windows).
    pub load: [f64; 3],
    /// Segundos desde o boot.
    pub uptime: u64,
}

/// Sobe a thread do monitor. Ela encerra sozinha quando o receptor é descartado.
pub fn spawn() -> Receiver<SysSnapshot> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut sys = System::new();
        let mut hist = Vec::with_capacity(CPU_HISTORY + 1);
        sys.refresh_cpu_usage(); // primeira leitura: a % vem da diferença entre duas
        loop {
            thread::sleep(Duration::from_secs(1));
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            let cpu = sys.global_cpu_usage();
            hist.push(cpu);
            if hist.len() > CPU_HISTORY {
                hist.remove(0);
            }
            let l = System::load_average();
            let snap = SysSnapshot {
                cpu,
                cpu_hist: hist.clone(),
                cores: sys.cpus().iter().map(|c| c.cpu_usage()).collect(),
                mem_used: sys.used_memory(),
                mem_total: sys.total_memory(),
                load: [l.one, l.five, l.fifteen],
                uptime: System::uptime(),
            };
            if tx.send(snap).is_err() {
                break;
            }
        }
    });
    rx
}

/// Sparkline em texto, uma barra por amostra (0–100%).
pub fn spark(samples: &[f32]) -> String {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    samples.iter().map(|&v| BARS[((v.clamp(0.0, 100.0) / 100.0) * 7.0).round() as usize]).collect()
}

/// Bytes → GiB.
pub fn gib(bytes: u64) -> f64 {
    bytes as f64 / (1u64 << 30) as f64
}

/// `3d04h` ou `04h12m`.
pub fn uptime(secs: u64) -> String {
    let (d, h, m) = (secs / 86_400, secs / 3600 % 24, secs / 60 % 60);
    if d > 0 {
        format!("{d}d{h:02}h")
    } else {
        format!("{h:02}h{m:02}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_sends_snapshots() {
        let rx = spawn();
        let s = rx.recv_timeout(Duration::from_secs(3)).expect("sem snapshot em 3 s");
        assert!(s.mem_total > 0 && s.mem_used <= s.mem_total && !s.cores.is_empty() && s.cpu_hist == [s.cpu]);
        assert_eq!(spark(&[0.0, 50.0, 100.0, 150.0]), "▁▅██");
        assert_eq!((uptime(3 * 86_400 + 4 * 3600), uptime(3700)), ("3d04h".into(), "01h01m".into()));
    }
}
