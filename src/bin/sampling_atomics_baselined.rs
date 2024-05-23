use rand_distr::{Distribution, SkewNormal};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tokio::time;
use tracing::info;
use tracing_subscriber::FmtSubscriber;

const COLLECTION_DUR: Duration = Duration::from_secs(3);
const BASELINE_INTERVAL: Duration = Duration::from_millis(1000);

type DATA = Vec<(Duration, (Vec<(Duration, f64)>, Vec<(Duration, f64)>))>;

#[tokio::main]
async fn main() {
    FmtSubscriber::builder().init();

    collect_and_plot_timeseries(
        100,
        //(Duration::from_millis(10), Duration::from_millis(4)),
        (Duration::from_micros(10), Duration::from_micros(4)),
        &[
            Duration::from_millis(1000),
            Duration::from_millis(500),
            Duration::from_millis(200),
            Duration::from_millis(50),
            Duration::from_millis(10),
        ],
        false
    )
    .await;
}

async fn collect_and_plot_timeseries(
    task_count: usize,
    delay: (Duration, Duration),
    intervals: &[Duration],
    baseline: bool
) {
    let data = collect_data(
        task_count,
        delay,
        intervals,
    )
    .await;
    plot_timeseries(task_count, delay.0, &data, baseline);
    tables_timeseries(&data, baseline);
}

async fn collect_data(
    task_count: usize,
    delay: (Duration, Duration),
    intervals: &[Duration],
) -> DATA {
    let s_atomic = Arc::new(AtomicU64::new(0));
    let b_atomic = Arc::new(AtomicU64::new(0));
    let mut handles = vec![];

    for _ in 0..task_count {
        let s_atomic = s_atomic.clone();
        let b_atomic = b_atomic.clone();

        let handle = tokio::spawn(async move {
            task(delay.0, delay.1, s_atomic, b_atomic).await;
        });
        handles.push(handle);
    }

    let mut data = vec![];
    for sample_interval in intervals {
        let (s_vals, b_vals) =
            get_interval_data_baselined(*sample_interval, &s_atomic, &b_atomic).await;
        data.push((*sample_interval, (s_vals, b_vals)));
    }

    for handle in &handles {
        handle.abort();
    }

    for handle in handles {
        assert!(handle.await.unwrap_err().is_cancelled());
    }

    data
}

async fn get_interval_data_baselined(
    interval: Duration,
    s_atomic: &AtomicU64,
    b_atomic: &AtomicU64,
) -> (Vec<(Duration, f64)>, Vec<(Duration, f64)>) {
    let mut s_vals = vec![];
    let mut b_vals = vec![];

    let start = Instant::now();
    tokio::join! {
        async {
            let mut s_timer = Timer::new(interval).await;
            s_atomic.store(0, Ordering::Relaxed);
            loop {
                let elapsed = s_timer.tick().await;
                let val = s_atomic.swap(0, Ordering::Relaxed);
                let tps = val as f64 / elapsed.as_secs_f64();
                s_vals.push((start.elapsed(), tps));

                if start.elapsed() >= COLLECTION_DUR {
                    break;
                }
            }
        },
        async {
            let mut b_timer = Timer::new(BASELINE_INTERVAL).await;
            b_atomic.store(0, Ordering::Relaxed);
            loop {
                let elapsed = b_timer.tick().await;
                let val = b_atomic.swap(0, Ordering::Relaxed);
                let tps = val as f64 / elapsed.as_secs_f64();
                b_vals.push((start.elapsed(), tps));

                if start.elapsed() >= COLLECTION_DUR {
                    break;
                }
            }
        }
    };

    info!("{interval:?}");
    /*
    info!(
        "s_vals: {:?}..{:?}",
        &s_vals[..5],
        &s_vals[s_vals.len() - 5..]
    );
    info!(
        "b_vals: {:?}..{:?}",
        &b_vals[..5],
        &b_vals[b_vals.len() - 5..]
    );
    */

    let (s_mean, s_std) = avg_tps(&s_vals);
    info!("s_tps: {s_mean}, std: {s_std}");

    let (b_mean, b_std) = avg_tps(&b_vals);
    info!("b_tps: {b_mean}, std: {b_std}");

    (s_vals, b_vals)
}

async fn task(delay: Duration, std: Duration, s_atomic: Arc<AtomicU64>, b_atomic: Arc<AtomicU64>) {
    loop {
        let normal = SkewNormal::new(delay.as_secs_f64(), std.as_secs_f64(), 20.).unwrap();
        let v: f64 = normal.sample(&mut rand::thread_rng()).max(0.);
        tokio::time::sleep(std::time::Duration::from_secs_f64(v)).await;
        s_atomic.fetch_add(1, Ordering::Relaxed);
        b_atomic.fetch_add(1, Ordering::Relaxed);
    }
}

fn avg_tps(samples: &[(Duration, f64)]) -> (f64, f64) {
    let len = samples.len() as f64;
    let mean = samples.iter().map(|(_, x)| x).sum::<f64>() / len;

    let std = (samples.iter().map(|(_, x)| (x - mean).powi(2)).sum::<f64>() / len).sqrt();

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
        Self { interval, prev }
    }

    async fn tick(&mut self) -> Duration {
        let new = self.interval.tick().await;
        let elapsed = self.prev.elapsed();
        self.prev = new;

        elapsed
    }
}

fn plot_timeseries(task_count: usize, delay: Duration, data: &DATA, baseline: bool) {
    use plotly::{
        common::{Mode, Title},
        layout::{Axis, Legend},
        Layout, Plot, Scatter,
    };

    let mut plot = Plot::new();

    for (sample_interval, (s_vals, b_vals)) in data {
        let (x, y): (Vec<f64>, Vec<f64>) = s_vals
            .iter()
            .skip(2)
            .map(|(x, y)| (x.as_secs_f64(), *y))
            .unzip::<f64, f64, _, _>();
        let trace = Scatter::new(x, y)
            .name(format!("{sample_interval:?}"))
            .mode(Mode::Lines);

        plot.add_trace(trace);

        if baseline {
            let (x, y): (Vec<_>, Vec<_>) = b_vals
                .iter()
                .map(|(x, y)| (x.as_secs_f64(), *y))
                .unzip::<f64, f64, _, _>();
            let trace = Scatter::new(x, y)
                .name(format!("{sample_interval:?} (baseline)"))
                .mode(Mode::Lines);

            plot.add_trace(trace);
        }
    }
    let title = format!("{task_count} Tasks, {delay:?} Delay with SkewNormal Noise");
    let layout = Layout::new()
        .title(Title::new(&title))
        .y_axis(Axis::new().title("Measured TPS".into()))
        .x_axis(Axis::new().title("Time (s)".into()))
        .legend(Legend::new().title("Sampling Interval".into()));
    plot.set_layout(layout);
    plot.show();
    plot.write_html("out.html");
}

fn tables_timeseries(data: &DATA, baseline: bool) {
    println!("| Interval | Mean TPS (μ) | Std (σ) | Error |");
    println!("| --- | --- | --- | --- |");

    for (sample_interval, (s_vals, b_vals)) in data {
        let (mean, std) = avg_tps(&s_vals);
        let (b_mean, b_std) = avg_tps(&b_vals);

        let err = (b_mean - mean) / b_mean * 100.;

        println!("| {sample_interval:?} | {mean:.2}| {std:.2}| {err:.2}% |");

        if baseline {
            println!("| {sample_interval:?} (baseline) | {b_mean:.2}| {b_std:.2}| |");
        }
    }
}
