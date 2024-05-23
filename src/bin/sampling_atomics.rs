use std::time::{Duration};
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
use tracing_subscriber::FmtSubscriber;
use tracing::info;
use tokio::time;
use std::collections::HashMap;
use rand_distr::{SkewNormal, Distribution};

const N: usize = 100;

const INTERVAL_SWEEP: [Duration; 4] = [
    //Duration::from_millis(5000),
    Duration::from_millis(1000),
    Duration::from_millis(500),
    //Duration::from_millis(200),
    Duration::from_millis(100),
    //Duration::from_millis(50),
    //Duration::from_millis(20),
    //Duration::from_millis(15),
    Duration::from_millis(10),
    //Duration::from_millis(1),
    //Duration::from_micros(100),
    //Duration::from_micros(10),
    //Duration::from_micros(1),
    //Duration::from_nanos(1),
];

#[tokio::main]
async fn main() {
    FmtSubscriber::builder().init();

    //let data = collect_data(100, (Duration::from_millis(800), Duration::from_millis(600))).await;
    //let data = collect_data(1000, (Duration::from_millis(10), Duration::from_millis(4))).await;
    let data = collect_data(100, (Duration::from_micros(10), Duration::from_micros(4))).await;
    plot(data);
}

async fn collect_data(task_count: usize, delay: (Duration, Duration)) -> HashMap<Duration, Vec<f64>> {
    let s_atomic = Arc::new(AtomicU64::new(0));
    let mut handles = vec![];

    for _ in 0..task_count {
        let s_atomic = s_atomic.clone();
        let handle = tokio::spawn(async move {
            task(delay.0, delay.1, s_atomic).await;
        });
        handles.push(handle);
    }

    let mut data = HashMap::new();
    for sample_interval in INTERVAL_SWEEP {
        let mut s_vals = vec![];
        // NOTE: Have to reset the atomics otherwise we get some weird data issues.
        s_atomic.store(0, Ordering::Relaxed);
        let mut s_timer = Timer::new(sample_interval).await;
        loop {
            let elapsed = s_timer.tick().await;

            let val = s_atomic.swap(0, Ordering::Relaxed);
            let tps = val as f64 / elapsed.as_secs_f64();
            s_vals.push(tps);

            if s_vals.len() >= N {
                break
            }
        }

        info!("{sample_interval:?}");
        info!("s_vals: {:?}..{:?}", &s_vals[..5], &s_vals[s_vals.len() - 5..]);

        let (s_mean, s_std) = avg_tps(&s_vals);
        info!("s_tps: {s_mean}, std: {s_std}");

        data.insert(sample_interval, s_vals);
    }

    for handle in &handles {
        handle.abort();
    }

    for handle in handles {
        assert!(handle.await.unwrap_err().is_cancelled());
    }

    data
}

async fn task(delay: Duration, std: Duration, s_atomic: Arc<AtomicU64>) {
    loop {
        let normal =
            SkewNormal::new(delay.as_secs_f64(), std.as_secs_f64(), 20.).unwrap();
        let v: f64 = normal.sample(&mut rand::thread_rng()).max(0.);
        tokio::time::sleep(std::time::Duration::from_secs_f64(v)).await;
        //tokio::time::sleep(delay).await;
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


/// A simple wrapper around tokio::time::Interval to make
/// getting the actual `elapsed()` time easy.
struct Timer {
    interval: time::Interval,
    prev: time::Instant,
}

impl Timer {
    async fn new(dur: Duration) -> Self {
        let mut interval = time::interval(dur);
        // First is instant
        let prev = interval.tick().await;
        Self {
            interval,
            prev
        }
    }

    async fn tick(&mut self) -> Duration {
        let new = self.interval.tick().await;
        let elapsed = self.prev.elapsed();
        self.prev = new;

        elapsed
    }
}

fn plot(data: HashMap<Duration, Vec<f64>>) {
    use plotly::{Plot, Scatter, Layout, layout::{Axis, Legend}, common::Mode};

    let mut plot = Plot::new();

    println!("| Interval | Mean TPS (μ) | Std (σ) |");
    println!("| --- | --- | --- |");
    for sample_interval in INTERVAL_SWEEP {
        let d = data.get(&sample_interval).unwrap();
        let d = d.clone();

        let (mean, std) = avg_tps(&d);

        println!("| {sample_interval:?} | {mean:.2}| {std:.2}|");

        let x: Vec<_> = (0..N).collect();
        let trace = Scatter::new(x, d)
            .name(format!("{sample_interval:?}"))
            .mode(Mode::Lines);

        let layout = Layout::new()
            .title("100 Tasks, 10us Delay with SkewNormal Noise".into())
            .y_axis(Axis::new().title("Measured TPS".into()))
            .x_axis(Axis::new().title("Samples".into()))
            .legend(Legend::new().title("Sampling Interval".into()));

        plot.add_trace(trace);
        plot.set_layout(layout);

    }
    plot.show();
    plot.write_html("out.html");
}
