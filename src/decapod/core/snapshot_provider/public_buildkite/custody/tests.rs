use super::*;

const JOB: &str = "44444444-4444-4444-4444-444444444444";
const PIPE: &str = "22222222-2222-2222-2222-222222222222";
const BUILD: &str = "11111111-1111-1111-1111-111111111111";
fn request(index: usize, size: usize) -> CustodyRequest {
    let archive_path = format!(
        "/organizations/org/pipelines/pipe/builds/7/jobs/{JOB}/artifacts/{index:08x}-7777-7777-7777-777777777777"
    );
    CustodyRequest {
        archive: NamedArchiveSelection {
            request: PublicRequest {
                path: archive_path.clone(),
                artifact_redirect: true,
                log_redirect: None,
            },
            row: serde_json::json!({"sha256sum": &hash(b"x")[7..]}),
            job: JOB.into(),
            origin: format!("fixture-{index}"),
            digest: hash(b"x"),
            size,
            file_count: 1,
        },
        log: PublicRequest {
            path: format!("/organizations/org/pipelines/pipe/builds/7/jobs/{JOB}/download.txt"),
            artifact_redirect: false,
            log_redirect: Some(LogRedirectIdentity {
                pipeline: PIPE.into(),
                build: BUILD.into(),
                job: JOB.into(),
            }),
        },
        log_max_bytes: 1024,
    }
}
fn budget(cancel: &Cancellation) -> Budget<'_> {
    Budget::new(Duration::from_secs(2), 1024, cancel).unwrap()
}
#[derive(Default)]
struct Reader {
    calls: usize,
    batches: usize,
    cancel: Option<Cancellation>,
    fail: bool,
    malformed_count: bool,
}
impl PublicReader for Reader {
    fn get(&mut self, _: &PublicRequest, _: &Budget<'_>) -> Result<Vec<u8>> {
        self.calls += 1;
        if self.fail {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"))
        } else {
            Ok(b"x".to_vec())
        }
    }
    fn get_custody_many(
        &mut self,
        reads: &[CustodyRead],
        budget: &Budget<'_>,
    ) -> Result<Vec<CustodyResponse>> {
        self.batches += 1;
        let mut responses = Vec::new();
        for read in reads {
            responses.push(CustodyResponse {
                bytes: self.get(&read.request, budget)?,
                elapsed: Duration::from_millis(7),
            });
        }
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
        }
        if self.malformed_count {
            responses.pop();
        }
        Ok(responses)
    }
}
#[test]
fn custody_bound_is_four_reads_and_128_mib_including_individual_headers() {
    let cancel = Cancellation::default();
    let budget = budget(&cancel);
    let first = request(1, 10);
    let second = request(2, 20);
    assert!(first.can_batch_with(&second));
    let reads: Vec<_> = [first.reads(), second.reads()]
        .into_iter()
        .flatten()
        .collect();
    validate_reads(&reads, &budget).unwrap();
    let mut too_many = reads.clone();
    too_many.push(first.reads()[0].clone());
    assert!(validate_reads(&too_many, &budget).is_err());
    let large = request(3, CUSTODY_PARALLEL_BYTES);
    assert!(large.parallel_bytes().is_none());
    assert!(!first.can_batch_with(&large));
    assert!(validate_reads(&large.reads(), &budget).is_err());
}
#[test]
fn each_response_keeps_its_own_limit_and_complete_count() {
    let cancel = Cancellation::default();
    let budget = budget(&cancel);
    let reads = request(1, 1).reads();
    let responses = vec![CustodyResponse {
        bytes: vec![0; 2],
        elapsed: Duration::ZERO,
    }];
    assert!(validate_responses(&reads, &responses, &budget).is_err());
    let responses = vec![
        CustodyResponse {
            bytes: vec![0; 1],
            elapsed: Duration::ZERO,
        },
        CustodyResponse {
            bytes: vec![0; 1025],
            elapsed: Duration::ZERO,
        },
    ];
    assert!(validate_responses(&reads, &responses, &budget).is_err());
}
#[test]
fn denied_batch_is_not_retried_serially() {
    let cancel = Cancellation::default();
    let budget = budget(&cancel);
    let mut reader = Reader {
        fail: true,
        ..Default::default()
    };
    let request = request(1, 1);
    let error = collect_custody(&mut reader, &[&request], &budget)
        .err()
        .unwrap();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!((reader.batches, reader.calls), (1, 1));
}
#[test]
fn cancellation_or_partial_batch_never_enters_metadata_cache() {
    for cancelled in [false, true] {
        let cancel = Cancellation::default();
        let budget = budget(&cancel);
        let mut raw = Reader {
            cancel: cancelled.then(|| cancel.clone()),
            malformed_count: !cancelled,
            ..Default::default()
        };
        let mut reader = CollectionReader::new(&mut raw);
        let request = request(1, 1);
        assert!(collect_custody(&mut reader, &[&request], &budget).is_err());
        assert_eq!(reader.measurements()["cache_entries"], 0);
        assert_eq!(reader.measurements()["cache_bytes"], 0);
        assert_eq!(reader.measurements()["public_get_requests"], 2);
    }
}
#[test]
fn archive_and_log_batch_bytes_are_not_cached_and_request_timing_is_not_wall_time() {
    let cancel = Cancellation::default();
    let budget = budget(&cancel);
    let mut raw = Reader::default();
    let mut reader = CollectionReader::new(&mut raw);
    let request = request(1, 1);
    let result = collect_custody(&mut reader, &[&request], &budget).unwrap();
    assert_eq!(result[0].request_milliseconds(), (7, 7));
    assert_eq!(reader.measurements()["cache_entries"], 0);
    assert_eq!(reader.measurements()["public_get_requests"], 2);
    assert_eq!(reader.measurements()["response_bytes"], 2);
}
#[test]
fn oversized_selection_is_serial_before_transfer_and_failure_stays_single_attempt() {
    let cancel = Cancellation::default();
    let budget = budget(&cancel);
    let mut reader = Reader {
        fail: true,
        ..Default::default()
    };
    let large = request(1, CUSTODY_PARALLEL_BYTES);
    assert!(collect_custody(&mut reader, &[&large], &budget).is_err());
    assert_eq!((reader.batches, reader.calls), (0, 1));
}
#[test]
fn expired_budget_starts_no_request() {
    let cancel = Cancellation::default();
    let budget = Budget {
        deadline: Instant::now(),
        cancel: &cancel,
        max_bytes: 1024,
    };
    let mut reader = Reader::default();
    assert_eq!(
        collect_custody(&mut reader, &[&request(1, 1)], &budget)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::TimedOut
    );
    assert_eq!((reader.calls, reader.batches), (0, 0));
}

#[test]
fn batch_transfers_do_not_decode_or_reorder_logical_groups() {
    struct RoutedReader {
        bodies: BTreeMap<String, Vec<u8>>,
        calls: usize,
    }
    impl PublicReader for RoutedReader {
        fn get(&mut self, request: &PublicRequest, _: &Budget<'_>) -> Result<Vec<u8>> {
            self.calls += 1;
            self.bodies
                .get(&request.path)
                .cloned()
                .ok_or_else(|| invalid("unknown fixture route"))
        }
    }
    let cancel = Cancellation::default();
    let budget = Budget::new(Duration::from_secs(2), 1024 * 1024, &cancel).unwrap();
    let good =
        super::super::super::artifact_archive::tests::zip("first/receipt.json", b"first", 0x8000);
    let bad = b"not a ZIP despite matching transport digest".to_vec();
    let mut first = request(1, good.len());
    let mut second = request(2, bad.len());
    first.archive.row["sha256sum"] = hash(&good)[7..].into();
    first.archive.digest = hash(&good);
    second.archive.row["sha256sum"] = hash(&bad)[7..].into();
    second.archive.digest = hash(&bad);
    let mut reader = RoutedReader {
        calls: 0,
        bodies: BTreeMap::from([
            (first.archive.request.path.clone(), good),
            (second.archive.request.path.clone(), bad),
            (first.log.path.clone(), b"provider log".to_vec()),
        ]),
    };
    let collected = collect_custody(&mut reader, &[&first, &second], &budget).unwrap();
    assert_eq!(reader.calls, 4);
    assert!(
        collected
            .iter()
            .all(|group| matches!(group.archive, ArchiveState::Raw(_, _)))
    );
    let mut groups = collected.into_iter();
    let (files, _) = groups.next().unwrap().into_verified(&budget).unwrap();
    assert_eq!(files.file("first/receipt.json").unwrap(), b"first");
    assert!(groups.next().unwrap().into_verified(&budget).is_err());
}
