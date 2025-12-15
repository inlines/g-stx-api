use actix_web::{get, HttpResponse};
use prometheus::{register_counter, register_gauge, register_histogram, Counter, IntGauge, Histogram, Encoder, TextEncoder};

lazy_static::lazy_static! {
    pub static ref HTTP_REQUESTS_TOTAL: Counter = register_counter!(
        "http_requests_total",
        "Total HTTP requests"
    ).unwrap();
    
    pub static ref HTTP_REQUESTS_DURATION: Histogram = register_histogram!(
        "http_request_duration_seconds",
        "HTTP request duration in seconds",
        vec![0.05, 0.1, 0.5, 1.0, 2.0, 5.0]
    ).unwrap();
    
    pub static ref WS_CONNECTIONS: IntGauge = register_int_gauge!(
        "ws_connections",
        "Active WebSocket connections"
    ).unwrap();
    
    pub static ref CHAT_MESSAGES_SENT: Counter = register_counter!(
        "chat_messages_sent_total",
        "Total chat messages sent"
    ).unwrap();
    
    pub static ref DB_POOL_CONNECTIONS: IntGauge = register_int_gauge!(
        "db_pool_connections",
        "Active DB connections in pool"
    ).unwrap();
}

#[get("/metrics")]
pub async fn metrics_endpoint() -> HttpResponse {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    
    if let Err(e) = encoder.encode(&metric_families, &mut buffer) {
        eprintln!("Failed to encode metrics: {}", e);
        return HttpResponse::InternalServerError().finish();
    }
    
    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4; charset=utf-8")
        .body(buffer)
}