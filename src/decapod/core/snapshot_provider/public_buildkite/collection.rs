//! One collection's bounded raw-byte memoization and staged recipe discovery.
//! Bytes are transport reuse only: every consumer still verifies provider hashes,
//! producer identity and its independently reviewed contract. No verdict cache.
use super::*;

const CACHE_MAX_BYTES: usize = 32 * 1024 * 1024;
const CACHE_MAX_ENTRY_BYTES: usize = 1024 * 1024;
const CACHE_MAX_ENTRIES: usize = 1024;

/// Dropped after one exact-candidate collection. Live inventories, build status
/// and logs are never cached; rechecks always reach the underlying reader.
/// Large custody ZIPs bypass the cache and retain their own existing bounds.
pub struct CollectionReader<'a, R> {
    reader: &'a mut R,
    cache: BTreeMap<String, Vec<u8>>,
    cache_bytes: usize,
    cache_hits: u64,
    requests: u64,
    response_bytes: u64,
    transport_wall: Duration,
    started: Instant,
}
impl<'a, R: PublicReader> CollectionReader<'a, R> {
    pub fn new(reader: &'a mut R) -> Self {
        Self {
            reader,
            cache: BTreeMap::new(),
            cache_bytes: 0,
            cache_hits: 0,
            requests: 0,
            response_bytes: 0,
            transport_wall: Duration::ZERO,
            started: Instant::now(),
        }
    }

    /// Counts sealed GETs submitted to the reader, excluding redirects and retries.
    /// transport_wall_ms is batch wall time, not a sum of overlapping reads.
    pub fn measurements(&self) -> Value {
        serde_json::json!({
            "elapsed_ms": self.started.elapsed().as_millis(),
            "transport_wall_ms": self.transport_wall.as_millis(),
            "public_get_requests": self.requests,
            "response_bytes": self.response_bytes,
            "cache_hits": self.cache_hits,
            "cache_entries": self.cache.len(),
            "cache_bytes": self.cache_bytes,
            "max_parallel_reads": PUBLIC_READ_CONCURRENCY,
            "cache_max_entries": CACHE_MAX_ENTRIES,
            "cache_max_bytes": CACHE_MAX_BYTES,
            "cache_max_entry_bytes": CACHE_MAX_ENTRY_BYTES,
            "request_count_scope": "sealed GETs submitted to reader; excludes redirects and timeout retries"
        })
    }

    fn cached(&mut self, request: &PublicRequest, budget: &Budget<'_>) -> Result<Option<Vec<u8>>> {
        budget.remaining()?;
        request.authorize_read()?;
        if let Some(bytes) = self.cache.get(&request.path) {
            ensure(
                bytes.len() <= budget.max_bytes,
                "cached public response exceeded bound",
            )?;
            self.cache_hits += 1;
            return Ok(Some(bytes.clone()));
        }
        Ok(None)
    }

    fn remember(
        &mut self,
        request: &PublicRequest,
        bytes: &[u8],
        budget: &Budget<'_>,
    ) -> Result<()> {
        budget.remaining()?;
        let kind = request.authorize_read()?.kind();
        ensure(
            bytes.len() <= budget.max_bytes,
            "public response exceeded bound",
        )?;
        self.remember_validated(request, bytes, kind);
        Ok(())
    }

    /// Only called after validating the whole response (or complete batch).
    /// Insertion is infallible so cancellation cannot leave a partial batch.
    fn remember_validated(&mut self, request: &PublicRequest, bytes: &[u8], kind: PublicReadKind) {
        self.response_bytes += bytes.len() as u64;
        if matches!(
            kind,
            PublicReadKind::JobArtifacts | PublicReadKind::ArtifactContent
        ) && bytes.len() <= CACHE_MAX_ENTRY_BYTES
            && self.cache.len() < CACHE_MAX_ENTRIES
            && self.cache_bytes + bytes.len() <= CACHE_MAX_BYTES
            && !self.cache.contains_key(&request.path)
        {
            self.cache_bytes += bytes.len();
            self.cache.insert(request.path.clone(), bytes.to_vec());
        }
    }

    /// Prefetch only a finite batch of typed metadata/artifact routes. Duplicate
    /// routes are read once. No failures or partial batch results are retained.
    fn prefetch(&mut self, requests: Vec<PublicRequest>, budget: &Budget<'_>) -> Result<()> {
        let mut unique = BTreeMap::new();
        for request in requests {
            budget.remaining()?;
            let kind = request.authorize_read()?.kind();
            ensure(
                matches!(
                    kind,
                    PublicReadKind::JobArtifacts | PublicReadKind::ArtifactContent
                ),
                "only collection artifacts may be prefetched",
            )?;
            if self.cached(&request, budget)?.is_none() {
                unique.entry(request.path.clone()).or_insert(request);
            }
        }
        let requests: Vec<_> = unique.into_values().collect();
        for batch in requests.chunks(PUBLIC_READ_CONCURRENCY) {
            budget.remaining()?;
            self.requests += batch.len() as u64;
            let start = Instant::now();
            let results = self.reader.get_many(batch, budget);
            self.transport_wall += start.elapsed();
            let results = results?;
            ensure(
                results.len() == batch.len(),
                "public batch response count mismatch",
            )?;
            for bytes in &results {
                ensure(
                    bytes.len() <= budget.max_bytes,
                    "public response exceeded bound",
                )?;
            }
            let kinds: Vec<_> = batch
                .iter()
                .map(|request| request.authorize_read().map(|read| read.kind()))
                .collect::<Result<_>>()?;
            budget.remaining()?;
            for ((request, bytes), kind) in batch.iter().zip(results).zip(kinds) {
                self.remember_validated(request, &bytes, kind);
            }
        }
        Ok(())
    }

    /// Discover at most four recipes at a time, then let the caller reject an
    /// unsupported contract before fetching another batch. Each phase reuses
    /// the ordinary verifier; prefetch never replaces any custody/digest check.
    pub fn locate_recipes(
        &mut self,
        inventory: &PublicInventory,
        budget: &Budget<'_>,
        mut accept: impl FnMut(RecipeLocator) -> Result<()>,
    ) -> Result<()> {
        let jobs: Vec<_> = inventory
            .jobs
            .values()
            .filter(|job| job.step_key().is_some())
            .map(|job| job.id())
            .collect();
        let started = Instant::now();
        for (index, batch) in jobs.chunks(PUBLIC_READ_CONCURRENCY).enumerate() {
            self.prefetch(
                batch
                    .iter()
                    .map(|job| listing_request(inventory, job))
                    .collect::<Result<Vec<_>>>()?,
                budget,
            )?;
            let mut result_requests = Vec::new();
            for job in batch {
                if let Some(row) = recipe_result_row(self, inventory, job, budget)? {
                    result_requests.push(content_request(&row)?);
                }
            }
            self.prefetch(result_requests, budget)?;
            let mut partials = Vec::new();
            for job in batch {
                if let Some(partial) = locate_recipe_result(self, inventory, job, budget)? {
                    partials.push(partial);
                }
            }
            self.prefetch(
                partials
                    .iter()
                    .map(|partial| listing_request(inventory, &partial.importer))
                    .collect::<Result<Vec<_>>>()?,
                budget,
            )?;
            let mut plan_requests = Vec::new();
            for partial in &partials {
                let rows = artifact_rows(self, inventory, &partial.importer, budget)?;
                let path = format!(".buildkite-gha/plans/{}.json", &partial.plan_digest[7..]);
                let selected: Vec<_> = rows.iter().filter(|row| row["path"] == path).collect();
                ensure(selected.len() == 1, "artifact missing or ambiguous")?;
                plan_requests.push(content_request(selected[0])?);
            }
            self.prefetch(plan_requests, budget)?;
            for partial in partials {
                accept(finish_recipe_locator(self, inventory, partial, budget)?)?;
            }
            eprintln!(
                "Buildkite collection: stage=recipe_discovery build={} completed={}/{} elapsed_ms={} public_get_requests={} cache_hits={}",
                inventory.build.number(),
                ((index + 1) * PUBLIC_READ_CONCURRENCY).min(jobs.len()),
                jobs.len(),
                started.elapsed().as_millis(),
                self.requests,
                self.cache_hits,
            );
        }
        Ok(())
    }
}
impl<R: PublicReader> PublicReader for CollectionReader<'_, R> {
    fn get(&mut self, request: &PublicRequest, budget: &Budget<'_>) -> Result<Vec<u8>> {
        if let Some(bytes) = self.cached(request, budget)? {
            return Ok(bytes);
        }
        self.requests += 1;
        let start = Instant::now();
        let bytes = self.reader.get(request, budget);
        self.transport_wall += start.elapsed();
        let bytes = bytes?;
        self.remember(request, &bytes, budget)?;
        Ok(bytes)
    }
    fn get_custody_many(
        &mut self,
        requests: &[CustodyRead],
        budget: &Budget<'_>,
    ) -> Result<Vec<CustodyResponse>> {
        custody::validate_reads(requests, budget)?;
        self.requests += requests.len() as u64;
        let start = Instant::now();
        let result = self.reader.get_custody_many(requests, budget);
        self.transport_wall += start.elapsed();
        let bytes = result?;
        custody::validate_responses(requests, &bytes, budget)?;
        self.response_bytes += bytes
            .iter()
            .map(|body| body.bytes.len() as u64)
            .sum::<u64>();
        // Never retain ZIPs or logs in the metadata cache.
        Ok(bytes)
    }
}
impl<R> Drop for CollectionReader<'_, R> {
    fn drop(&mut self) {
        eprintln!(
            "Buildkite collection: stage=collection_summary elapsed_ms={} transport_wall_ms={} public_get_requests={} response_bytes={} cache_hits={} cache_entries={} cache_bytes={}",
            self.started.elapsed().as_millis(),
            self.transport_wall.as_millis(),
            self.requests,
            self.response_bytes,
            self.cache_hits,
            self.cache.len(),
            self.cache_bytes,
        );
    }
}
fn listing_request(inventory: &PublicInventory, job: &str) -> Result<PublicRequest> {
    ensure(
        inventory.jobs.contains_key(job),
        "artifact producer absent from complete inventory",
    )?;
    Ok(PublicRequest {
        path: format!("{}/artifacts", inventory.build.job_base(job)?),
        artifact_redirect: false,
        log_redirect: None,
    })
}
fn content_request(row: &Value) -> Result<PublicRequest> {
    ensure(
        string(row, "state")? == "finished" && !boolean(row, "self_hosted")?,
        "unfinished/self-hosted artifact unsupported",
    )?;
    ensure(
        digest(&format!("sha256:{}", string(row, "sha256sum")?)),
        "public artifact lacks SHA256",
    )?;
    let request = PublicRequest {
        path: string(row, "url")?.to_owned(),
        artifact_redirect: true,
        log_redirect: None,
    };
    request.authorize_read()?;
    Ok(request)
}

#[cfg(test)]
mod tests;
