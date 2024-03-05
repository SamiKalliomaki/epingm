use clap::{Parser, ValueEnum};
use std::net::ToSocketAddrs;
use std::{
    io,
    net::IpAddr,
    thread,
    time::{Duration, Instant},
};
use textplots::{Chart, Plot, Shape, LabelBuilder, LabelFormat};
use volley::{measure_volley, VolleyResult};

use crate::volley::PingResult;

mod volley;

#[derive(Clone, Debug, ValueEnum)]
enum Format {
    Text,
    Csv,
}

#[derive(Parser, Debug)]
struct ProgramArgs {
    /// Number of pings to send per volley
    #[arg(short, long, default_value = "1000")]
    count: usize,

    /// Seconds between each ping in a volley.
    #[arg(short, long, default_value = "0.01")]
    interval: f32,

    /// Payload size in bytes.
    #[arg(short, long, default_value = "64")]
    size: usize,

    /// Maximum number of seconds to wait for a reply.
    #[arg(long, default_value = "1")]
    timeout: f32,

    /// Seconds between each volley.
    #[arg(long, default_value = "0")]
    volley_interval: f32,

    /// Output format
    #[arg(short, long, default_value = "text")]
    format: Format,

    /// Targets to ping
    #[arg(required = true)]
    target: Vec<String>,

    /// Display a graph of the ping results.
    #[arg(long)]
    graph: bool,

    /// Graph width.
    #[arg(long, default_value = "300")]
    graph_width: u32,

    /// Graph height.
    #[arg(long, default_value = "100")]
    graph_height: u32,

    /// Graph maximum latency.
    #[arg(long, default_value = "0.1")]
    graph_max_latency: f32,
}

fn secs_to_duration(secs: f32) -> Duration {
    Duration::from_nanos((secs * 1e9) as u64)
}

fn resolve(target: &str) -> io::Result<IpAddr> {
    match (target.to_string() + ":0").to_socket_addrs() {
        Err(e) => Err(io::Error::new(
            e.kind(),
            format!("Failed to resolve {}: {}", target, e),
        )),
        Ok(mut addrs) => match addrs.next() {
            None => Err(io::Error::new(
                io::ErrorKind::NotFound,
                "No addresses found",
            )),
            Some(addr) => Ok(addr.ip()),
        },
    }
}

fn run(args: ProgramArgs) {
    let count = args.count;
    let interval = secs_to_duration(args.interval);
    let timeout = secs_to_duration(args.timeout);
    let volley_interval = secs_to_duration(args.volley_interval);
    let targets = args.target;
    let format = args.format;

    for target in &targets {
        match resolve(target) {
            Err(e) => {
                eprintln!("{}", e);
                return;
            }
            Ok(_) => {}
        };
    }

    match format {
        Format::Text => {}
        Format::Csv => {
            println!("time,target,ip,received,sent,lost,avg,min,max,50th,99th,missing");
        }
    }

    let mut next_volley = Instant::now();
    loop {
        for target in &targets {
            let addr = match resolve(target) {
                Err(e) => {
                    eprintln!("{}", e);
                    continue;
                }
                Ok(addr) => addr,
            };

            let start = chrono::Local::now();
            let info = match measure_volley(addr, count, args.size, interval, timeout) {
                VolleyResult::Error(e) => {
                    eprintln!("Failed to measure volley: {}", e);
                    continue;
                }
                VolleyResult::Success(info) => info,
            };

            let mut sum = Duration::ZERO;
            let mut latencies: Vec<u64> = Vec::new();
            let mut missing: Vec<usize> = Vec::new();

            for (i, result) in info.results.iter().enumerate() {
                match result {
                    None => {
                        missing.push(i);
                    }
                    Some(PingResult {
                        latency,
                        reply_size: _,
                    }) => {
                        latencies.push(latency.as_millis() as u64);
                        sum += latency.clone();
                    }
                }
            }

            let timeout_millis = timeout.as_millis() as u64;
            let avg;
            if info.received > 0 {
                avg = (sum / info.received as u32).as_millis() as u64;
            } else {
                avg = timeout_millis;
            }

            latencies.sort();

            let min;
            let max;
            let percentile50;
            let percentile99;
            if latencies.len() > 0 {
                min = latencies.first().unwrap().clone();
                max = latencies.last().unwrap().clone();
                percentile50 = latencies[(latencies.len() as f64 * 0.50) as usize];
                percentile99 = latencies[(latencies.len() as f64 * 0.99) as usize];
            } else {
                min = timeout_millis;
                max = timeout_millis;
                percentile50 = timeout_millis;
                percentile99 = timeout_millis;
            }

            let lost = count - info.received;

            match format {
                Format::Text => {
                    println!(
                        "[{}] {} ({}): received: {}/{}, lost: {}, avg: {} ms, min: {} ms, max: {} ms, 50th: {} ms, 99th: {} ms, missing: {:?}",
                        start.format("%Y-%m-%d %H:%M:%S"),
                        target,
                        addr,
                        info.received,
                        info.sent,
                        lost,
                        avg,
                        min,
                        max,
                        percentile50,
                        percentile99,
                        missing
                    );
                }
                Format::Csv => {
                    println!(
                        "{},{},{},{},{},{},{},{},{},{},{},{:?}",
                        start.format("%Y-%m-%d %H:%M:%S"),
                        target,
                        addr,
                        info.received,
                        info.sent,
                        lost,
                        avg,
                        min,
                        max,
                        percentile50,
                        percentile99,
                        missing
                    );
                }
            }

            if args.graph {
                let mut values: Vec<(f32, f32)> = Vec::new();
                for (i, result) in info.results.iter().enumerate() {
                    match result {
                        None => {}
                        Some(PingResult {
                            latency,
                            reply_size: _,
                        }) => {
                            values.push((i as f32, latency.as_nanos() as f32 / 1e6));
                        }
                    }
                }

                Chart::new_with_y_range(
                    args.graph_width,
                    args.graph_height,
                    0.0,
                    (count - 1) as f32,
                    0.0,
                    args.graph_max_latency * 1000.0,
                )
                .lineplot(&Shape::Points(&values))
                .x_label_format(LabelFormat::None)
                .display();
            }
        }

        next_volley += volley_interval;
        if next_volley > Instant::now() {
            let sleep_duration = next_volley - Instant::now();
            thread::sleep(sleep_duration);
        }
    }
}

fn main() {
    let args = ProgramArgs::parse();
    run(args);
}
