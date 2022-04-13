use std::{env::var, f64::{INFINITY, NEG_INFINITY}};
use prometheus_http_query::{Client, RangeVector};
use chrono::{Duration, Local, DateTime};
use lazy_static::lazy_static;

lazy_static! {
    static ref CLIENT: Client = Client::try_from(
        var("PROMETHEUS_END_POINT").unwrap_or("http://127.0.0.1:9090".to_string())
    ).unwrap();
    static ref METRICS: [&'static str; 9] = [
        r#"rate(requests_sent_total{serviceName="SERVICE_NAME"}[SAMPLE])"#,
        r#"sum by (serviceName) (rate(requests_received_total{serviceName="SERVICE_NAME"}[SAMPLE]))"#,
        r#"rate(request_duration_seconds_sum{serviceName="SERVICE_NAME"}[SAMPLE])"#,
        r#"avg_over_time(cpu_seconds{serviceName="SERVICE_NAME"}[SAMPLE])"#,
        r#"avg_over_time(memory_usage_bytes{serviceName="SERVICE_NAME"}[SAMPLE])"#,
        r#"rate(acknowledged_bytes_sum{serviceName="SERVICE_NAME"}[SAMPLE])"#,
        r#"rate(transmitted_bytes_sum{serviceName="SERVICE_NAME"}[SAMPLE])"#,
        r#"avg_over_time(backlog{level="1",serviceName="SERVICE_NAME"}[SAMPLE])"#,
        r#"sum by (serviceName) (avg_over_time(backlog{level!="1",serviceName="SERVICE_NAME"}[SAMPLE]))"#
    ];

    static ref SAMPLES: [&'static str; 3] = [
        "1m",
        "5m",
        "15m"
    ];
}


async fn query_prometheus(query: &str, time: DateTime<Local>) -> Result<f64,  Box<dyn std::error::Error>> {
    let v = RangeVector(query.to_string());

    let response = CLIENT.query(v, Some(time.timestamp()), None).await?;

    let value = response.as_instant().ok_or("query was empty")?.get(0).ok_or(format!("metric {} not found", query))?.sample().value();

    Ok(value)
}

fn after_pad(value: f64, add: bool) -> f64 {
    if add {
        return value + (value * 0.25)
    }
    return value - (value * 0.25)
}

pub async fn build_telemetry_matrix(service: &str) -> Result<[[[f64; 3]; 9]; 10],  Box<dyn std::error::Error>> {
    let mut data: [[[f64; 3]; 9]; 10] = [[[0.0; 3]; 9]; 10];

    let time_steps: [DateTime<Local>; 10] = [
        Local::now(),
        Local::now() - Duration::minutes(1),
        Local::now() - Duration::minutes(2),
        Local::now() - Duration::minutes(4),
        Local::now() - Duration::minutes(8),
        Local::now() - Duration::minutes(16),
        Local::now() - Duration::minutes(32),
        Local::now() - Duration::minutes(64),
        Local::now() - Duration::minutes(128),
        Local::now() - Duration::minutes(256),
    ];

    let mut low: [[f64; 3]; 9] = [[INFINITY; 3]; 9];
    let mut high: [[f64; 3]; 9] = [[NEG_INFINITY; 3]; 9];

    for (x, time_step) in time_steps.iter().enumerate() {
        for (y, metric) in METRICS.iter().enumerate() {
            for (z, sample) in SAMPLES.iter().enumerate(){
                let query = &metric.replace("SERVICE_NAME", service).replace("SAMPLE", sample);
                match query_prometheus(query, *time_step).await {
                    Ok(value) => {
                        data[x][y][z] = value;
                        if after_pad(value, true) > high[y][z] {
                            high[y][z] = after_pad(value, true)
                        }if after_pad(value, false) < low[y][z] {
                            low[y][z] = after_pad(value, false)
                        }
                    },
                    Err(e) => return Err(e),
                }
            }
        }
    }

    for (_, timestep) in data.iter_mut().enumerate() {
        for (y, metric) in timestep.iter_mut().enumerate() {
            for (z, peroid) in metric.iter_mut().enumerate(){

                let max = high[y][z];
                let min = low[y][z];

                let mut lower_bound = max - min;

                if lower_bound <= 0.0{
                    lower_bound = 1.0;
                }

                let value =  (*peroid - min) / lower_bound;

                *peroid = value
            }
        }
    }

    Ok(data)
}