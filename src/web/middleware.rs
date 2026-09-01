use axum::{
    extract::Request,
    http::{HeaderValue, Method, header},
    middleware::Next,
    response::Response,
};
use std::net::Ipv4Addr;

use super::error::WebError;

const HARDENING_HEADERS: [(&str, &str); 3] = [
    (
        "content-security-policy",
        "default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'; object-src 'none'",
    ),
    ("x-content-type-options", "nosniff"),
    ("referrer-policy", "same-origin"),
];

fn with_hardening_headers(mut response: Response) -> Response {
    for (name, value) in HARDENING_HEADERS {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    response
}
pub(crate) async fn reject_cross_site_requests(
    request: Request,
    next: Next,
) -> Result<Response, WebError> {
    if !host_is_loopback(
        request
            .headers()
            .get(header::HOST)
            .and_then(|host| host.to_str().ok()),
    ) {
        return Err(loopback_forbidden());
    }

    if matches!(
        request.method(),
        &Method::GET | &Method::HEAD | &Method::OPTIONS | &Method::TRACE
    ) {
        return Ok(with_hardening_headers(next.run(request).await));
    }

    if let Some(origin) = request.headers().get(header::ORIGIN) {
        let Some(origin) = origin.to_str().ok() else {
            return Err(cross_site_forbidden());
        };
        let Some(host) = request
            .headers()
            .get(header::HOST)
            .and_then(|host| host.to_str().ok())
        else {
            return Err(cross_site_forbidden());
        };
        if !origin_matches_host(host, origin) {
            return Err(cross_site_forbidden());
        }
    }

    if let Some(site) = request.headers().get("sec-fetch-site") {
        let Some(site) = site.to_str().ok() else {
            return Err(cross_site_forbidden());
        };
        if site != "same-origin" && site != "none" {
            return Err(cross_site_forbidden());
        }
    }

    Ok(with_hardening_headers(next.run(request).await))
}

fn cross_site_forbidden() -> WebError {
    WebError::forbidden("Cross-site requests are not allowed against this local Web UI.")
}

fn loopback_forbidden() -> WebError {
    WebError::forbidden("The local Web UI only accepts requests addressed to a loopback host.")
}

/// The Web UI is served over plain HTTP on a loopback address, so every
/// legitimate request arrives with a loopback `Host` header (`127.0.0.0/8`,
/// `localhost`, or `[::1]`, with an optional port). Requiring that header
/// defeats DNS rebinding, where a domain resolving to 127.0.0.1 would
/// otherwise present a matching, same-origin `Origin` for POSTs and readable
/// GET responses.
pub(crate) fn host_is_loopback(host: Option<&str>) -> bool {
    let Some(host) = host else {
        return false;
    };
    let host = host.trim();
    let address = if let Some(rest) = host.strip_prefix('[') {
        rest.split_once(']').map_or(host, |(address, _)| address)
    } else if host.matches(':').count() > 1 {
        host
    } else {
        host.rsplit_once(':').map_or(host, |(address, _)| address)
    };
    if address.eq_ignore_ascii_case("localhost") || address == "::1" {
        return true;
    }
    address.parse::<Ipv4Addr>().is_ok_and(|ip| ip.is_loopback())
}

/// The Web UI is served over plain HTTP on a loopback address, so a
/// same-origin browser request sends `Origin: http://<host>` where `<host>`
/// exactly matches the `Host` header.
fn origin_matches_host(host: &str, origin: &str) -> bool {
    origin
        .strip_prefix("http://")
        .is_some_and(|origin_host| origin_host == host)
}
