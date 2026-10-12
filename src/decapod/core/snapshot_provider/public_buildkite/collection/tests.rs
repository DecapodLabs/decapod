use super::*;
use std::sync::{
    Barrier,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Default)]
struct Reader {
    calls: usize,
    fail: bool,
    bytes: Vec<u8>,
    batch_sizes: Vec<usize>,
}
impl PublicReader for Reader {
    fn get(&mut self, _: &PublicRequest, _: &Budget<'_>) -> Result<Vec<u8>> {
        self.calls += 1;
        if self.fail {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"))
        } else {
            Ok(self.bytes.clone())
        }
    }
    fn get_many(
        &mut self,
        requests: &[PublicRequest],
        budget: &Budget<'_>,
    ) -> Result<Vec<Vec<u8>>> {
        self.batch_sizes.push(requests.len());
        requests
            .iter()
            .map(|request| self.get(request, budget))
            .collect()
    }
}
fn request(index: usize) -> PublicRequest {
    PublicRequest {
        path: format!(
            "/organizations/org/pipelines/pipe/builds/7/jobs/44444444-4444-4444-4444-444444444444/artifacts/{index:08x}-7777-7777-7777-777777777777"
        ),
        artifact_redirect: true,
        log_redirect: None,
    }
}
fn budget(cancel: &Cancellation) -> Budget<'_> {
    Budget::new(Duration::from_secs(10), 2 * 1024 * 1024, cancel).unwrap()
}
#[test]
fn exact_paths_deduplicate_within_one_collection_only() {
    let cancel = Cancellation::default();
    let budget = budget(&cancel);
    let mut raw = Reader {
        bytes: b"payload".to_vec(),
        ..Default::default()
    };
    {
        let mut cache = CollectionReader::new(&mut raw);
        cache
            .prefetch(vec![request(1), request(1), request(2)], &budget)
            .unwrap();
        cache.get(&request(1), &budget).unwrap();
        cache.get(&request(2), &budget).unwrap();
        assert_eq!(cache.reader.calls, 2);
        assert_eq!(cache.reader.batch_sizes, [2]);
        assert_eq!(cache.measurements()["public_get_requests"], 2);
        assert_eq!(cache.measurements()["cache_hits"], 2);
    }
    CollectionReader::new(&mut raw)
        .get(&request(1), &budget)
        .unwrap();
    assert_eq!(raw.calls, 3, "no cache survives collection lifetime");
}
#[test]
fn failure_never_enters_cache_and_cached_reads_still_obey_deadline_cancel_and_limit() {
    let cancel = Cancellation::default();
    let normal = budget(&cancel);
    let mut raw = Reader {
        fail: true,
        bytes: vec![0; 32],
        ..Default::default()
    };
    let mut cache = CollectionReader::new(&mut raw);
    assert!(cache.get(&request(1), &normal).is_err());
    assert!(cache.cache.is_empty());
    cache.reader.fail = false;
    cache.get(&request(1), &normal).unwrap();
    let small = Budget::new(Duration::from_secs(10), 1, &cancel).unwrap();
    assert!(cache.get(&request(1), &small).is_err());
    let expired = Budget {
        deadline: Instant::now(),
        cancel: &cancel,
        max_bytes: normal.max_bytes,
    };
    assert_eq!(
        cache.get(&request(1), &expired).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    cancel.cancel();
    assert_eq!(
        cache.get(&request(1), &normal).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(cache.reader.calls, 2);
}
#[test]
fn failed_prefetch_retains_no_partial_batch_and_never_retries_denial() {
    let cancel = Cancellation::default();
    let budget = budget(&cancel);
    let mut raw = Reader {
        fail: true,
        ..Default::default()
    };
    let mut cache = CollectionReader::new(&mut raw);
    let error = cache
        .prefetch(vec![request(1), request(2)], &budget)
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert!(cache.cache.is_empty());
    assert_eq!(cache.reader.calls, 1);
}
#[test]
fn cancellation_after_batch_download_commits_no_cached_responses() {
    struct CancelAfterRead<'a>(&'a Cancellation);
    impl PublicReader for CancelAfterRead<'_> {
        fn get(&mut self, _: &PublicRequest, _: &Budget<'_>) -> Result<Vec<u8>> {
            unreachable!("prefetch must use the batch interface")
        }
        fn get_many(&mut self, requests: &[PublicRequest], _: &Budget<'_>) -> Result<Vec<Vec<u8>>> {
            let responses = vec![b"body".to_vec(); requests.len()];
            self.0.cancel();
            Ok(responses)
        }
    }
    let cancel = Cancellation::default();
    let budget = budget(&cancel);
    let mut reader = CancelAfterRead(&cancel);
    let mut cache = CollectionReader::new(&mut reader);
    let error = cache
        .prefetch(vec![request(1), request(2)], &budget)
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert!(cache.cache.is_empty());
    assert_eq!(cache.cache_bytes, 0);
}
#[test]
fn entry_size_total_size_and_count_are_all_bounded() {
    let cancel = Cancellation::default();
    let budget = budget(&cancel);
    let mut raw = Reader::default();
    let mut cache = CollectionReader::new(&mut raw);
    cache
        .remember(&request(1), &vec![0; CACHE_MAX_ENTRY_BYTES + 1], &budget)
        .unwrap();
    assert!(cache.cache.is_empty());
    let bytes = vec![0; CACHE_MAX_ENTRY_BYTES];
    for i in 1..=40 {
        cache.remember(&request(i), &bytes, &budget).unwrap();
    }
    assert_eq!(cache.cache_bytes, CACHE_MAX_BYTES);
    assert_eq!(cache.cache.len(), CACHE_MAX_BYTES / CACHE_MAX_ENTRY_BYTES);
    drop(cache);
    let mut cache = CollectionReader::new(&mut raw);
    for i in 1..=CACHE_MAX_ENTRIES + 1 {
        cache.remember(&request(i), b"x", &budget).unwrap();
    }
    assert_eq!(cache.cache.len(), CACHE_MAX_ENTRIES);
    assert_eq!(cache.cache_bytes, CACHE_MAX_ENTRIES);
}
#[test]
fn live_inventory_and_poll_routes_are_never_cached_or_prefetched() {
    let cancel = Cancellation::default();
    let budget = budget(&cancel);
    let mut raw = Reader {
        bytes: b"{}".to_vec(),
        ..Default::default()
    };
    let mut cache = CollectionReader::new(&mut raw);
    for path in [
        "/org/pipe/builds/7.json",
        "/org/pipe/builds/7/data/steps",
        "/org/pipe/builds/7/data/jobs?include_retried_jobs=true&paginate=false",
    ] {
        let request = PublicRequest {
            path: path.into(),
            artifact_redirect: false,
            log_redirect: None,
        };
        cache.get(&request, &budget).unwrap();
        cache.get(&request, &budget).unwrap();
        assert!(cache.prefetch(vec![request], &budget).is_err());
    }
    assert_eq!(cache.reader.calls, 6);
    assert!(cache.cache.is_empty());
}
#[test]
fn parallel_metadata_batch_is_bounded_ordered_and_joined_including_failure() {
    let running = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let finished = AtomicUsize::new(0);
    let barrier = Barrier::new(PUBLIC_READ_CONCURRENCY);
    let requests: Vec<_> = (0..PUBLIC_READ_CONCURRENCY).collect();
    let result = bounded_parallel_reads(&requests, |value| {
        let active = running.fetch_add(1, Ordering::SeqCst) + 1;
        peak.fetch_max(active, Ordering::SeqCst);
        barrier.wait();
        running.fetch_sub(1, Ordering::SeqCst);
        finished.fetch_add(1, Ordering::SeqCst);
        if *value == 1 {
            Err(invalid("failed read"))
        } else {
            Ok(*value)
        }
    });
    assert!(result.is_err());
    assert_eq!(peak.load(Ordering::SeqCst), PUBLIC_READ_CONCURRENCY);
    assert_eq!(finished.load(Ordering::SeqCst), PUBLIC_READ_CONCURRENCY);
    assert_eq!(running.load(Ordering::SeqCst), 0);
    assert_eq!(
        bounded_parallel_reads(&[3, 2, 1, 0], |value| Ok(*value)).unwrap(),
        [3, 2, 1, 0]
    );
    assert!(bounded_parallel_reads(&[0; PUBLIC_READ_CONCURRENCY + 1], |_| Ok(())).is_err());
}
#[test]
fn cancellation_is_shared_by_all_joined_parallel_reads() {
    let cancel = Cancellation::default();
    let budget = budget(&cancel);
    let barrier = Barrier::new(PUBLIC_READ_CONCURRENCY);
    let finished = AtomicUsize::new(0);
    let requests: Vec<_> = (0..PUBLIC_READ_CONCURRENCY).collect();
    let result = bounded_parallel_reads(&requests, |value| {
        if *value == 0 {
            cancel.cancel();
        }
        barrier.wait();
        let result = budget.remaining();
        finished.fetch_add(1, Ordering::SeqCst);
        result
    });
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
    assert_eq!(finished.load(Ordering::SeqCst), PUBLIC_READ_CONCURRENCY);
}

#[test]
fn prefetch_chunks_and_oversized_responses_remain_bounded() {
    let cancel = Cancellation::default();
    let budget = budget(&cancel);
    let mut raw = Reader {
        bytes: b"x".to_vec(),
        ..Default::default()
    };
    let mut cache = CollectionReader::new(&mut raw);
    cache
        .prefetch((0..9).map(request).collect(), &budget)
        .unwrap();
    assert_eq!(cache.reader.batch_sizes, [8, 1]);
    assert_eq!(cache.reader.calls, 9);
    cache.reader.bytes = vec![0; budget.max_bytes + 1];
    assert!(cache.prefetch(vec![request(20)], &budget).is_err());
    assert!(!cache.cache.contains_key(&request(20).path));
}

#[test]
fn metadata_batch_never_uses_an_archive_sized_output_allowance() {
    let cancel = Cancellation::default();
    let budget = Budget {
        deadline: Instant::now() + Duration::from_secs(1),
        cancel: &cancel,
        max_bytes: 17 * 1024 * 1024,
    };
    struct DefaultReader;
    impl PublicReader for DefaultReader {
        fn get(&mut self, _: &PublicRequest, _: &Budget<'_>) -> Result<Vec<u8>> {
            panic!("oversized metadata budget must reject before transport")
        }
    }
    assert!(DefaultReader.get_many(&[request(1)], &budget).is_err());
}
