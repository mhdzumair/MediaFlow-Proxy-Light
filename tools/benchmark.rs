#[cfg(feature = "benchmark")]
use clap::Parser;
use futures_util::TryStreamExt;
#[cfg(feature = "benchmark")]
use reqwest::Client;
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Semaphore};
use tokio::time::sleep;

#[cfg(feature = "benchmark")]
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// First proxy URL (e.g., http://localhost:8888)
    #[arg(long)]
    proxy1_url: String,

    /// First proxy API password
    #[arg(long)]
    proxy1_password: String,

    /// Second proxy URL (e.g., http://localhost:8000)
    #[arg(long)]
    proxy2_url: String,

    /// Second proxy API password
    #[arg(long)]
    proxy2_password: String,

    /// Display name for first proxy
    #[arg(long, default_value = "MediaFlow Proxy 1")]
    proxy1_name: String,

    /// Display name for second proxy
    #[arg(long, default_value = "MediaFlow Proxy 2")]
    proxy2_name: String,

    /// First proxy container name (optional, for Docker metrics)
    #[arg(long)]
    proxy1_container: Option<String>,

    /// Second proxy container name (optional, for Docker metrics)
    #[arg(long)]
    proxy2_container: Option<String>,

    /// Test video URL
    #[arg(long)]
    video_url: String,

    /// Number of concurrent connections
    #[arg(long, default_value = "20")]
    concurrent_connections: usize,

    /// Test duration in seconds
    #[arg(long, default_value = "30")]
    duration: u64,

    /// Request timeout in seconds
    #[arg(long, default_value = "60")]
    request_timeout: u64,

    /// Connection timeout in seconds
    #[arg(long, default_value = "10")]
    connect_timeout: u64,
}

#[cfg(feature = "benchmark")]
#[derive(Debug)]
struct MetricsSnapshot {
    cpu_usage: Option<f64>,
    memory_usage: Option<u64>,
    response_time: Duration,
}

#[cfg(feature = "benchmark")]
struct BenchmarkResults {
    avg_cpu: Option<f64>,
    max_cpu: Option<f64>,
    avg_memory: Option<u64>,
    max_memory: Option<u64>,
    avg_response_time: Duration,
    min_response_time: Duration,
    max_response_time: Duration,
    successful_requests: usize,
    failed_requests: usize,
    total_bytes_transferred: u64,
    avg_speed_mbps: f64,
    peak_speed_mbps: f64,
}

#[cfg(feature = "benchmark")]
async fn get_container_stats(
    docker: &bollard::Docker,
    container_id: &str,
) -> Result<(f64, u64), Box<dyn Error>> {
    use bollard::container::StatsOptions;
    use futures_util::StreamExt;

    let stats_options = StatsOptions {
        stream: false, // We want a single snapshot
        ..Default::default()
    };

    let stats = docker.stats(container_id, Some(stats_options));
    tokio::pin!(stats);

    if let Some(Ok(stat)) = stats.next().await {
        let cpu_delta = stat.cpu_stats.cpu_usage.total_usage as f64
            - stat.precpu_stats.cpu_usage.total_usage as f64;
        let system_delta = stat.cpu_stats.system_cpu_usage.unwrap_or(0) as f64
            - stat.precpu_stats.system_cpu_usage.unwrap_or(0) as f64;

        let cpu_usage = if system_delta > 0.0 && cpu_delta > 0.0 {
            (cpu_delta / system_delta) * 100.0 * stat.cpu_stats.online_cpus.unwrap_or(1) as f64
        } else {
            0.0
        };

        let memory_usage = stat.memory_stats.usage.unwrap_or(0);

        Ok((cpu_usage, memory_usage))
    } else {
        Err("Failed to get container stats".into())
    }
}

#[cfg(feature = "benchmark")]
async fn test_proxy(
    client: &Client,
    docker: Option<&bollard::Docker>,
    proxy_url: &str,
    api_password: &str,
    video_url: &str,
    container_name: Option<&str>,
    concurrent_connections: usize,
    duration: u64,
) -> Result<BenchmarkResults, Box<dyn Error>> {
    // Add semaphore to limit concurrent connections
    let semaphore = Arc::new(Semaphore::new(concurrent_connections));
    // Use rolling metrics window instead of storing all metrics
    let recent_metrics = Arc::new(Mutex::new(Vec::with_capacity(100)));
    let start_time = Instant::now();
    let end_time = start_time + Duration::from_secs(duration);

    println!("Starting benchmark for {}", proxy_url);

    let mut successful_requests = 0;
    let mut failed_requests = 0;
    let total_bytes = Arc::new(Mutex::new(0u64));
    let peak_speed = Arc::new(Mutex::new(0f64));

    while Instant::now() < end_time {
        // Only collect container metrics every second
        let container_metrics = if let (Some(docker), Some(container)) = (docker, container_name) {
            if Instant::now().duration_since(start_time).as_secs() % 1 == 0 {
                match get_container_stats(docker, container).await {
                    Ok((cpu, memory)) => Some((cpu, memory)),
                    Err(_) => None,
                }
            } else {
                None
            }
        } else {
            None
        };

        let mut batch_successful = 0;
        let mut batch_failed = 0;

        let futures = (0..concurrent_connections).map(|_| {
            let permit = semaphore.clone().acquire_owned();
            let client = client.clone();
            let url = format!(
                "{}/proxy/stream?d={}&api_password={}",
                proxy_url, video_url, api_password
            );
            let total_bytes = Arc::clone(&total_bytes);
            let peak_speed = Arc::clone(&peak_speed);

            async move {
                // Wait for semaphore permit
                let _permit = permit.await.unwrap();
                let start = Instant::now();
                match client
                    .get(&url)
                    .timeout(Duration::from_secs(10))
                    .send()
                    .await
                {
                    Ok(response) => {
                        if response.status().is_success() {
                            // Stream the response instead of loading it all into memory
                            let mut stream = response.bytes_stream();
                            let mut bytes_len = 0u64;

                            while let Ok(Some(chunk)) = stream.try_next().await {
                                bytes_len += chunk.len() as u64;
                            }

                            let duration = start.elapsed();
                            let speed_mbps =
                                (bytes_len as f64 * 8.0) / (duration.as_secs_f64() * 1_000_000.0);

                            let mut total = total_bytes.lock().await;
                            *total += bytes_len;

                            let mut peak = peak_speed.lock().await;
                            if speed_mbps > *peak {
                                *peak = speed_mbps;
                            }

                            Ok((duration, bytes_len))
                        } else {
                            Err("Request failed")
                        }
                    }
                    Err(_) => Err("Request failed"),
                }
            }
        });

        let results: Vec<Result<(Duration, u64), &str>> = futures::future::join_all(futures).await;

        let mut metrics_lock = recent_metrics.lock().await;
        // Keep only recent metrics to avoid memory growth
        if metrics_lock.len() > 1000 {
            metrics_lock.drain(0..500);
        }

        for result in results {
            match result {
                Ok((duration, bytes)) => {
                    batch_successful += 1;
                    metrics_lock.push(MetricsSnapshot {
                        cpu_usage: container_metrics.as_ref().map(|(cpu, _)| *cpu),
                        memory_usage: container_metrics.as_ref().map(|(_, mem)| *mem),
                        response_time: duration,
                    });
                }
                Err(_) => {
                    batch_failed += 1;
                }
            }
        }

        successful_requests += batch_successful;
        failed_requests += batch_failed;

        if batch_successful == 0 && batch_failed > 0 {
            println!("Warning: All requests in batch failed");
        }

        // Add small delay between batches to prevent overwhelming
        sleep(Duration::from_millis(100)).await;
    }

    let metrics = recent_metrics.lock().await;
    if metrics.is_empty() {
        return Err(format!("No metrics collected for {}", proxy_url).into());
    }

    // Calculate results
    let (avg_cpu, max_cpu) = if container_name.is_some() {
        let cpu_metrics: Vec<f64> = metrics.iter().filter_map(|m| m.cpu_usage).collect();
        if !cpu_metrics.is_empty() {
            let avg = cpu_metrics.iter().sum::<f64>() / cpu_metrics.len() as f64;
            let max = cpu_metrics.iter().fold(0.0_f64, |a, &b| a.max(b));
            (Some(avg), Some(max))
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    let (avg_memory, max_memory) = if container_name.is_some() {
        let mem_metrics: Vec<u64> = metrics.iter().filter_map(|m| m.memory_usage).collect();
        if !mem_metrics.is_empty() {
            let avg = mem_metrics.iter().sum::<u64>() / mem_metrics.len() as u64;
            let max = mem_metrics.iter().fold(0, |a, &b| a.max(b));
            (Some(avg), Some(max))
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    let response_times: Vec<Duration> = metrics.iter().map(|m| m.response_time).collect();
    let avg_response_time = response_times.iter().sum::<Duration>() / response_times.len() as u32;
    let min_response_time = response_times
        .iter()
        .min()
        .copied()
        .unwrap_or(Duration::ZERO);
    let max_response_time = response_times
        .iter()
        .max()
        .copied()
        .unwrap_or(Duration::ZERO);

    let total_bytes = *total_bytes.lock().await;
    let test_duration = Duration::from_secs(duration);
    let avg_speed_mbps = (total_bytes as f64 * 8.0) / (test_duration.as_secs_f64() * 1_000_000.0);
    let peak_speed_mbps = *peak_speed.lock().await;

    Ok(BenchmarkResults {
        avg_cpu,
        max_cpu,
        avg_memory,
        max_memory,
        avg_response_time,
        min_response_time,
        max_response_time,
        successful_requests,
        failed_requests,
        total_bytes_transferred: total_bytes,
        avg_speed_mbps,
        peak_speed_mbps,
    })
}

#[cfg(feature = "benchmark")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(args.connect_timeout))
        .timeout(Duration::from_secs(args.request_timeout))
        .pool_idle_timeout(Duration::from_secs(args.duration + 10))
        .build()?;

    // Only connect to Docker if container names are provided
    let docker = if args.proxy1_container.is_some() || args.proxy2_container.is_some() {
        Some(bollard::Docker::connect_with_local_defaults()?)
    } else {
        None
    };

    println!("Starting benchmark comparison...\n");

    // Test first proxy
    let proxy1_results = test_proxy(
        &client,
        docker.as_ref(),
        &args.proxy1_url,
        &args.proxy1_password,
        &args.video_url,
        args.proxy1_container.as_deref(),
        args.concurrent_connections,
        args.duration,
    )
    .await?;

    // Wait between tests
    sleep(Duration::from_secs(5)).await;

    // Test second proxy
    let proxy2_results = test_proxy(
        &client,
        docker.as_ref(),
        &args.proxy2_url,
        &args.proxy2_password,
        &args.video_url,
        args.proxy2_container.as_deref(),
        args.concurrent_connections,
        args.duration,
    )
    .await?;

    // Print results
    println!("\nBenchmark Results:");
    println!("=================");

    println!("\n{}:", args.proxy1_name);
    if args.proxy1_container.is_some() {
        println!(
            "  CPU Usage: {:.2}% avg, {:.2}% max",
            proxy1_results.avg_cpu.unwrap_or(0.0),
            proxy1_results.max_cpu.unwrap_or(0.0)
        );
        println!(
            "  Memory Usage: {} MB avg, {} MB max",
            proxy1_results.avg_memory.unwrap_or(0) / 1024 / 1024,
            proxy1_results.max_memory.unwrap_or(0) / 1024 / 1024
        );
    }
    println!(
        "  Response Time: {:?} avg, {:?} min, {:?} max",
        proxy1_results.avg_response_time,
        proxy1_results.min_response_time,
        proxy1_results.max_response_time
    );
    println!(
        "  Transfer Speed: {:.2} Mbps avg, {:.2} Mbps peak",
        proxy1_results.avg_speed_mbps, proxy1_results.peak_speed_mbps
    );
    println!(
        "  Total Data Transferred: {:.2} MB",
        proxy1_results.total_bytes_transferred as f64 / 1024.0 / 1024.0
    );
    println!(
        "  Requests: {} successful, {} failed",
        proxy1_results.successful_requests, proxy1_results.failed_requests
    );

    println!("\n{}:", args.proxy2_name);
    if args.proxy2_container.is_some() {
        println!(
            "  CPU Usage: {:.2}% avg, {:.2}% max",
            proxy2_results.avg_cpu.unwrap_or(0.0),
            proxy2_results.max_cpu.unwrap_or(0.0)
        );
        println!(
            "  Memory Usage: {} MB avg, {} MB max",
            proxy2_results.avg_memory.unwrap_or(0) / 1024 / 1024,
            proxy2_results.max_memory.unwrap_or(0) / 1024 / 1024
        );
    }
    println!(
        "  Response Time: {:?} avg, {:?} min, {:?} max",
        proxy2_results.avg_response_time,
        proxy2_results.min_response_time,
        proxy2_results.max_response_time
    );

    println!(
        "  Transfer Speed: {:.2} Mbps avg, {:.2} Mbps peak",
        proxy2_results.avg_speed_mbps, proxy2_results.peak_speed_mbps
    );
    println!(
        "  Total Data Transferred: {:.2} MB",
        proxy2_results.total_bytes_transferred as f64 / 1024.0 / 1024.0
    );
    println!(
        "  Requests: {} successful, {} failed",
        proxy2_results.successful_requests, proxy2_results.failed_requests
    );

    // Print performance comparison
    println!(
        "\nPerformance Comparison ({} vs {}):",
        args.proxy2_name, args.proxy1_name
    );
    println!(
        "  Response Time: {:.2}x faster",
        proxy1_results.avg_response_time.as_secs_f64()
            / proxy2_results.avg_response_time.as_secs_f64()
    );
    println!(
        "  Transfer Speed: {:.2}x faster",
        proxy2_results.avg_speed_mbps / proxy1_results.avg_speed_mbps
    );
    println!(
        "  Request Throughput: {:.2}x higher",
        proxy2_results.successful_requests as f64 / proxy1_results.successful_requests as f64
    );

    if args.proxy1_container.is_some() && args.proxy2_container.is_some() {
        println!(
            "  CPU Usage: {:.2}x lower",
            proxy1_results.avg_cpu.unwrap_or(0.0) / proxy2_results.avg_cpu.unwrap_or(1.0)
        );
        println!(
            "  Memory Usage: {:.2}x lower",
            proxy1_results.avg_memory.unwrap_or(0) as f64
                / proxy2_results.avg_memory.unwrap_or(1) as f64
        );
    }

    Ok(())
}
