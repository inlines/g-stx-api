use actix_web::{HttpResponse, get};
use prometheus::{
    Counter, CounterVec, Encoder, HistogramVec, IntGauge, TextEncoder, register_counter,
};

lazy_static::lazy_static! {
    pub static ref HTTP_REQUESTS_TOTAL: CounterVec = register_counter_vec!(
        "http_requests_total",
        "Total HTTP requests",
        &["method", "endpoint", "status"]
    ).unwrap();

    pub static ref HTTP_REQUESTS_DURATION: HistogramVec = register_histogram_vec!(
        "http_request_duration_seconds",
        "HTTP request duration in seconds",
        &["method", "endpoint"],
        vec![0.05, 0.1, 0.5, 1.0, 2.0, 5.0]
    ).unwrap();

    pub static ref WS_CONNECTIONS: IntGauge = register_int_gauge!(
        "ws_connections",
        "Unique logins with active WebSocket sessions in this backend process"
    ).unwrap();

    pub static ref CHAT_MESSAGES_SENT: Counter = register_counter!(
        "chat_messages_sent_total",
        "Chat messages successfully persisted to the database"
    ).unwrap();

    pub static ref FAILED_LOGIN_ATTEMPTS: CounterVec = register_counter_vec!(
        "failed_login_attempts_total",
        "Failed login requests by bounded reason",
        &["reason"]
    ).unwrap();

    pub static ref CHAT_PERSISTENCE_ERRORS: Counter = register_counter!(
        "chat_message_persistence_errors_total",
        "Chat messages that could not be saved"
    ).unwrap();

    // Kept as a standalone counter for existing dashboards.
    pub static ref SUCCESSFUL_LOGINS: Counter = register_counter!(
        "successful_logins_total",
        "Total successful logins"
    ).unwrap();

    pub static ref SUCCESSFUL_ADD_TO_COLLECTION: Counter = register_counter!(
        "successful_add_to_collection_total",
        "New collection rows inserted"
    ).unwrap();

    pub static ref WTS_SAVES: Counter = register_counter!(
        "wts_saves_total",
        "Successful WTS saves, including edits"
    ).unwrap();

    pub static ref SUCCESSFUL_ADD_TO_WISHLIST: Counter = register_counter!(
        "successful_add_to_wishlist_total",
        "New wishlist rows inserted"
    ).unwrap();

    pub static ref SUCCESSFUL_REGISTRATIONS: Counter = register_counter!(
        "successful_registrations_total",
        "Total successful registrations"
    ).unwrap();


}

/// Register finite business series before the first event / scrape.
pub fn initialize() {
    for counter in [
        &*CHAT_MESSAGES_SENT,
        &*CHAT_PERSISTENCE_ERRORS,
        &*SUCCESSFUL_LOGINS,
        &*SUCCESSFUL_ADD_TO_COLLECTION,
        &*WTS_SAVES,
        &*SUCCESSFUL_ADD_TO_WISHLIST,
        &*SUCCESSFUL_REGISTRATIONS,
    ] {
        let _ = counter.get();
    }
    let _ = WS_CONNECTIONS.get();
    for reason in ["invalid_password", "user_not_found", "database_error"] {
        let _ = FAILED_LOGIN_ATTEMPTS.with_label_values(&[reason]);
    }
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
