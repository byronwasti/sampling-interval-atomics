use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, Duration};

static SAMPLING_ATOMIC: AtomicU64 = AtomicU64::new(0);
static BASELINE_ATOMIC: AtomicU64 = AtomicU64::new(0);

const TASK_COUNT: usize = 10;
const SAMPLING_INTERVAL: Duration = Duration::from_millis(2);

const DURATION: Duration = Duration::from_secs(10);

fn main() {
    for _ in 0..TASK_COUNT {
        std::thread::spawn(task);
    }

    let start = Instant::now();
    let mut baseline = Instant::now();
    let mut sampling = Instant::now();

    let mut baseline_values = vec![];
    let mut sampling_values = vec![];
    loop {
        if start.elapsed() > DURATION {
            break
        }

        if baseline.elapsed() >= Duration::from_secs(1) {
            baseline = Instant::now();
            let baseline_value = BASELINE_ATOMIC.swap(0, Ordering::Relaxed);
            baseline_values.push(baseline_value);
        }

        if sampling.elapsed() >= SAMPLING_INTERVAL {
            sampling = Instant::now();
            let sampling_value = SAMPLING_ATOMIC.swap(0, Ordering::Relaxed);
            sampling_values.push(sampling_value);
        }
    }

    let b_tps = avg_tps(&baseline_values, Duration::from_secs(1));
    println!("Baseline: {b_tps} TPS");

    let s_tps = avg_tps(&sampling_values, SAMPLING_INTERVAL);
    println!("Sampled: {s_tps} TPS");

    println!("Error: {:.2}", (b_tps - s_tps) / b_tps * 100.);

}

fn task() {
    loop {
        std::thread::sleep(Duration::from_millis(1));
        SAMPLING_ATOMIC.fetch_add(1, Ordering::Relaxed);
        BASELINE_ATOMIC.fetch_add(1, Ordering::Relaxed);
    }
}

fn avg_tps(samples: &[u64], duration: Duration) -> f64 {
    let len = samples.len();
    let avg_samples = samples.iter().sum::<u64>() as f64 / len as f64;
    avg_samples / duration.as_secs_f64()
}
