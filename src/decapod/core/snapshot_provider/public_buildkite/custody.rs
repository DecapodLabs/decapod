//! Bounded transport overlap only. Archive interpretation and source verification
//! remain on the owner thread, one logical group at a time in dependency order.
use super::*;

pub const CUSTODY_READ_CONCURRENCY: usize = 4;
/// Sum of per-request ProcessLimits allowances, NOT total capture or RSS.
/// Each allowance applies independently to stdout/stderr; HTTP body copies,
/// cache/recipes and one expanded archive have separate bounded storage.
pub const CUSTODY_PARALLEL_BYTES: usize = 128 * 1024 * 1024;

/// Created only from a selected provider artifact or inventory-bound log. Each
/// read owns its existing capture allowance; no batch-wide enlarged API budget.
#[derive(Clone, Debug)]
pub struct CustodyRead {
    pub(super) request: PublicRequest,
    max_bytes: usize,
}
impl CustodyRead {
    pub(super) fn budget<'a>(&self, shared: &Budget<'a>) -> Budget<'a> {
        Budget {
            deadline: shared.deadline,
            cancel: shared.cancel,
            max_bytes: self.max_bytes,
        }
    }
}
/// Transport-only measurement. Timing never contributes proof authority.
pub struct CustodyResponse {
    pub bytes: Vec<u8>,
    pub elapsed: Duration,
}
#[derive(Clone, Debug)]
pub struct CustodyRequest {
    archive: NamedArchiveSelection,
    log: PublicRequest,
    log_max_bytes: usize,
}
impl CustodyRequest {
    pub fn parallel_bytes(&self) -> Option<usize> {
        self.archive
            .capture_limit()
            .checked_add(self.log_max_bytes)
            .filter(|bytes| *bytes <= CUSTODY_PARALLEL_BYTES)
    }
    pub fn can_batch_with(&self, next: &Self) -> bool {
        self.parallel_bytes()
            .zip(next.parallel_bytes())
            .is_some_and(|(a, b)| a + b <= CUSTODY_PARALLEL_BYTES)
    }
    fn reads(&self) -> [CustodyRead; 2] {
        [
            CustodyRead {
                request: self.archive.request.clone(),
                max_bytes: self.archive.capture_limit(),
            },
            CustodyRead {
                request: self.log.clone(),
                max_bytes: self.log_max_bytes,
            },
        ]
    }
}

pub fn prepare_custody(
    reader: &mut impl PublicReader,
    inventory: &PublicInventory,
    recipe: &ImportedRecipe,
    name: &str,
    budget: &Budget<'_>,
) -> Result<CustodyRequest> {
    budget.remaining()?;
    Ok(CustodyRequest {
        archive: select_named_archive(reader, inventory, recipe, name, budget)?,
        log: log_request(inventory, recipe.result.job_id())?,
        log_max_bytes: budget.max_bytes,
    })
}

enum ArchiveState {
    Raw(NamedArchiveSelection, Vec<u8>),
    Verified(super::super::artifact_archive::ArtifactFiles),
}
/// Transferred bytes are not execution proof. Only into_verified interprets the
/// ZIP; callers must still run every source preparation/execution verifier.
pub struct CollectedCustody {
    archive: ArchiveState,
    log: CollectedLog,
    archive_read_ms: u128,
    log_read_ms: u128,
}
impl CollectedCustody {
    /// Sums of individual request durations may overlap; these are not wall time.
    pub fn request_milliseconds(&self) -> (u128, u128) {
        (self.archive_read_ms, self.log_read_ms)
    }
    pub fn into_verified(
        self,
        budget: &Budget<'_>,
    ) -> Result<(super::super::artifact_archive::ArtifactFiles, CollectedLog)> {
        budget.remaining()?;
        let files = match self.archive {
            ArchiveState::Raw(selected, bytes) => selected.verify(bytes, budget)?,
            ArchiveState::Verified(files) => files,
        };
        Ok((files, self.log))
    }
}
fn verified_log(request: &CustodyRequest, bytes: Vec<u8>) -> Result<CollectedLog> {
    ensure(!bytes.is_empty(), "provider job log empty or unavailable")?;
    Ok(CollectedLog {
        job_id: request.archive.job.clone(),
        digest: hash(&bytes),
        bytes,
    })
}

/// Two logical groups at most. Oversized-but-valid singleton selection takes
/// the existing serial path BEFORE any transfer, never after a failed batch.
pub fn collect_custody(
    reader: &mut impl PublicReader,
    requests: &[&CustodyRequest],
    budget: &Budget<'_>,
) -> Result<Vec<CollectedCustody>> {
    budget.remaining()?;
    ensure(
        !requests.is_empty() && requests.len() <= 2,
        "custody group batch exceeds bound",
    )?;
    if requests.len() == 1 && requests[0].parallel_bytes().is_none() {
        let request = requests[0];
        // Fully decode/drop the archive bytes before acquiring its log, exactly
        // as the original path did. Do not retain multiple expanded archives.
        let started = Instant::now();
        let files = request.archive.collect(reader, budget)?;
        let archive_read_ms = started.elapsed().as_millis();
        let started = Instant::now();
        let log = verified_log(request, read_request(reader, &request.log, budget)?)?;
        let log_read_ms = started.elapsed().as_millis();
        return Ok(vec![CollectedCustody {
            archive: ArchiveState::Verified(files),
            log,
            archive_read_ms,
            log_read_ms,
        }]);
    }
    let reads: Vec<_> = requests
        .iter()
        .flat_map(|request| request.reads())
        .collect();
    validate_reads(&reads, budget)?;
    let bytes = reader.get_custody_many(&reads, budget)?;
    validate_responses(&reads, &bytes, budget)?;
    let mut bytes = bytes.into_iter();
    let mut result = Vec::new();
    for request in requests {
        let archive = bytes
            .next()
            .ok_or_else(|| invalid("custody archive body absent"))?;
        let log = bytes
            .next()
            .ok_or_else(|| invalid("custody log body absent"))?;
        let archive_read_ms = archive.elapsed.as_millis();
        let log_read_ms = log.elapsed.as_millis();
        let log = verified_log(request, log.bytes)?;
        result.push(CollectedCustody {
            archive: ArchiveState::Raw(request.archive.clone(), archive.bytes),
            log,
            archive_read_ms,
            log_read_ms,
        });
    }
    budget.remaining()?;
    Ok(result)
}
pub(super) fn validate_reads(reads: &[CustodyRead], budget: &Budget<'_>) -> Result<()> {
    budget.remaining()?;
    ensure(
        !reads.is_empty() && reads.len() <= CUSTODY_READ_CONCURRENCY,
        "custody read batch exceeds bound",
    )?;
    let mut total = 0usize;
    for read in reads {
        let kind = read.request.authorize_read()?.kind();
        ensure(
            read.max_bytes > 0
                && match kind {
                    PublicReadKind::ArtifactContent => {
                        read.max_bytes <= 256 * 1024 * 1024 + 64 * 1024
                    }
                    PublicReadKind::JobLog => read.max_bytes <= budget.max_bytes,
                    _ => false,
                },
            "custody request output allowance invalid",
        )?;
        total = total
            .checked_add(read.max_bytes)
            .ok_or_else(|| invalid("custody read size overflow"))?;
    }
    ensure(
        total <= CUSTODY_PARALLEL_BYTES,
        "custody parallel bytes exceed bound",
    )
}
pub(super) fn validate_responses(
    reads: &[CustodyRead],
    bytes: &[CustodyResponse],
    budget: &Budget<'_>,
) -> Result<()> {
    validate_reads(reads, budget)?;
    ensure(
        reads.len() == bytes.len(),
        "custody response count mismatch",
    )?;
    for (read, bytes) in reads.iter().zip(bytes) {
        ensure(
            bytes.bytes.len() <= read.max_bytes,
            "custody response exceeded individual bound",
        )?;
    }
    budget.remaining()?;
    Ok(())
}

#[cfg(test)]
mod tests;
