// Copyright 2016 TiKV Project Authors. Licensed under Apache-2.0.

use std::io::Write;

use prometheus::{proto::MetricType, Encoder, TextEncoder, TEXT_FORMAT};
use slog::error;

pub mod process_linux;
pub mod threads_linux;
use hyper::{
    header::CONTENT_TYPE,
    http::HeaderValue,
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server, StatusCode,
};
pub fn initial_metric<S: Into<String>>(log: slog::Logger, namespace: S) {
    process_linux::monitor_process()
        .unwrap_or_else(|e| error!(log, "failed to start process monitor: {}", e));
    threads_linux::monitor_threads(namespace)
        .unwrap_or_else(|e| error!(log, "failed to start thread monitor: {}", e));
}

pub fn dump(should_simplify: bool) -> String {
    let mut buffer = vec![];
    dump_to(&mut buffer, should_simplify);
    String::from_utf8(buffer).unwrap()
}

pub fn dump_to(w: &mut impl Write, should_simplify: bool) {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    if !should_simplify {
        if let Err(e) = encoder.encode(&metric_families, w) {
            eprintln!("prometheus encoding error. error: {}", e)
        }
        return;
    }

    // filter out metrics that has no sample values
    for mut mf in metric_families {
        let mut metrics = mf.take_metric().into_vec();
        match mf.get_field_type() {
            MetricType::COUNTER => {
                metrics.retain(|m| m.get_counter().get_value() > 0.0);
            }
            MetricType::HISTOGRAM => metrics.retain(|m| m.get_histogram().get_sample_count() > 0),
            _ => {}
        }
        if !metrics.is_empty() {
            mf.set_metric(metrics.into());
            if let Err(e) = encoder.encode(&[mf], w) {
                eprintln!("prometheus encoding error. error: {}", e);
            }
        }
    }
}

pub async fn http_serve() {
    let addr = ([127, 0, 0, 1], 9898).into();
    println!("Listening on http://{}", addr);

    let serve_future = Server::bind(&addr).serve(make_service_fn(|_| async {
        Ok::<_, hyper::Error>(service_fn(http_serve_req))
    }));

    if let Err(err) = serve_future.await {
        eprintln!("server error: {}", err);
    }
}

async fn http_serve_req(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    let path = req.uri().path().to_owned();
    let method = req.method().to_owned();
    match (method, path.as_ref()) {
        (Method::GET, "/metrics") => {
            let metrics = dump(false).into_bytes();
            let mut resp = Response::new(metrics.into());
            resp.headers_mut()
                .insert(CONTENT_TYPE, HeaderValue::from_static(TEXT_FORMAT));
            Ok(resp)
        }
        _ => {
            let response = Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("path not found"))
                .unwrap();
            Ok(response)
        }
    }
}
