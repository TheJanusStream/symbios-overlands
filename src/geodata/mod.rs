//! Fetching GDI Berlin's geodata when a geodata region is visited (#1582,
//! epic #1580; design in `docs/geodata.md`).
//!
//! The pure half - where a region's square lies, which URLs to ask, how to
//! decode the answers - is the `geodata` crate. This module is the I/O half:
//! it gets the bytes, and keeps them, because the services answer every
//! request with `Cache-Control: no-store`.
//!
//! - [`GeoRequest`] (`request`): one request - its canonical URL, which is
//!   also its cache key, its byte cap, and how to tell the answer asked for
//!   from an error page before anything is kept;
//! - [`GeoStore`] (`store`): where answers are kept between visits - a
//!   directory under the platform cache natively, the browser's Cache API
//!   on the web (`store_browser`) - bounded by a time-to-live, an epoch, and
//!   natively a size cap;
//! - [`fetch_once`] (`fetch`): one attempt - from the store if it can, else
//!   from the network, kept only if it validates;
//! - [`GeoFetcher`] (`fetcher`): the resource the rest of the app asks -
//!   requests in, answers out by id, identical requests merged, at most
//!   [`MAX_IN_FLIGHT`] running, transient failures retried after a backoff,
//!   and [`GeoProgress`] counts for a loading screen.
//!
//! Only `https://gdi.berlin.de/services/` is ever asked: any other request
//! fails as [`GeoFetchError::Refused`] before the store or the network sees
//! it.

mod fetch;
mod fetcher;
mod request;
mod store;
#[cfg(target_arch = "wasm32")]
mod store_browser;

use std::sync::Arc;

use bevy::prelude::*;

use crate::world_builder::asset_failure::AssetFetchError;

pub use fetch::{Fetched, GetResult, Source, fetch_once};
pub use fetcher::{
    GeoFetcher, GeoProgress, GeoRequestId, GeoTransport, GetFuture, HttpTransport, MAX_ATTEMPTS,
    MAX_IN_FLIGHT, RETRY_BACKOFF_SECS,
};
pub use request::{
    FEATURES_CAP, GeoKind, GeoRequest, HITS_CAP, LEGEND_CAP, RENDER_CAP, is_gdi_services_url,
};
pub use store::{CACHE_EPOCH, GeoStore, TTL_SECS};

/// Why a request has no answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeoFetchError {
    /// A request this client does not send: a URL outside GDI Berlin's
    /// services, or a render the decoders would refuse. A bug, never the
    /// network's doing.
    Refused,
    /// The fetch failed: unreachable, an error status, a body over the cap.
    Fetch(AssetFetchError),
    /// A success status with a body that is not the answer asked for -
    /// GeoServer's XML exception, a render of the wrong size.
    BadResponse,
    /// The answer came from somewhere other than GDI Berlin's services: the
    /// request was redirected off them. Never kept.
    Redirected,
}

impl GeoFetchError {
    /// Whether another attempt might succeed: a fetch failure the network
    /// could heal. A refused request or a wrong answer would only repeat.
    pub fn retryable(&self) -> bool {
        matches!(self, GeoFetchError::Fetch(error) if !error.permanent())
    }
}

impl std::fmt::Display for GeoFetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeoFetchError::Refused => write!(f, "request refused before sending"),
            GeoFetchError::Fetch(error) => write!(f, "fetch failed: {error:?}"),
            GeoFetchError::BadResponse => write!(f, "the answer was not the one asked for"),
            GeoFetchError::Redirected => write!(f, "redirected off GDI Berlin's services"),
        }
    }
}

impl std::error::Error for GeoFetchError {}

/// Installs the [`GeoFetcher`] with this platform's store and the real
/// network, and drives it every frame.
pub struct GeodataPlugin;

impl Plugin for GeodataPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GeoFetcher::new(
            GeoStore::platform_default(),
            Arc::new(HttpTransport),
        ))
        .add_systems(Update, drive_geo_fetches);
    }
}

/// Settle finished fetches and start waiting ones, on real time so a paused
/// game does not pause a retry. A frame with nothing to settle or start -
/// idle, waiting on the network, or waiting out a backoff - leaves the
/// resource untouched: [`GeoFetcher::needs_drive`] reads through `Deref`,
/// and only real work takes it mutably.
pub fn drive_geo_fetches(mut fetcher: ResMut<GeoFetcher>, time: Res<Time<Real>>) {
    let now = time.elapsed_secs_f64();
    if fetcher.needs_drive(now) {
        fetcher.drive(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_healable_fetch_failures_are_retried() {
        assert!(GeoFetchError::Fetch(AssetFetchError::Unreachable).retryable());
        assert!(GeoFetchError::Fetch(AssetFetchError::HttpStatus(503)).retryable());
        assert!(GeoFetchError::Fetch(AssetFetchError::HttpStatus(429)).retryable());
        assert!(GeoFetchError::Fetch(AssetFetchError::TimedOut).retryable());
        assert!(!GeoFetchError::Fetch(AssetFetchError::HttpStatus(404)).retryable());
        assert!(!GeoFetchError::Fetch(AssetFetchError::TooLarge { limit: 1 }).retryable());
        assert!(!GeoFetchError::Refused.retryable());
        assert!(!GeoFetchError::BadResponse.retryable());
        assert!(!GeoFetchError::Redirected.retryable());
    }

    #[test]
    fn an_idle_fetcher_is_not_marked_changed_by_the_frame_system() {
        let mut app = App::new();
        app.init_resource::<Time<Real>>()
            .insert_resource(GeoFetcher::new(GeoStore::Off, Arc::new(HttpTransport)))
            .add_systems(Update, drive_geo_fetches);
        app.update();
        let tick = app.world().resource_ref::<GeoFetcher>().last_changed();
        app.update();
        app.update();
        assert_eq!(
            app.world().resource_ref::<GeoFetcher>().last_changed(),
            tick
        );
    }
}
