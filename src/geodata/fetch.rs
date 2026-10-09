//! One attempt at a request: from the store if it holds a fresh answer, else
//! from the network - and kept only if it is the answer asked for, from the
//! host it was asked of. Or past the store, when what it holds is known to
//! be older than the answer a record was saved with (#1590).

use std::future::Future;
use std::sync::Arc;

use crate::world_builder::asset_failure::AssetFetchError;

use super::{GeoFetchError, GeoRequest, GeoStore, is_gdi_services_url};

/// Where an answer came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Kept from an earlier visit, `at` this time (Unix seconds).
    Store { at: i64 },
    /// Fetched just now.
    Network,
}

/// An answer and where it came from, or why there is none.
pub type Fetched = Result<(Arc<[u8]>, Source), GeoFetchError>;

/// What the network half of an attempt answers: the body, and the URL it
/// finally came from after any redirects.
pub type GetResult = Result<(Vec<u8>, String), AssetFetchError>;

/// One attempt at `request`, at `now` (Unix seconds): the store's answer if
/// it has a fresh valid one, else `get(url, cap)`'s - from GDI Berlin's
/// services, within the cap, and validated - which is then kept.
///
/// A request this client will not send fails before the store or the
/// network sees it. A stored answer is validated again before it is used,
/// in case the store was edited outside the app.
pub async fn fetch_once<F, Fut>(request: &GeoRequest, store: &GeoStore, now: i64, get: F) -> Fetched
where
    F: FnOnce(String, usize) -> Fut,
    Fut: Future<Output = GetResult>,
{
    if !request.is_allowed() {
        return Err(GeoFetchError::Refused);
    }
    if let Some((body, at)) = store.get_stamped(request.url(), request.cap(), now).await
        && request.validate(&body).is_ok()
    {
        return Ok((body.into(), Source::Store { at }));
    }
    fetch_network(request, store, now, get).await
}

/// One attempt at `request` past the store, at `now`: `get(url, cap)`'s
/// answer, validated as [`fetch_once`] validates one, then kept in place of
/// whatever the store held. For an answer whose stored copy is known to be
/// stale.
pub async fn fetch_fresh<F, Fut>(
    request: &GeoRequest,
    store: &GeoStore,
    now: i64,
    get: F,
) -> Fetched
where
    F: FnOnce(String, usize) -> Fut,
    Fut: Future<Output = GetResult>,
{
    if !request.is_allowed() {
        return Err(GeoFetchError::Refused);
    }
    fetch_network(request, store, now, get).await
}

/// The network half of an attempt at an allowed `request`.
async fn fetch_network<F, Fut>(request: &GeoRequest, store: &GeoStore, now: i64, get: F) -> Fetched
where
    F: FnOnce(String, usize) -> Fut,
    Fut: Future<Output = GetResult>,
{
    let cap = request.cap();
    let (body, answered_from) = get(request.url().to_owned(), cap)
        .await
        .map_err(GeoFetchError::Fetch)?;
    if !is_gdi_services_url(&answered_from) {
        return Err(GeoFetchError::Redirected);
    }
    if body.len() > cap {
        return Err(GeoFetchError::Fetch(AssetFetchError::TooLarge {
            limit: cap,
        }));
    }
    request.validate(&body)?;
    store.put(request.url(), &body, now).await;
    Ok((body.into(), Source::Network))
}

#[cfg(test)]
mod tests {
    use super::*;
    use geodata::berlin;
    use geodata::request::Bbox;
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::sync::Mutex;

    const NOW: i64 = 1_800_000_000;
    const LEGEND: &[u8] = br#"{"Legend":[{"layerName":"c_dgm1","rules":[]}]}"#;
    const DOM: Bbox = Bbox {
        min_e: 391_200,
        min_n: 5_819_700,
        max_e: 391_800,
        max_n: 5_820_300,
    };

    fn memory() -> GeoStore {
        GeoStore::Memory(Arc::new(Mutex::new(HashMap::new())))
    }

    fn block<F: Future>(f: F) -> F::Output {
        futures_lite::future::block_on(f)
    }

    /// A transport answering `body` from the URL it was asked.
    fn answer(body: &'static [u8]) -> impl FnOnce(String, usize) -> std::future::Ready<GetResult> {
        move |url, _| std::future::ready(Ok((body.to_vec(), url)))
    }

    #[test]
    fn a_miss_fetches_validates_and_keeps_then_the_store_answers() {
        let store = memory();
        let request = GeoRequest::legend(&berlin::TERRAIN, 0);
        let calls = Cell::new(0);
        let get = |url: String, cap: usize| {
            calls.set(calls.get() + 1);
            assert_eq!((url.as_str(), cap), (request.url(), request.cap()));
            std::future::ready(Ok((LEGEND.to_vec(), url)))
        };
        let (body, source) = block(fetch_once(&request, &store, NOW, get)).unwrap();
        assert_eq!((&*body, source), (LEGEND, Source::Network));
        let (body, source) = block(fetch_once(&request, &store, NOW + 60, |_, _| async {
            panic!("a fresh stored answer must not reach the network")
        }))
        .unwrap();
        assert_eq!((&*body, source), (LEGEND, Source::Store { at: NOW }));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn an_error_page_is_neither_returned_nor_kept() {
        let store = memory();
        let request = GeoRequest::legend(&berlin::TERRAIN, 0);
        assert_eq!(
            block(fetch_once(
                &request,
                &store,
                NOW,
                answer(b"<ServiceExceptionReport/>")
            )),
            Err(GeoFetchError::BadResponse)
        );
        assert_eq!(block(store.get(request.url(), request.cap(), NOW)), None);
    }

    #[test]
    fn an_answer_redirected_off_the_service_is_neither_returned_nor_kept() {
        let store = memory();
        let request = GeoRequest::legend(&berlin::TERRAIN, 0);
        let elsewhere = |_: String, _: usize| {
            std::future::ready(Ok((
                LEGEND.to_vec(),
                "https://example.org/legend.json".to_owned(),
            )))
        };
        assert_eq!(
            block(fetch_once(&request, &store, NOW, elsewhere)),
            Err(GeoFetchError::Redirected)
        );
        assert_eq!(block(store.get(request.url(), request.cap(), NOW)), None);
        // A redirect that stays on the service is an answer like any other.
        let moved = |_: String, _: usize| {
            let url = "https://gdi.berlin.de/services/wms/dgm1_v2?request=GetLegendGraphic";
            std::future::ready(Ok((LEGEND.to_vec(), url.to_owned())))
        };
        assert!(block(fetch_once(&request, &store, NOW, moved)).is_ok());
    }

    #[test]
    fn a_stored_answer_that_no_longer_validates_is_fetched_again() {
        let store = memory();
        let request = GeoRequest::legend(&berlin::TERRAIN, 0);
        block(store.put(request.url(), b"tampered", NOW));
        let (body, source) = block(fetch_once(&request, &store, NOW, answer(LEGEND))).unwrap();
        assert_eq!((&*body, source), (LEGEND, Source::Network));
    }

    #[test]
    fn network_failures_pass_through_and_keep_nothing() {
        let store = memory();
        let request = GeoRequest::legend(&berlin::TERRAIN, 0);
        let failed = block(fetch_once(&request, &store, NOW, |_, _| {
            std::future::ready(Err(AssetFetchError::HttpStatus(503)))
        }));
        assert_eq!(
            failed,
            Err(GeoFetchError::Fetch(AssetFetchError::HttpStatus(503)))
        );
        assert_eq!(block(store.get(request.url(), request.cap(), NOW)), None);
    }

    #[test]
    fn an_answer_past_the_cap_is_refused_even_if_the_transport_let_it_through() {
        let store = memory();
        let request = GeoRequest::hits(&berlin::BUILDINGS, DOM);
        let mut body = br#"<x numberMatched="1"/>"#.to_vec();
        body.resize(request.cap() + 1, b' ');
        let result = block(fetch_once(&request, &store, NOW, move |url, _| {
            std::future::ready(Ok((body, url)))
        }));
        assert_eq!(
            result,
            Err(GeoFetchError::Fetch(AssetFetchError::TooLarge {
                limit: request.cap()
            }))
        );
    }

    #[test]
    fn a_fresh_attempt_goes_past_the_store_and_keeps_what_it_got() {
        let store = memory();
        let request = GeoRequest::legend(&berlin::TERRAIN, 0);
        let older = br#"{"Legend":[{"layerName":"c_dgm1","rules":[{"name":"old"}]}]}"#;
        block(store.put(request.url(), older, NOW));
        let (body, source) = block(fetch_fresh(&request, &store, NOW, answer(LEGEND))).unwrap();
        assert_eq!((&*body, source), (LEGEND, Source::Network));
        let (body, source) = block(fetch_once(&request, &store, NOW, |_, _| async {
            panic!("the fresh answer was kept")
        }))
        .unwrap();
        assert_eq!((&*body, source), (LEGEND, Source::Store { at: NOW }));
        // Refused as a plain attempt is.
        let refused = GeoRequest::render(&berlin::TERRAIN, DOM, 4096, 4096);
        assert_eq!(
            block(fetch_fresh(&refused, &store, NOW, answer(LEGEND))),
            Err(GeoFetchError::Refused)
        );
    }

    #[test]
    fn a_refused_request_touches_neither_store_nor_network() {
        let store = memory();
        let request = GeoRequest::render(&berlin::TERRAIN, DOM, 4096, 4096);
        let result = block(fetch_once(&request, &store, NOW, |_, _| async {
            panic!("a refused request must not reach the network")
        }));
        assert_eq!(result, Err(GeoFetchError::Refused));
    }
}
