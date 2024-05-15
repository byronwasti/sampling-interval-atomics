use std::time::{Duration, Instant};
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
use tracing_subscriber::FmtSubscriber;
use tracing::info;

const BASELINE_INTERVAL: Duration = Duration::from_secs(1);

const INTERVAL_SWEEP: [Duration; 3] = [
    Duration::from_millis(500),
    //Duration::from_millis(100),
    Duration::from_millis(10),
    //Duration::from_millis(1),
    //Duration::from_micros(100),
    //Duration::from_micros(10),
    Duration::from_micros(1),
    //Duration::from_nanos(1),
];

#[tokio::main]
async fn main() {
    FmtSubscriber::builder().init();

    let data = collect_data(100, Duration::from_millis(10)).await;
    println!("{data:?}");
}

async fn collect_data(task_count: usize, delay: Duration) -> Vec<(f64, f64)> {
    let b_atomic = Arc::new(AtomicU64::new(0));
    let s_atomic = Arc::new(AtomicU64::new(0));
    let mut handles = vec![];

    for _ in 0..task_count {
        let b_atomic = b_atomic.clone();
        let s_atomic = s_atomic.clone();
        let handle = tokio::spawn(async move {
            task(delay, b_atomic, s_atomic).await;
        });
        handles.push(handle);
    }

    let mut data = vec![];
    for sample_interval in INTERVAL_SWEEP {
        // NOTE: Have to reset the atomics otherwise we get some weird data issues.
        b_atomic.store(0, Ordering::Relaxed);
        s_atomic.store(0, Ordering::Relaxed);

        let start = Instant::now();
        let mut b_timer = Instant::now();
        let mut s_timer = Instant::now();

        let mut b_vals = vec![];
        let mut s_vals = vec![];

        loop {
            if start.elapsed() > Duration::from_secs(10) {
                break
            }

            if b_timer.elapsed() > BASELINE_INTERVAL {
                b_timer = Instant::now();
                let val = b_atomic.swap(0, Ordering::Relaxed);
                let tps = if val != 0 {
                    val as f64 / BASELINE_INTERVAL.as_secs_f64()
                } else {
                    0.
                };
                b_vals.push(tps);
            }

            if s_timer.elapsed() > sample_interval {
                s_timer = Instant::now();
                let val = s_atomic.swap(0, Ordering::Relaxed);
                let tps = if val != 0 {
                    val as f64 / sample_interval.as_secs_f64()
                } else {
                    0.
                };
                s_vals.push(tps);
            }
        }

        info!("{sample_interval:?}");
        info!("b_vals: {b_vals:?}");
        info!("s_vals: {:?}..{:?}", &s_vals[..5], &s_vals[s_vals.len() - 5..]);

        let (b_mean, b_std) = avg_tps(&b_vals);
        let (s_mean, s_std) = avg_tps(&s_vals);
        info!("b_tps: {b_mean}, std: {b_std}");
        info!("s_tps: {s_mean}, std: {s_std}");

        let err_per = ((b_mean - s_mean) / b_mean) * 100.;
        info!("err: {err_per}%");
        data.push((sample_interval.as_secs_f64(), err_per));
    }

    for handle in &handles {
        handle.abort();
    }

    for handle in handles {
        assert!(handle.await.unwrap_err().is_cancelled());
    }

    data
}

async fn task(delay: Duration, b_atomic: Arc<AtomicU64>, s_atomic: Arc<AtomicU64>) {
    loop {
        tokio::time::sleep(delay).await;
        b_atomic.fetch_add(1, Ordering::Relaxed);
        s_atomic.fetch_add(1, Ordering::Relaxed);
    }
}

fn avg_tps(samples: &[f64]) -> (f64, f64) {
    let len = samples.len() as f64;
    let mean = samples.iter().sum::<f64>() / len;

    let std = (samples.iter()
        .map(|x| (x - mean).powi(2))
        .sum::<f64>() / len).sqrt();

    (mean, std)
}

